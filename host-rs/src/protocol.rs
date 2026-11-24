//! Line protocol between the Pico Mixpad and the PC (same as host/protocol.py).
//!
//! ASCII text, one message per line, terminated by `\n`.
//!
//! Device -> PC
//!   `HELLO MACROPAD <version>`  reply to `?`
//!   `E <ch> <delta>`            app knob 0-3 turned; positive = clockwise
//!   `P <ch>`                    app knob pushed
//!   `M`                         mic-mute key pressed
//!   `K <id>`                    key with a PC action pressed: 1-8, or `master`
//!   `B <n>`                     any button pressed (0-7 keys, 8-11 knob pushes, 12 master)
//!
//! PC -> device
//!   `?`                                   handshake
//!   `V <ch> <volume|-> <muted> <label>`   one app channel; `-` = app not running
//!   `MIC <1|0|->`                         mic muted / live / no microphone
//!   `QUIET <1|0>`                         key finder: buttons only report (max 60 s)

pub const HANDSHAKE: &str = "?\n";
pub const LABEL_MAX: usize = 10;
pub const KEY_IDS: [&str; 9] = ["1", "2", "3", "4", "5", "6", "7", "8", "master"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceMessage {
    Hello(String),
    Turn { channel: i32, delta: i32 },
    Press(i32),
    MicToggle,
    KeyAction(String),
    Button(u8),
}

/// Parse one line from the device. `None` for anything unrecognised.
pub fn parse_device_line(line: &str) -> Option<DeviceMessage> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    match parts.as_slice() {
        ["HELLO", "MACROPAD", version, ..] => Some(DeviceMessage::Hello(version.to_string())),
        ["E", ch, delta] => Some(DeviceMessage::Turn { channel: ch.parse().ok()?, delta: delta.parse().ok()? }),
        ["P", ch] => Some(DeviceMessage::Press(ch.parse().ok()?)),
        ["M"] => Some(DeviceMessage::MicToggle),
        ["K", id] if KEY_IDS.contains(id) => Some(DeviceMessage::KeyAction(id.to_string())),
        ["B", n] => {
            let n: u8 = n.parse().ok()?;
            (n <= 12).then_some(DeviceMessage::Button(n))
        }
        _ => None,
    }
}

/// The OLED font is ASCII only: keep printable ASCII, trim, at most LABEL_MAX characters.
pub fn clean_label(label: &str) -> String {
    let cleaned: String = label.chars().filter(|c| c.is_ascii() && !c.is_ascii_control()).collect();
    let cut: String = cleaned.trim().chars().take(LABEL_MAX).collect();
    if cut.is_empty() { "?".to_string() } else { cut }
}

pub fn channel_line(channel: usize, label: &str, volume: Option<f64>, muted: bool) -> String {
    let vol = match volume {
        None => "-".to_string(),
        Some(v) => (v.round().clamp(0.0, 100.0) as i64).to_string(),
    };
    format!("V {} {} {} {}\n", channel, vol, muted as u8, clean_label(label))
}

pub fn mic_line(muted: Option<bool>) -> String {
    match muted {
        None => "MIC -\n".to_string(),
        Some(m) => format!("MIC {}\n", m as u8),
    }
}

pub fn quiet_line(on: bool) -> String {
    if on { "QUIET 1\n".to_string() } else { "QUIET 0\n".to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_device_message() {
        use DeviceMessage::*;
        assert_eq!(parse_device_line("HELLO MACROPAD 1.3\n"), Some(Hello("1.3".into())));
        assert_eq!(parse_device_line("E 2 -3"), Some(Turn { channel: 2, delta: -3 }));
        assert_eq!(parse_device_line("P 1"), Some(Press(1)));
        assert_eq!(parse_device_line("M"), Some(MicToggle));
        assert_eq!(parse_device_line("K master"), Some(KeyAction("master".into())));
        assert_eq!(parse_device_line("B 12"), Some(Button(12)));
    }

    #[test]
    fn rejects_garbage() {
        for line in ["", "E 1", "E x 2", "K 9", "B 13", "M 1", "HELLO WORLD 1", "B -1"] {
            assert_eq!(parse_device_line(line), None, "{line:?}");
        }
    }

    #[test]
    fn formats_state_lines() {
        assert_eq!(channel_line(0, "Game", Some(72.4), false), "V 0 72 0 Game\n");
        assert_eq!(channel_line(3, "Music", None, false), "V 3 - 0 Music\n");
        assert_eq!(channel_line(1, "x", Some(140.0), true), "V 1 100 1 x\n");
        assert_eq!(mic_line(Some(true)), "MIC 1\n");
        assert_eq!(mic_line(None), "MIC -\n");
        assert_eq!(clean_label("  Spötify\n  very long name "), "Sptify  ve");
        assert_eq!(clean_label("\u{1F3AE}"), "?");
    }
}
