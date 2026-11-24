//! One connected session: turns pad messages into audio changes and sends the state back to the
//! OLED. Independent of the serial port and of Windows, so it is unit-tested with fakes.

use std::io;
use std::time::{Duration, Instant};

use crate::audio::Audio;
use crate::config::Config;
use crate::protocol::{self, DeviceMessage};
use crate::status::{Level, Shared};

pub const STATE_PUSH: Duration = Duration::from_secs(1); // full refresh, doubles as heartbeat
pub const MIN_PUSH_GAP: Duration = Duration::from_millis(50); // at most 20 refreshes/s after events

/// A line-based link to the pad (the serial port, or a fake in tests).
pub trait Link {
    /// Waits briefly for one line; Ok(None) if nothing arrived.
    fn read_line(&mut self) -> io::Result<Option<String>>;
    fn write(&mut self, data: &[u8]) -> io::Result<()>;
}

pub struct Bridge<'a, L: Link, A: Audio + ?Sized> {
    pub link: L,
    pub audio: &'a mut A,
    pub config: Config,
    pub status: Shared,
    pub opener: Box<dyn FnMut(&str) + 'a>,
    last_push: Option<Instant>,
    pub dirty: bool,
}

impl<'a, L: Link, A: Audio + ?Sized> Bridge<'a, L, A> {
    pub fn new(link: L, audio: &'a mut A, config: Config, status: Shared, opener: Box<dyn FnMut(&str) + 'a>) -> Self {
        Bridge { link, audio, config, status, opener, last_push: None, dirty: true }
    }

    /// Act on one message. Returns true if the OLED should be refreshed now.
    pub fn handle(&mut self, msg: DeviceMessage) -> bool {
        use DeviceMessage::*;
        let identifying = {
            let mut st = self.status.lock();
            match &msg {
                Button(n) => {
                    st.report("button", *n as i64);
                    return false;
                }
                Turn { channel, .. } => st.report("knob", *channel as i64),
                _ => {}
            }
            st.identifying()
        };
        if identifying && matches!(msg, Turn { .. } | Press(_) | MicToggle | KeyAction(_)) {
            return false;
        }
        let channels = &self.config.channels;
        match msg {
            Turn { channel, delta } if (0..channels.len() as i32).contains(&channel) => {
                let apps = channels[channel as usize].apps.clone();
                self.audio.nudge(&apps, delta as f64 * self.config.step_percent);
                true
            }
            Press(channel) if (0..channels.len() as i32).contains(&channel) => {
                let apps = channels[channel as usize].apps.clone();
                self.audio.toggle_mute(&apps);
                true
            }
            MicToggle => {
                let state = self.audio.toggle_mic();
                log::info!("mic {}", match state { Some(true) => "muted", Some(false) => "live", None => "not found" });
                true
            }
            KeyAction(key) => {
                match self.config.pc_action(&key).map(str::to_string) {
                    Some(target) => (self.opener)(&target),
                    None => log::info!("key {key} has no PC action set"),
                }
                false
            }
            _ => false,
        }
    }

    pub fn push_state(&mut self) -> io::Result<()> {
        let mut out = String::new();
        let mut levels = Vec::new();
        for (i, ch) in self.config.channels.iter().enumerate() {
            let (volume, muted) = self.audio.level(&ch.apps);
            levels.push(Level { volume, muted });
            out.push_str(&protocol::channel_line(i, &ch.label, volume, muted));
        }
        let mic = self.audio.mic_muted();
        out.push_str(&protocol::mic_line(mic));
        self.link.write(out.as_bytes())?;
        self.last_push = Some(Instant::now());
        let apps = self.audio.session_names();
        let mut st = self.status.lock();
        st.levels = levels;
        st.mic = mic;
        st.audio_apps = apps;
        Ok(())
    }

    /// Send queued lines, wait briefly for one pad line, act on it, refresh the OLED when needed.
    pub fn step(&mut self) -> io::Result<()> {
        let queued: Vec<String> = std::mem::take(&mut self.status.lock().outbox);
        for line in queued {
            self.link.write(line.as_bytes())?;
        }
        if let Some(line) = self.link.read_line()? {
            if let Some(msg) = protocol::parse_device_line(&line) {
                if self.handle(msg) {
                    self.dirty = true;
                }
            }
        }
        let since = self.last_push.map_or(Duration::MAX, |t| t.elapsed());
        if (self.dirty && since >= MIN_PUSH_GAP) || since >= STATE_PUSH {
            self.push_state()?;
            self.dirty = false;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Channel;
    use std::collections::VecDeque;

    struct FakeLink {
        lines: VecDeque<String>,
        written: String,
    }

    impl Link for FakeLink {
        fn read_line(&mut self) -> io::Result<Option<String>> {
            Ok(self.lines.pop_front())
        }
        fn write(&mut self, data: &[u8]) -> io::Result<()> {
            self.written.push_str(std::str::from_utf8(data).unwrap());
            Ok(())
        }
    }

    #[derive(Default)]
    struct FakeAudio {
        nudges: Vec<(Vec<String>, f64)>,
        mutes: Vec<Vec<String>>,
        mic: bool,
    }

    impl Audio for FakeAudio {
        fn nudge(&mut self, t: &[String], p: f64) {
            self.nudges.push((t.to_vec(), p));
        }
        fn toggle_mute(&mut self, t: &[String]) {
            self.mutes.push(t.to_vec());
        }
        fn level(&mut self, t: &[String]) -> (Option<f64>, bool) {
            if t.is_empty() { (None, false) } else { (Some(42.4), false) }
        }
        fn mic_muted(&mut self) -> Option<bool> {
            Some(self.mic)
        }
        fn toggle_mic(&mut self) -> Option<bool> {
            self.mic = !self.mic;
            Some(self.mic)
        }
        fn session_names(&mut self) -> Vec<String> {
            vec!["cs2.exe".into()]
        }
    }

    fn link(lines: &[&str]) -> FakeLink {
        FakeLink { lines: lines.iter().map(|s| s.to_string()).collect(), written: String::new() }
    }

    fn config() -> Config {
        let mut cfg = Config::default();
        cfg.channels = vec![
            Channel { label: "Game".into(), apps: vec!["cs2.exe".into()] },
            Channel { label: "B".into(), apps: vec![] },
            Channel { label: "C".into(), apps: vec![] },
            Channel { label: "D".into(), apps: vec![] },
        ];
        cfg.step_percent = 3.0;
        cfg.pc_actions = vec![("3".into(), "notepad.exe".into())];
        cfg
    }

    #[test]
    fn knobs_mic_and_state() {
        let mut audio = FakeAudio::default();
        let status = Shared::default();
        let mut b = Bridge::new(link(&["E 0 2", "P 0", "M", "E 7 1"]), &mut audio, config(), status.clone(), Box::new(|_| {}));
        for _ in 0..4 {
            b.step().unwrap();
            std::thread::sleep(MIN_PUSH_GAP + Duration::from_millis(5)); // refreshes are rate-limited
        }
        assert!(b.link.written.starts_with("V 0 42 0 Game\nV 1 - 0 B\n"));
        assert!(b.link.written.contains("MIC 1\n"));
        drop(b);
        assert_eq!(audio.nudges, vec![(vec!["cs2.exe".to_string()], 6.0)]);
        assert_eq!(audio.mutes, vec![vec!["cs2.exe".to_string()]]);
        let st = status.lock();
        assert_eq!(st.levels[0].volume, Some(42.4));
        assert_eq!(st.audio_apps, vec!["cs2.exe".to_string()]);
    }

    #[test]
    fn pc_actions_open_their_target() {
        let mut audio = FakeAudio::default();
        let mut opened = Vec::new();
        {
            let mut b = Bridge::new(link(&["K 3", "K 4"]), &mut audio, config(), Shared::default(),
                Box::new(|t: &str| opened.push(t.to_string())));
            b.step().unwrap();
            b.step().unwrap();
        }
        assert_eq!(opened, vec!["notepad.exe".to_string()]);
    }

    #[test]
    fn key_finder_reports_without_acting() {
        let mut audio = FakeAudio::default();
        let status = Shared::default();
        {
            let mut st = status.lock();
            st.outbox.push(protocol::quiet_line(true));
            st.set_identify(true);
        }
        {
            let mut b = Bridge::new(link(&["B 5", "E 2 1", "M"]), &mut audio, config(), status.clone(), Box::new(|_| {}));
            b.step().unwrap();
            assert!(b.link.written.starts_with("QUIET 1\n"));
            assert_eq!(status.lock().input.as_ref().map(|i| (i.kind, i.n, i.seq)), Some(("button", 5, 1)));
            b.step().unwrap();
            assert_eq!(status.lock().input.as_ref().map(|i| (i.kind, i.n, i.seq)), Some(("knob", 2, 2)));
            b.step().unwrap();
        }
        assert!(audio.nudges.is_empty() && !audio.mic, "inputs are only reported while finding keys");
        assert!(status.lock().outbox.is_empty());
    }
}
