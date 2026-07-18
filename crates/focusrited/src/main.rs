//! Read-only Focusrite controller daemon bootstrap.

use std::{env, process, sync::Arc, thread, time::Duration};

use focusrited::{
    ipc::LocalServer,
    scarlett2_alsa::Scarlett2Alsa,
    startup::{Config, ConfigError, load_profiles},
    worker::DeviceWorker,
};

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
    let worker = Arc::new(
        DeviceWorker::start_with_profiles(Scarlett2Alsa::new(card), profiles)
            .unwrap_or_else(|error| fail(error)),
    );
    let _ipc = LocalServer::start(Arc::clone(&worker), config.socket_path)
        .unwrap_or_else(|error| fail(error));
    let mut online = true;

    loop {
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
}

fn fail(error: impl std::fmt::Display) -> ! {
    eprintln!("focusrited: {error}");
    process::exit(2);
}
