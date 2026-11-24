//! Audio control. `Audio` is the interface the connection logic uses; `WindowsAudio` implements it
//! with the Windows Core Audio API (per-app ISimpleAudioVolume, microphone IAudioEndpointVolume).
//!
//! Targets in a knob's app list:
//!   "discord.exe"  a process name (case-insensitive)
//!   "@focused"     whatever app is in the foreground window
//!   "@system"      Windows system sounds

pub trait Audio {
    /// Change the volume of every matching app by `percent` (positive = louder).
    fn nudge(&mut self, targets: &[String], percent: f64);
    /// Mute or unmute every matching app (following the first one's current state).
    fn toggle_mute(&mut self, targets: &[String]);
    /// (volume 0-100, muted) of the first matching app, or (None, false) if none is playing.
    fn level(&mut self, targets: &[String]) -> (Option<f64>, bool);
    fn mic_muted(&mut self) -> Option<bool>;
    /// Returns the new state: Some(true) = muted, None = no microphone.
    fn toggle_mic(&mut self) -> Option<bool>;
    /// Process names of apps with an audio session, for the settings page suggestions.
    fn session_names(&mut self) -> Vec<String>;
}

/// Stand-in used where Windows audio isn't available (tests, non-Windows builds).
pub struct NoAudio;

impl Audio for NoAudio {
    fn nudge(&mut self, _: &[String], _: f64) {}
    fn toggle_mute(&mut self, _: &[String]) {}
    fn level(&mut self, _: &[String]) -> (Option<f64>, bool) {
        (None, false)
    }
    fn mic_muted(&mut self) -> Option<bool> {
        None
    }
    fn toggle_mic(&mut self) -> Option<bool> {
        None
    }
    fn session_names(&mut self) -> Vec<String> {
        Vec::new()
    }
}

#[cfg(windows)]
pub use self::win::WindowsAudio;

#[cfg(windows)]
mod win {
    use std::time::{Duration, Instant};

    use windows::core::{Interface, PWSTR};
    use windows::Win32::Foundation::{CloseHandle, BOOL, HWND, S_OK};
    use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
    use windows::Win32::Media::Audio::{
        eCapture, eConsole, eRender, IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator,
        ISimpleAudioVolume, MMDeviceEnumerator,
    };
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED};
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

    use super::Audio;

    const SESSION_CACHE: Duration = Duration::from_secs(1);

    struct Session {
        /// Lower-case process name; None for the system sounds session.
        name: Option<String>,
        volume: ISimpleAudioVolume,
    }

    pub struct WindowsAudio {
        enumerator: IMMDeviceEnumerator,
        sessions: Vec<Session>,
        listed: Option<Instant>,
    }

    pub fn process_name(pid: u32) -> Option<String> {
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let mut buf = [0u16; 1024];
            let mut len = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
            let _ = CloseHandle(handle);
            ok.ok()?;
            let path = String::from_utf16_lossy(&buf[..len as usize]);
            path.rsplit(['\\', '/']).next().map(|n| n.to_lowercase())
        }
    }

    pub fn foreground_exe() -> Option<String> {
        unsafe {
            let hwnd: HWND = GetForegroundWindow();
            if hwnd.0.is_null() {
                return None;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            (pid != 0).then(|| process_name(pid)).flatten()
        }
    }

    impl WindowsAudio {
        /// Must be created on the thread that will use it (COM is initialised for that thread).
        pub fn new() -> windows::core::Result<Self> {
            unsafe {
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
                let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
                Ok(WindowsAudio { enumerator, sessions: Vec::new(), listed: None })
            }
        }

        fn list_sessions(&self) -> windows::core::Result<Vec<Session>> {
            let mut out = Vec::new();
            unsafe {
                let device = self.enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;
                let manager: IAudioSessionManager2 = device.Activate(CLSCTX_ALL, None)?;
                let list = manager.GetSessionEnumerator()?;
                for i in 0..list.GetCount()? {
                    let Ok(control) = list.GetSession(i) else { continue };
                    let Ok(control2) = control.cast::<IAudioSessionControl2>() else { continue };
                    let Ok(volume) = control.cast::<ISimpleAudioVolume>() else { continue };
                    let name = if control2.IsSystemSoundsSession() == S_OK {
                        None
                    } else {
                        match control2.GetProcessId().ok().and_then(process_name) {
                            Some(n) => Some(n),
                            None => continue, // process exited, or we may not read it
                        }
                    };
                    out.push(Session { name, volume });
                }
            }
            Ok(out)
        }

        fn sessions(&mut self) -> &[Session] {
            if self.listed.map_or(true, |t| t.elapsed() > SESSION_CACHE) {
                match self.list_sessions() {
                    Ok(s) => self.sessions = s,
                    Err(e) => {
                        log::warn!("listing audio sessions failed: {e}");
                        self.sessions.clear();
                    }
                }
                self.listed = Some(Instant::now());
            }
            &self.sessions
        }

        fn matching(&mut self, targets: &[String]) -> Vec<ISimpleAudioVolume> {
            let targets: Vec<String> = targets.iter().map(|t| t.to_lowercase()).collect();
            let mut names: Vec<String> = targets.iter().filter(|t| !t.starts_with('@')).cloned().collect();
            if targets.iter().any(|t| t == "@focused") {
                if let Some(fg) = foreground_exe() {
                    names.push(fg);
                }
            }
            let want_system = targets.iter().any(|t| t == "@system");
            self.sessions()
                .iter()
                .filter(|s| match &s.name {
                    None => want_system,
                    Some(n) => names.contains(n),
                })
                .map(|s| s.volume.clone())
                .collect()
        }

        fn mic(&self) -> Option<IAudioEndpointVolume> {
            unsafe {
                let device = self.enumerator.GetDefaultAudioEndpoint(eCapture, eConsole).ok()?;
                device.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None).ok()
            }
        }
    }

    impl Audio for WindowsAudio {
        fn nudge(&mut self, targets: &[String], percent: f64) {
            for v in self.matching(targets) {
                unsafe {
                    if let Ok(level) = v.GetMasterVolume() {
                        let new = (level as f64 + percent / 100.0).clamp(0.0, 1.0) as f32;
                        let _ = v.SetMasterVolume(new, std::ptr::null());
                    }
                }
            }
        }

        fn toggle_mute(&mut self, targets: &[String]) {
            let found = self.matching(targets);
            let Some(first) = found.first() else { return };
            unsafe {
                let muted = first.GetMute().map(|b| b.as_bool()).unwrap_or(false);
                for v in &found {
                    let _ = v.SetMute(BOOL::from(!muted), std::ptr::null());
                }
            }
        }

        fn level(&mut self, targets: &[String]) -> (Option<f64>, bool) {
            match self.matching(targets).first() {
                None => (None, false),
                Some(v) => unsafe {
                    let vol = v.GetMasterVolume().map(|l| l as f64 * 100.0).ok();
                    let muted = v.GetMute().map(|b| b.as_bool()).unwrap_or(false);
                    (vol, muted)
                },
            }
        }

        fn mic_muted(&mut self) -> Option<bool> {
            let mic = self.mic()?;
            unsafe { mic.GetMute().ok().map(|b| b.as_bool()) }
        }

        fn toggle_mic(&mut self) -> Option<bool> {
            let mic = self.mic()?;
            unsafe {
                let muted = !mic.GetMute().ok()?.as_bool();
                mic.SetMute(BOOL::from(muted), std::ptr::null()).ok()?;
                Some(muted)
            }
        }

        fn session_names(&mut self) -> Vec<String> {
            let mut names: Vec<String> = self.sessions().iter().filter_map(|s| s.name.clone()).collect();
            names.sort();
            names.dedup();
            names
        }
    }
}
