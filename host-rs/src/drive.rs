//! The pad's CIRCUITPY drive and the settings.json on it.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::padsettings;

const LABEL: &str = "CIRCUITPY";

#[cfg(windows)]
fn candidates() -> Vec<PathBuf> {
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW};
    use windows::Win32::System::Diagnostics::Debug::{SetErrorMode, SEM_FAILCRITICALERRORS};

    const DRIVE_REMOVABLE: u32 = 2;
    let mut found = Vec::new();
    unsafe {
        let old = SetErrorMode(SEM_FAILCRITICALERRORS); // no "insert a disk" pop-ups
        let mask = GetLogicalDrives();
        for (i, letter) in ('A'..='Z').enumerate() {
            if mask & (1 << i) == 0 {
                continue;
            }
            let root = format!("{letter}:\\");
            let wide: Vec<u16> = root.encode_utf16().chain(Some(0)).collect();
            if GetDriveTypeW(PCWSTR(wide.as_ptr())) != DRIVE_REMOVABLE {
                continue;
            }
            let mut name = [0u16; 64];
            if GetVolumeInformationW(PCWSTR(wide.as_ptr()), Some(&mut name), None, None, None, None).is_ok() {
                let end = name.iter().position(|&c| c == 0).unwrap_or(name.len());
                if String::from_utf16_lossy(&name[..end]).eq_ignore_ascii_case(LABEL) {
                    found.push(PathBuf::from(root));
                }
            }
        }
        SetErrorMode(old);
    }
    found
}

#[cfg(not(windows))]
fn candidates() -> Vec<PathBuf> {
    let user = std::env::var("USER").unwrap_or_default();
    ["/Volumes".to_string(), format!("/media/{user}"), format!("/run/media/{user}"), "/media".to_string()]
        .iter()
        .map(|base| Path::new(base).join(LABEL))
        .filter(|p| p.is_dir())
        .collect()
}

/// The CIRCUITPY drive, or None. `configured` may name a path instead of "auto".
pub fn find(configured: &str) -> Option<PathBuf> {
    if !configured.is_empty() && !configured.eq_ignore_ascii_case("auto") {
        let p = PathBuf::from(configured);
        return p.is_dir().then_some(p);
    }
    candidates().into_iter().find(|d| d.join("code.py").exists() || d.join("boot_out.txt").exists())
}

/// The pad's settings with defaults filled in, whether or not the drive is present.
pub fn read_settings(drive: Option<&Path>) -> Value {
    let raw = drive
        .and_then(|d| fs::read_to_string(d.join("settings.json")).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or(Value::Null);
    padsettings::normalise(&raw)
}

/// Writes settings.json. CircuitPython restarts the firmware when it sees the change.
pub fn write_settings(drive: &Path, settings: &Value) -> io::Result<()> {
    let clean = padsettings::normalise(settings);
    let text = serde_json::to_string_pretty(&clean).map_err(io::Error::other)? + "\n";
    let mut f = fs::File::create(drive.join("settings.json"))?;
    f.write_all(text.as_bytes())?;
    f.sync_all()
}
