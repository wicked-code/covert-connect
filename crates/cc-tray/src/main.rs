#![cfg_attr(windows, windows_subsystem = "windows")]

mod logger;

use anyhow::{Context, Result, bail};
#[cfg(target_os = "macos")]
use auto_launch::MacOSLaunchMode;
use auto_launch::{AutoLaunch, AutoLaunchBuilder};
use single_instance::SingleInstance;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{
    Arc, Condvar, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

use tao::event::{Event, StartCause};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
#[cfg(target_os = "macos")]
use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};
#[cfg(windows)]
use tray_icon::TrayIconEvent;
use tray_icon::{
    Icon, TrayIcon, TrayIconBuilder,
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
};

const APP_NAME: &str = concat!("covert-connect-tray-", env!("CARGO_PKG_VERSION"));
const SINGLE_INSTANCE_KEY: &str = "covert-connect-tray-single-instance";
const THEME_POLL_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Debug, Clone)]
enum UserEvent {
    ThemeChanged(bool),
    ExitCompleted,
    ExitFailed(String),
}

#[cfg(not(debug_assertions))]
const ICON_LIGHT: &[u8] = include_bytes!("../assets/app-icon.png");
#[cfg(debug_assertions)]
const ICON_LIGHT: &[u8] = include_bytes!("../assets/app-icon-d.png");
#[cfg(not(debug_assertions))]
const ICON_DARK: &[u8] = include_bytes!("../assets/app-icon-dark.png");
#[cfg(debug_assertions)]
const ICON_DARK: &[u8] = include_bytes!("../assets/app-icon-dark-d.png");

fn load_icon(dark: bool) -> Result<Icon> {
    let bytes = if dark { ICON_DARK } else { ICON_LIGHT };
    let img = image::load_from_memory(bytes).context("decode tray icon")?.into_rgba8();
    let (w, h) = img.dimensions();
    Icon::from_rgba(img.into_raw(), w, h).context("build tray icon")
}

fn is_dark_theme() -> bool {
    matches!(dark_light::detect(), Ok(dark_light::Mode::Dark))
}

fn ui_executable_path() -> Result<PathBuf> {
    sibling_executable_path("covert_connect")
}

fn client_executable_path() -> Result<PathBuf> {
    sibling_executable_path("cc-client")
}

fn sibling_executable_path(base_name: &str) -> Result<PathBuf> {
    let dir = std::env::current_exe()?
        .parent()
        .context("tray exe has no parent directory")?
        .to_path_buf();
    let name = if cfg!(windows) {
        format!("{base_name}.exe")
    } else {
        base_name.to_owned()
    };
    Ok(dir.join(name))
}

fn build_auto_launch() -> Result<AutoLaunch> {
    let exe = std::env::current_exe()?;
    let path_str = exe.to_str().context("exe path is not valid UTF-8")?;
    let mut builder = AutoLaunchBuilder::new();
    builder.set_app_name(APP_NAME).set_app_path(path_str);
    #[cfg(target_os = "macos")]
    builder.set_macos_launch_mode(MacOSLaunchMode::SMAppService);
    let auto = builder.build()?;
    Ok(auto)
}

fn register_autostart() -> Result<()> {
    let auto = build_auto_launch()?;
    if !auto.is_enabled().unwrap_or(false) {
        auto.enable()?;
    }
    Ok(())
}

fn unregister_autostart() -> Result<()> {
    let auto = build_auto_launch()?;
    if auto.is_enabled().unwrap_or(false) {
        auto.disable()?;
    }
    Ok(())
}

fn spawn_ui(args: &[&str]) -> Result<()> {
    let path = ui_executable_path()?;
    if !path.exists() {
        anyhow::bail!("executable not found at {}", path.display());
    }
    Command::new(&path)
        .args(args)
        .spawn()
        .with_context(|| format!("failed to launch executable at {}", path.display()))?;
    Ok(())
}

fn spawn_api(args: &[&str]) -> Result<()> {
    let path = client_executable_path()?;
    if !path.exists() {
        anyhow::bail!("executable not found at {}", path.display());
    }
    let status = Command::new(&path)
        .args(args)
        .status()
        .with_context(|| format!("failed to launch executable at {}", path.display()))?;
    if !status.success() {
        bail!("executable at {} exited with status {}", path.display(), status);
    }

    Ok(())
}

fn show_error_dialog(message: &str) {
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title("Covert Connect")
        .set_description(message)
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}

fn instance_id() -> String {
    #[cfg(target_os = "macos")]
    {
        std::env::temp_dir()
            .join(SINGLE_INSTANCE_KEY)
            .to_string_lossy()
            .to_string()
    }
    #[cfg(not(target_os = "macos"))]
    SINGLE_INSTANCE_KEY.to_owned()
}

fn main() -> Result<()> {
    let instance = SingleInstance::new(&instance_id()).context("create single-instance guard")?;
    if !instance.is_single() {
        return Ok(());
    }

    logger::init();
    if let Err(e) = register_autostart() {
        log::warn!("failed to register autostart: {e:?}");
    }

    #[cfg_attr(not(target_os = "macos"), allow(unused_mut))]
    let mut event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    #[cfg(target_os = "macos")]
    event_loop.set_activation_policy(ActivationPolicy::Accessory);

    // Build the menu
    let menu = Menu::new();
    let show_item = MenuItem::new("Show", true, None);
    let exit_item = MenuItem::new("Exit", true, None);
    menu.append(&show_item)?;
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&exit_item)?;

    let show_id = show_item.id().clone();
    let exit_id = exit_item.id().clone();

    let initial_dark = is_dark_theme();

    let proxy = event_loop.create_proxy();
    let running = Arc::new(AtomicBool::new(true));
    let exiting = Arc::new(AtomicBool::new(false));
    let theme_shutdown = Arc::new((Mutex::new(()), Condvar::new()));

    let theme_proxy = proxy.clone();
    let theme_running = Arc::clone(&running);
    let theme_shutdown_thread = Arc::clone(&theme_shutdown);
    std::thread::spawn(move || {
        let mut last_dark = initial_dark;
        let (lock, cvar) = &*theme_shutdown_thread;

        while theme_running.load(Ordering::Relaxed) {
            let guard = match lock.lock() {
                Ok(g) => g,
                Err(_) => break,
            };
            let (_g, _res) = match cvar.wait_timeout(guard, THEME_POLL_INTERVAL) {
                Ok(pair) => pair,
                Err(_) => break,
            };
            if !theme_running.load(Ordering::Relaxed) {
                break;
            }

            let dark = is_dark_theme();
            if dark != last_dark {
                last_dark = dark;
                let _ = theme_proxy.send_event(UserEvent::ThemeChanged(dark));
            }
        }
    });

    let signal_theme_shutdown = {
        let theme_shutdown = Arc::clone(&theme_shutdown);
        let running = Arc::clone(&running);
        move || {
            running.store(false, Ordering::Relaxed);
            theme_shutdown.1.notify_all();
        }
    };

    let show_id_for_handler = show_id.clone();
    let exit_id_for_handler = exit_id.clone();
    let exit_proxy = proxy.clone();
    let exiting_for_handler = Arc::clone(&exiting);
    MenuEvent::set_event_handler(Some(move |ev: MenuEvent| {
        if ev.id == show_id_for_handler {
            std::thread::spawn(move || {
                if let Err(e) = spawn_ui(&["/show"]) {
                    log::error!("failed to launch UI: {e:?}");
                }
            });
        } else if ev.id == exit_id_for_handler {
            if exiting_for_handler.swap(true, Ordering::SeqCst) {
                return;
            }
            let exit_proxy = exit_proxy.clone();
            std::thread::spawn(move || {
                if let Err(e) = unregister_autostart() {
                    log::warn!("failed to unregister autostart: {e:?}");
                }
                if let Err(e) = spawn_ui(&["/exit"]) {
                    log::warn!("failed to send /exit to UI: {e:?}");
                }
                match spawn_api(&["uninstall"]) {
                    Ok(_) => {
                        let _ = exit_proxy.send_event(UserEvent::ExitCompleted);
                    }
                    Err(e) => {
                        log::error!("failed to exit client: {e:?}");
                        let _ = exit_proxy.send_event(UserEvent::ExitFailed(format!("{e:#}")));
                    }
                }
            });
        }
    }));

    #[cfg(windows)]
    {
        TrayIconEvent::set_event_handler(Some(move |ev: TrayIconEvent| {
            if let TrayIconEvent::Click {
                button: tray_icon::MouseButton::Left,
                button_state: tray_icon::MouseButtonState::Down,
                ..
            } = ev
            {
                std::thread::spawn(move || {
                    if let Err(e) = spawn_ui(&["/show"]) {
                        log::error!("failed to launch UI: {e:?}");
                    }
                });
            }
        }));
    }

    let mut tray: Option<TrayIcon> = None;
    let mut menu_holder = Some(menu);

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::NewEvents(StartCause::Init) => {
                let menu = match menu_holder.take() {
                    Some(m) => m,
                    None => return,
                };
                match load_icon(initial_dark) {
                    Ok(icon) => {
                        let mut builder = TrayIconBuilder::new()
                            .with_menu(Box::new(menu))
                            .with_tooltip("Covert Connect")
                            .with_icon(icon);
                        if cfg!(windows) {
                            builder = builder.with_menu_on_left_click(false);
                        }
                        match builder.build() {
                            Ok(t) => tray = Some(t),
                            Err(e) => {
                                log::error!("failed to build tray: {e:?}");
                                signal_theme_shutdown();
                                *control_flow = ControlFlow::Exit;
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("failed to load tray icon: {e:?}");
                        signal_theme_shutdown();
                        *control_flow = ControlFlow::Exit;
                    }
                }
            }
            Event::UserEvent(UserEvent::ThemeChanged(dark)) => {
                if let Some(t) = tray.as_ref() {
                    match load_icon(dark) {
                        Ok(icon) => {
                            if let Err(e) = t.set_icon(Some(icon)) {
                                log::warn!("failed to update tray icon: {e:?}");
                            }
                        }
                        Err(e) => log::warn!("failed to load themed icon: {e:?}"),
                    }
                }
            }
            Event::UserEvent(UserEvent::ExitCompleted) => {
                signal_theme_shutdown();
                *control_flow = ControlFlow::Exit;
            }
            Event::UserEvent(UserEvent::ExitFailed(msg)) => {
                exiting.store(false, Ordering::SeqCst);
                show_error_dialog(&format!("Failed to exit client.\n\n{msg}"));
            }
            _ => {}
        }
    });
}
