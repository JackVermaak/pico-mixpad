//! Small Windows helpers.

use windows::core::w;
use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
use windows::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
use windows::Win32::System::Threading::CreateMutexW;

/// When started from a terminal, print the log there too. Returns true if a console was attached.
pub fn attach_console() -> bool {
    unsafe { AttachConsole(ATTACH_PARENT_PROCESS).is_ok() }
}

/// False if another copy is already running. The mutex lives until the process exits.
pub fn single_instance() -> bool {
    unsafe {
        match CreateMutexW(None, true, w!("Local\\PicoMixpadHost")) {
            Ok(handle) => {
                let first = GetLastError() != ERROR_ALREADY_EXISTS;
                let _ = handle; // never closed: the mutex lives as long as the process
                first
            }
            Err(_) => true, // can't tell: run anyway
        }
    }
}
