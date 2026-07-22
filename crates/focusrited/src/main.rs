//! Read-only Focusrite controller daemon bootstrap.

use std::{
    env, io, process,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use focusrited::{
    Device,
    dashboard_store::{DashboardConfig, DashboardStore},
    ipc::LocalServer,
    profile_store::ProfileStore,
    scarlett2_alsa::Scarlett2Alsa,
    startup::{Config, ConfigError, DashboardAction, load_dashboard, load_profiles},
    worker::DeviceWorker,
};

static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);

fn main() {
    let config = match Config::from_args(env::args().skip(1)) {
        Ok(config) => config,
        Err(ConfigError::Help) => {
            println!("{}", ConfigError::Help);
            return;
        }
        Err(error) => fail(error),
    };
    let card = config
        .card
        .as_deref()
        .unwrap_or_else(|| fail("--card is required"));
    if config.dashboard_action.is_some() {
        dashboard_action(card, &config).unwrap_or_else(|error| fail(error));
        return;
    }
    let profiles = load_profiles(&config).unwrap_or_else(|error| fail(error));
    let dashboard = load_dashboard(&config).unwrap_or_else(|error| fail(error));
    let worker = Arc::new(
        DeviceWorker::start_with_dashboard_and_profile_store(
            Scarlett2Alsa::new(card),
            profiles,
            dashboard,
            Some(ProfileStore::new(&config.profile_store_path)),
        )
        .unwrap_or_else(|error| fail(error)),
    );
    install_shutdown_handler().unwrap_or_else(|error| fail(error));
    let ipc = LocalServer::start(Arc::clone(&worker), config.socket_path)
        .unwrap_or_else(|error| fail(error));
    let mut online = true;

    while !SHUTDOWN_REQUESTED.load(Ordering::Relaxed) {
        thread::sleep(Duration::from_secs(1));
        match worker.state() {
            Ok(state) if state.online && !online => {
                eprintln!(
                    "focusrited: device recovered at revision {}",
                    state.revision
                );
                online = true;
            }
            Ok(state) if !state.online && online => {
                eprintln!("focusrited: device offline");
                online = false;
            }
            Err(_) => {}
            Ok(_) => {}
        }
    }
    ipc.stop();
    let _ = worker.stop();
}

fn dashboard_action(card: &str, config: &Config) -> io::Result<()> {
    let mut device = Scarlett2Alsa::new(card);
    let snapshot = device
        .snapshot()
        .map_err(|error| io::Error::other(format!("read-only discovery failed: {error:?}")))?;
    let store = DashboardStore::new(&config.dashboard_store_path);
    match config.dashboard_action.as_ref().expect("checked by caller") {
        DashboardAction::Inspect => {
            let dashboard = store
                .load()?
                .unwrap_or_else(|| DashboardConfig::defaults(&snapshot));
            dashboard.validate_for(&snapshot)?;
            serde_json::to_writer_pretty(io::stdout(), &dashboard).map_err(io::Error::other)?;
            println!();
        }
        DashboardAction::Export(path) => {
            let dashboard = store
                .load()?
                .unwrap_or_else(|| DashboardConfig::defaults(&snapshot));
            DashboardStore::new(path).save(&dashboard, &snapshot)?;
        }
        DashboardAction::Import(path) => {
            let dashboard = DashboardStore::new(path).load_required()?;
            store.save(&dashboard, &snapshot)?;
        }
    }
    Ok(())
}

extern "C" fn request_shutdown(_: libc::c_int) {
    SHUTDOWN_REQUESTED.store(true, Ordering::Relaxed);
}

fn install_shutdown_handler() -> io::Result<()> {
    for signal in [libc::SIGTERM, libc::SIGINT] {
        // Handler only stores an atomic flag; cleanup runs in normal process context.
        if unsafe { libc::signal(signal, request_shutdown as *const () as libc::sighandler_t) }
            == libc::SIG_ERR
        {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

fn fail(error: impl std::fmt::Display) -> ! {
    eprintln!("focusrited: {error}");
    process::exit(2);
}
