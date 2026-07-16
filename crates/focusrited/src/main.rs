//! Read-only Focusrite controller daemon bootstrap.

use std::{env, process, thread, time::Duration};

use focusrited::{
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
    let worker = DeviceWorker::start_with_profiles(Scarlett2Alsa::new(card), profiles)
        .unwrap_or_else(|error| fail(error));
    let mut online = true;

    loop {
        thread::sleep(Duration::from_secs(1));
        match worker.refresh() {
            Ok(state) if !online => {
                eprintln!(
                    "focusrited: device recovered at revision {}",
                    state.revision
                );
                online = true;
            }
            Ok(_) => {}
            Err(error) if online => {
                eprintln!("focusrited: device offline: {error}");
                online = false;
            }
            Err(_) => {}
        }
    }
}

fn fail(error: impl std::fmt::Display) -> ! {
    eprintln!("focusrited: {error}");
    process::exit(2);
}
