//! Pico Mixpad host app.
//!
//! Connects to the macropad, turns knob steps into per-app volume changes, toggles the
//! microphone, runs "Open on PC" keys, sends the state to the OLED, and serves the settings
//! page on http://localhost:8765. On Windows it lives in the notification area (tray):
//! left-click opens the settings page, right-click has Quit.
//!
//!   pico-mixpad.exe              start (tray icon, no window)
//!   pico-mixpad.exe --settings   start and open the settings page
//!   pico-mixpad.exe --no-tray    run without the tray icon (stop with Ctrl+C)

// Release builds are a GUI program on Windows: no console window when double-clicked.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod actions;
mod audio;
mod bridge;
mod config;
mod drive;
mod logging;
mod padsettings;
mod protocol;
mod serial;
mod status;
mod web;

#[cfg(windows)]
mod platform;
#[cfg(windows)]
mod tray;

use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use audio::Audio;
use bridge::Bridge;
use status::Shared;

const USAGE: &str = "Pico Mixpad host app

  pico-mixpad              start (tray icon on Windows)
  pico-mixpad --settings   start and open the settings page
  pico-mixpad --no-tray    run without the tray icon (stop with Ctrl+C)

config.toml and macropad_host.log are kept next to the program.";

fn app_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn make_audio() -> Box<dyn Audio> {
    #[cfg(windows)]
    match audio::WindowsAudio::new() {
        Ok(a) => return Box::new(a),
        Err(e) => log::error!("Windows audio unavailable: {e}"),
    }
    Box::new(audio::NoAudio)
}

/// Finds the pad, runs the connection, and starts over when it is unplugged. Never returns.
fn run(config_path: PathBuf, status: Shared) {
    let mut audio = make_audio();
    let mut watcher = config::Watcher::new(&config_path);
    let mut cfg = config::load(&config_path).unwrap_or_default();
    loop {
        if let Some(new) = watcher.poll() {
            cfg = new;
        }
        let Some((port, version)) = serial::find_port(&cfg.port) else {
            status.lock().connected = false;
            thread::sleep(Duration::from_secs(3));
            continue;
        };
        let link = match serial::SerialLink::open(&port, serial::READ_TIMEOUT) {
            Ok(l) => l,
            Err(e) => {
                log::warn!("could not open {port}: {e}");
                thread::sleep(Duration::from_secs(2));
                continue;
            }
        };
        log::info!("connected on {port}");
        {
            let mut st = status.lock();
            st.connected = true;
            st.port = Some(port.clone());
            st.firmware = version;
        }
        let mut b = Bridge::new(link, audio.as_mut(), cfg.clone(), status.clone(), Box::new(|t: &str| actions::open_target(t)));
        loop {
            if let Err(e) = b.step() {
                log::warn!("connection lost: {e}");
                break;
            }
            if let Some(new) = watcher.poll() {
                b.config = new.clone();
                b.dirty = true;
                cfg = new;
            }
        }
        drop(b);
        {
            let mut st = status.lock();
            st.connected = false;
            st.port = None;
        }
        thread::sleep(Duration::from_secs(2));
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |flag: &str| args.iter().any(|a| a == flag);
    #[cfg(windows)]
    let console = platform::attach_console() || cfg!(debug_assertions);
    #[cfg(not(windows))]
    let console = true;
    if has("--help") || has("-h") {
        println!("{USAGE}");
        return;
    }

    let dir = app_dir();
    logging::init(&dir.join("macropad_host.log"), console);
    let config_path = dir.join("config.toml");
    if !config_path.exists() {
        if let Err(e) = config::save(&config_path, &config::Config::default()) {
            log::error!("could not create {}: {e}", config_path.display());
        }
    }
    let cfg = match config::load(&config_path) {
        Ok(c) => c,
        Err(e) => {
            log::error!("{e}; using defaults until config.toml is fixed");
            config::Config::default()
        }
    };
    let url = format!("http://localhost:{}", cfg.web_port);

    #[cfg(windows)]
    if !platform::single_instance() {
        // Already running: just show its settings page.
        actions::open_target(&url);
        return;
    }

    log::info!("starting, channels: {}", cfg.channels.iter().map(|c| c.label.as_str()).collect::<Vec<_>>().join(", "));
    let status = Shared::default();
    match web::start(web::WebState { config_path: config_path.clone(), status: status.clone() }, cfg.web_port) {
        Ok(_) => log::info!("settings page: {url}"),
        Err(e) => log::error!("{e}"),
    }
    if has("--settings") {
        actions::open_target(&url);
    }

    let host = {
        let status = status.clone();
        thread::Builder::new().name("pad".into()).spawn(move || run(config_path, status)).expect("start pad thread")
    };

    #[cfg(windows)]
    if !has("--no-tray") {
        tray::run(status, &url); // returns when Quit is chosen
        log::info!("quit from the tray");
        std::process::exit(0);
    }
    let _ = host.join();
}
