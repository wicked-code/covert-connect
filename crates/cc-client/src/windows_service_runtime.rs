use std::{
    ffi::OsString,
    path::PathBuf,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use anyhow::{Result, anyhow};
use tokio::sync::oneshot;
use windows_service::{
    define_windows_service,
    service::{ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus, ServiceType},
    service_control_handler::{self, ServiceControlHandlerResult, ServiceStatusHandle},
    service_dispatcher,
};

use crate::client_controller::SERVICE_NAME;

static SERVICE_CFG_PATH: OnceLock<PathBuf> = OnceLock::new();

pub fn run_as_windows_service_if_needed(cfg_path: PathBuf) -> Result<bool> {
    if SERVICE_CFG_PATH.get().is_none() {
        let _ = SERVICE_CFG_PATH.set(cfg_path);
    }

    match service_dispatcher::start(SERVICE_NAME, ffi_service_main) {
        Ok(()) => Ok(true),
        Err(windows_service::Error::Winapi(io_err))
            if io_err.raw_os_error()
                == Some(windows::Win32::Foundation::ERROR_FAILED_SERVICE_CONTROLLER_CONNECT.0 as i32) =>
        {
            Ok(false)
        }
        Err(err) => Err(anyhow!("Failed to connect to Windows service dispatcher: {err}")),
    }
}

define_windows_service!(ffi_service_main, windows_service_main);

fn windows_service_main(_arguments: Vec<OsString>) {
    if let Err(err) = windows_service_entry() {
        eprintln!("Windows service error: {err}");
    }
}

fn windows_service_entry() -> Result<()> {
    let (stop_tx, stop_rx) = oneshot::channel();
    let stop_tx = Arc::new(Mutex::new(Some(stop_tx)));
    let stopping = Arc::new(AtomicBool::new(false));
    let status_handle_slot = Arc::new(Mutex::new(None::<ServiceStatusHandle>));

    let stop_tx_for_handler = Arc::clone(&stop_tx);
    let stopping_for_handler = Arc::clone(&stopping);
    let status_handle_slot_for_handler = Arc::clone(&status_handle_slot);
    let status_handle = service_control_handler::register(SERVICE_NAME, move |control_event| match control_event {
        ServiceControl::Stop | ServiceControl::Shutdown => {
            stopping_for_handler.store(true, Ordering::SeqCst);
            if let Ok(status_guard) = status_handle_slot_for_handler.lock() {
                if let Some(status_handle) = *status_guard {
                    let _ = status_handle.set_service_status(stop_pending_status(2));
                }
            }
            if let Ok(mut tx_guard) = stop_tx_for_handler.lock() {
                if let Some(tx) = tx_guard.take() {
                    let _ = tx.send(());
                }
            }
            ServiceControlHandlerResult::NoError
        }
        ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
        _ => ServiceControlHandlerResult::NotImplemented,
    })?;

    {
        let mut status_guard = status_handle_slot
            .lock()
            .map_err(|_| anyhow!("Service status handle lock poisoned"))?;
        *status_guard = Some(status_handle);
    }

    status_handle.set_service_status(start_pending_status())?;

    let cfg_path = match SERVICE_CFG_PATH.get() {
        Some(path) => path.clone(),
        None => crate::find_config_path()?.join("config.toml"),
    };

    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;

    let stopping_for_started = Arc::clone(&stopping);
    let on_started = Box::new(move || {
        if stopping_for_started.load(Ordering::SeqCst) {
            status_handle.set_service_status(stop_pending_status(2))?;
        } else {
            status_handle.set_service_status(running_status())?;
        }
        Ok(())
    });
    let run_result = runtime.block_on(crate::start_client(cfg_path, Some(stop_rx), Some(on_started)));

    let (exit_code, state) = match run_result {
        Ok(_) => (ServiceExitCode::Win32(0), ServiceState::Stopped),
        Err(err) => {
            eprintln!("Service run failure: {err}");
            (ServiceExitCode::Win32(1), ServiceState::Stopped)
        }
    };

    status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: state,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code,
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    })?;

    Ok(())
}

fn start_pending_status() -> ServiceStatus {
    ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::StartPending,
        controls_accepted: ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN,
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 1,
        wait_hint: Duration::from_secs(30),
        process_id: None,
    }
}

fn running_status() -> ServiceStatus {
    ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Running,
        controls_accepted: ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN,
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    }
}

fn stop_pending_status(checkpoint: u32) -> ServiceStatus {
    ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::StopPending,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: ServiceExitCode::Win32(0),
        checkpoint,
        wait_hint: Duration::from_secs(30),
        process_id: None,
    }
}
