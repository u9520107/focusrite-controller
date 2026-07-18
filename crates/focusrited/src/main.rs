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
    ipc::LocalServer,
    scarlett2_alsa::Scarlett2Alsa,
    startup::{Config, ConfigError, load_dashboard, load_profiles},
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
    let profiles = load_profiles(&config).unwrap_or_else(|error| fail(error));
    let dashboard = load_dashboard(&config).unwrap_or_else(|error| fail(error));
    let worker = Arc::new(
        DeviceWorker::start_with_dashboard(Scarlett2Alsa::new(card), profiles, dashboard)
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
