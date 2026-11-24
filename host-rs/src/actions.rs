//! Opening programs, files, folders and URLs, the way a double-click would.

pub fn open_target(target: &str) {
    let target = target.trim();
    if target.is_empty() {
        return;
    }
    log::info!("opening {target}");
    if let Err(e) = open(target) {
        log::warn!("could not open {target}: {e}");
    }
}

#[cfg(windows)]
fn open(target: &str) -> Result<(), String> {
    use windows::core::{w, PCWSTR};
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let wide: Vec<u16> = target.encode_utf16().chain(Some(0)).collect();
    let result = unsafe { ShellExecuteW(None, w!("open"), PCWSTR(wide.as_ptr()), None, None, SW_SHOWNORMAL) };
    // ShellExecute reports success with a value greater than 32.
    if result.0 as isize > 32 { Ok(()) } else { Err(format!("ShellExecute error {}", result.0 as isize)) }
}

#[cfg(not(windows))]
fn open(target: &str) -> Result<(), String> {
    let cmd = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
    std::process::Command::new(cmd).arg(target).spawn().map(|_| ()).map_err(|e| e.to_string())
}
