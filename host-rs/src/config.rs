//! config.toml: PC-side settings. Same file format as the Python host app, so both can share it.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use serde::Deserialize;

use crate::protocol::KEY_IDS;

#[derive(Debug, Clone, PartialEq)]
pub struct Channel {
    pub label: String,
    pub apps: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub port: String,
    pub step_percent: f64,
    pub channels: Vec<Channel>,
    /// key id ("1"-"8", "master") -> program, file, folder or URL to open
    pub pc_actions: Vec<(String, String)>,
    pub web_port: u16,
    pub pad_drive: String,
}

impl Default for Config {
    fn default() -> Self {
        let ch = |label: &str, apps: &[&str]| Channel {
            label: label.into(),
            apps: apps.iter().map(|a| a.to_string()).collect(),
        };
        Config {
            port: "auto".into(),
            step_percent: 2.0,
            channels: vec![
                ch("Game", &["cs2.exe"]),
                ch("Discord", &["discord.exe"]),
                ch("Browser", &["chrome.exe", "firefox.exe", "msedge.exe"]),
                ch("Music", &["spotify.exe"]),
            ],
            pc_actions: Vec::new(),
            web_port: 8765,
            pad_drive: "auto".into(),
        }
    }
}

impl Config {
    pub fn pc_action(&self, key: &str) -> Option<&str> {
        self.pc_actions.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }
}

#[derive(Deserialize)]
struct RawChannel {
    label: Option<String>,
    #[serde(default)]
    apps: Vec<String>,
}

#[derive(Deserialize)]
struct RawConfig {
    port: Option<String>,
    step_percent: Option<f64>,
    web_port: Option<u16>,
    pad_drive: Option<String>,
    #[serde(default)]
    channel: Vec<RawChannel>,
    #[serde(default)]
    pc_actions: toml::Table,
}

pub fn parse(text: &str) -> Result<Config, String> {
    let raw: RawConfig = toml::from_str(text).map_err(|e| e.to_string())?;
    if raw.channel.len() > 4 {
        return Err("config.toml defines more than 4 [[channel]] blocks".into());
    }
    let mut channels: Vec<Channel> = raw
        .channel
        .into_iter()
        .enumerate()
        .map(|(i, c)| Channel { label: c.label.unwrap_or_else(|| format!("App {}", i + 1)), apps: c.apps })
        .collect();
    while channels.len() < 4 {
        channels.push(Channel { label: format!("App {}", channels.len() + 1), apps: vec![] });
    }
    let pc_actions = KEY_IDS
        .iter()
        .filter_map(|k| {
            let v = raw.pc_actions.get(*k)?.as_str()?.trim();
            (!v.is_empty()).then(|| (k.to_string(), v.to_string()))
        })
        .collect();
    Ok(Config {
        port: raw.port.unwrap_or_else(|| "auto".into()),
        step_percent: raw.step_percent.unwrap_or(2.0),
        channels,
        pc_actions,
        web_port: raw.web_port.unwrap_or(8765),
        pad_drive: raw.pad_drive.unwrap_or_else(|| "auto".into()),
    })
}

pub fn load(path: &Path) -> Result<Config, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    parse(&text)
}

fn toml_str(value: &str) -> String {
    let mut out = String::from("\"");
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn num(v: f64) -> String {
    if v.fract() == 0.0 { format!("{}", v as i64) } else { format!("{v}") }
}

/// config.toml text for a Config (same layout and comments as the Python app writes).
pub fn dump(cfg: &Config) -> String {
    let mut lines = vec![
        "# Pico Mixpad host app configuration.".to_string(),
        "# Edit here or in the settings page: http://localhost:8765 while the app runs.".into(),
        String::new(),
        "# COM port of the macropad's DATA port, e.g. \"COM7\". \"auto\" finds it by handshake.".into(),
        format!("port = {}", toml_str(&cfg.port)),
        String::new(),
        "# Volume change per knob click, in percent.".into(),
        format!("step_percent = {}", num(cfg.step_percent)),
        String::new(),
        "# Settings page: http://localhost:<web_port>. pad_drive: the CIRCUITPY drive, or \"auto\".".into(),
        format!("web_port = {}", cfg.web_port),
        format!("pad_drive = {}", toml_str(&cfg.pad_drive)),
        String::new(),
        "# Four app knobs, left to right. \"label\" is shown on the OLED (first 7 characters).".into(),
        "# \"apps\" lists process names as shown in Task Manager > Details, plus these specials:".into(),
        "#   \"@focused\"  the app in the foreground window".into(),
        "#   \"@system\"   Windows system sounds".into(),
    ];
    for ch in &cfg.channels {
        let apps: Vec<String> = ch.apps.iter().map(|a| toml_str(a)).collect();
        lines.push(String::new());
        lines.push("[[channel]]".into());
        lines.push(format!("label = {}", toml_str(&ch.label)));
        lines.push(format!("apps = [{}]", apps.join(", ")));
    }
    lines.push(String::new());
    lines.push("# What keys set to \"Open on PC\" open: a program, file, folder or URL.".into());
    lines.push("# Keys \"1\"-\"8\", or \"master\" for the master knob push.".into());
    lines.push("[pc_actions]".into());
    for key in KEY_IDS {
        if let Some(target) = cfg.pc_action(key) {
            lines.push(format!("{} = {}", toml_str(key), toml_str(target)));
        }
    }
    lines.join("\n") + "\n"
}

/// Writes atomically, so the host loop never reads a half-written file.
pub fn save(path: &Path, cfg: &Config) -> io::Result<()> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, dump(cfg))?;
    fs::rename(&tmp, path)
}

/// Reloads config.toml when it changes (e.g. saved from the settings page).
pub struct Watcher {
    path: PathBuf,
    mtime: Option<SystemTime>,
    checked: Instant,
}

impl Watcher {
    pub fn new(path: &Path) -> Self {
        Watcher { path: path.to_path_buf(), mtime: mtime(path), checked: Instant::now() }
    }

    /// The new Config if the file changed since the last change seen; checks at most once a second.
    pub fn poll(&mut self) -> Option<Config> {
        if self.checked.elapsed() < Duration::from_secs(1) {
            return None;
        }
        self.checked = Instant::now();
        let now = mtime(&self.path);
        if now == self.mtime {
            return None;
        }
        self.mtime = now;
        match load(&self.path) {
            Ok(cfg) => {
                log::info!("config.toml reloaded");
                Some(cfg)
            }
            Err(e) => {
                log::warn!("config.toml not reloaded: {e}");
                None
            }
        }
    }
}

fn mtime(path: &Path) -> Option<SystemTime> {
    fs::metadata(path).and_then(|m| m.modified()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_the_file_format() {
        let cfg = Config {
            port: "COM7".into(),
            step_percent: 2.5,
            web_port: 9000,
            pad_drive: "E:\\".into(),
            channels: vec![
                Channel { label: "Ga\"me".into(), apps: vec!["cs2.exe".into(), "@focused".into()] },
                Channel { label: "B".into(), apps: vec![] },
                Channel { label: "C".into(), apps: vec!["x.exe".into()] },
                Channel { label: "D".into(), apps: vec!["@system".into()] },
            ],
            pc_actions: vec![
                ("2".into(), r"C:\Program Files\Spotify\Spotify.exe".into()),
                ("master".into(), "https://example.com".into()),
            ],
        };
        assert_eq!(parse(&dump(&cfg)).unwrap(), cfg);
    }

    #[test]
    fn reads_the_python_apps_config() {
        let py = include_str!("../../host/config.toml");
        let cfg = parse(py).unwrap();
        assert_eq!(cfg.channels.len(), 4);
        assert_eq!(dump(&cfg), py, "the Rust app must write config.toml exactly like the Python app");
    }

    #[test]
    fn watcher_reloads_changes() {
        let path = std::env::temp_dir().join(format!("mixpad-watch-{}.toml", std::process::id()));
        save(&path, &Config::default()).unwrap();
        let mut w = Watcher::new(&path);
        let mut changed = Config::default();
        changed.step_percent = 7.0;
        save(&path, &changed).unwrap();
        w.mtime = None; // filesystems can have coarse timestamps
        w.checked = Instant::now() - Duration::from_secs(2);
        assert_eq!(w.poll().map(|c| c.step_percent), Some(7.0));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn fills_missing_values() {
        let cfg = parse("[[channel]]\napps = [\"a.exe\"]\n").unwrap();
        assert_eq!(cfg.port, "auto");
        assert_eq!(cfg.channels[0].label, "App 1");
        assert_eq!(cfg.channels[3].label, "App 4");
        assert!(parse("[[channel]]\n[[channel]]\n[[channel]]\n[[channel]]\n[[channel]]\n").is_err());
    }
}
