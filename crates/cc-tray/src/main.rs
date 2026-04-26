#![cfg_attr(all(not(debug_assertions), windows), windows_subsystem = "windows")]

mod logger;

use anyhow::{Context, Result};
use auto_launch::{AutoLaunch, AutoLaunchBuilder};
#[cfg(target_os = "macos")]
use auto_launch::MacOSLaunchMode;
use single_instance::SingleInstance;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use tao::event::{Event, StartCause};
use tao::event_loop::{ControlFlow, EventLoop};
use tray_icon::{
    Icon, TrayIcon, TrayIconBuilder, TrayIconEvent,
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
};

const APP_NAME: &str = concat!("covert-connect-tray-", env!("CARGO_PKG_VERSION"));
const SINGLE_INSTANCE_KEY: &str = "covert-connect-tray-single-instance";
const THEME_POLL_INTERVAL: Duration = Duration::from_secs(2);

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
    let dir = std::env::current_exe()?
        .parent()
        .context("tray exe has no parent directory")?
        .to_path_buf();
    let name = if cfg!(windows) {
        "covert_connect.exe"
    } else {
        "covert_connect"
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
        anyhow::bail!("UI executable not found at {}", path.display());
    }
    Command::new(&path)
        .args(args)
        .spawn()
        .with_context(|| format!("failed to launch UI at {}", path.display()))?;
    Ok(())
}

fn main() -> Result<()> {
    logger::init();

    let instance = SingleInstance::new(SINGLE_INSTANCE_KEY).context("create single-instance guard")?;
    if !instance.is_single() {
        log::info!("another tray instance is already running, exiting");
        return Ok(());
    }

    if let Err(e) = register_autostart() {
        log::warn!("failed to register autostart: {e:?}");
    }

    let event_loop = EventLoop::new();

    // Build the menu
    let menu = Menu::new();
    let show_item = MenuItem::new("Show", true, None);
    let exit_item = MenuItem::new("Exit", true, None);
    menu.append(&show_item)?;
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&exit_item)?;

    let show_id = show_item.id().clone();
    let exit_id = exit_item.id().clone();

    let menu_channel = MenuEvent::receiver();

    let mut tray: Option<TrayIcon> = None;
    let mut last_dark = is_dark_theme();
    let mut menu_holder = Some(menu);

    event_loop.run(move |event, _, control_flow| {
        // Wake periodically to poll the OS theme so the icon can follow it.
        *control_flow = ControlFlow::WaitUntil(Instant::now() + THEME_POLL_INTERVAL);

        match event {
            Event::NewEvents(StartCause::Init) => {
                let menu = match menu_holder.take() {
                    Some(m) => m,
                    None => return,
                };
                match load_icon(last_dark) {
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
                                *control_flow = ControlFlow::Exit;
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("failed to load tray icon: {e:?}");
                        *control_flow = ControlFlow::Exit;
                    }
                }
            }
            Event::NewEvents(StartCause::ResumeTimeReached { .. }) => {
                let dark = is_dark_theme();
                if dark != last_dark {
                    last_dark = dark;
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
            }
            _ => {}
        }

        while let Ok(ev) = menu_channel.try_recv() {
            if ev.id == show_id {
                if let Err(e) = spawn_ui(&["/show"]) {
                    log::error!("failed to launch UI: {e:?}");
                }
            } else if ev.id == exit_id {
                if let Err(e) = unregister_autostart() {
                    log::warn!("failed to unregister autostart: {e:?}");
                }
                if let Err(e) = spawn_ui(&["/exit"]) {
                    log::warn!("failed to send /exit to UI: {e:?}");
                }
                *control_flow = ControlFlow::Exit;
            }
        }

        // On Windows, a left-click should show main window
        if cfg!(windows) {
            while let Ok(tray_ev) = TrayIconEvent::receiver().try_recv() {
                if let TrayIconEvent::Click {
                    button: tray_icon::MouseButton::Left,
                    button_state: tray_icon::MouseButtonState::Up,
                    ..
                } = tray_ev
                {
                    if let Err(e) = spawn_ui(&["/show"]) {
                        log::error!("failed to launch UI: {e:?}");
                    }
                }
            }
        }
    });
}
