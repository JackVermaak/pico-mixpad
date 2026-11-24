//! State shared between the pad connection, the settings page and the tray icon.

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Level {
    pub volume: Option<f64>,
    pub muted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Input {
    pub kind: &'static str, // "button" or "knob"
    pub n: i64,
    pub seq: u64,
}

#[derive(Debug, Default)]
pub struct Status {
    pub connected: bool,
    pub port: Option<String>,
    pub firmware: Option<String>,
    pub levels: Vec<Level>,
    pub mic: Option<bool>,
    pub audio_apps: Vec<String>,
    pub input: Option<Input>,
    /// Key finder (settings page "rearrange"): inputs are reported, not acted on, until then.
    pub identify_until: Option<Instant>,
    /// Lines for the pad, queued by the settings page and sent by the connection loop.
    pub outbox: Vec<String>,
    /// Settings page running without the pad connection (never set by this app).
    pub standalone: bool,
}

impl Status {
    pub fn identifying(&self) -> bool {
        self.identify_until.is_some_and(|t| Instant::now() < t)
    }

    pub fn set_identify(&mut self, on: bool) {
        self.identify_until = on.then(|| Instant::now() + Duration::from_secs(60));
    }

    pub fn report(&mut self, kind: &'static str, n: i64) {
        let seq = self.input.as_ref().map_or(0, |i| i.seq) + 1;
        self.input = Some(Input { kind, n, seq });
    }

    /// The `status` object the settings page reads.
    pub fn to_json(&self) -> Value {
        json!({
            "connected": self.connected,
            "port": self.port,
            "firmware": self.firmware,
            "levels": self.levels.iter().map(|l| json!({
                "volume": l.volume.map(|v| v.round() as i64),
                "muted": l.muted,
            })).collect::<Vec<_>>(),
            "mic": self.mic,
            "audio_apps": self.audio_apps,
            "input": self.input.as_ref().map(|i| json!({"kind": i.kind, "n": i.n, "seq": i.seq})),
            "identifying": self.identifying(),
            "standalone": self.standalone,
        })
    }
}

#[derive(Clone, Default)]
pub struct Shared(Arc<Mutex<Status>>);

impl Shared {
    pub fn lock(&self) -> MutexGuard<'_, Status> {
        // A panic while holding the lock would poison it; the data is still usable.
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
}
