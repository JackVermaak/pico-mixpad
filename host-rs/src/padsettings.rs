//! The pad's settings.json: defaults and validation (same rules as firmware/mixpad_settings.py),
//! plus the key and media names the settings page offers (host/hid_names.py).

use serde_json::{json, Map, Value};

pub const TEXT_MAX: usize = 64;

pub fn defaults() -> Value {
    json!({
        "keys": [
            {"type": "mic"},
            {"type": "media", "code": "PLAY_PAUSE"},
            {"type": "media", "code": "SCAN_NEXT_TRACK"},
            {"type": "keys", "keys": ["F16"]},
            {"type": "keys", "keys": ["GUI", "SHIFT", "S"]},
            {"type": "keys", "keys": ["F18"]},
            {"type": "keys", "keys": ["F19"]},
            {"type": "keys", "keys": ["F20"]}
        ],
        "master_push": {"type": "media", "code": "MUTE"},
        "encoder_divisor": 4,
        "master_reversed": false,
        "app_reversed": false,
        "master_max_steps": 10,
        "screen": {
            "rotation": 180,
            "mic_row": "auto",
            "dim_after_s": 60,
            "off_after_s": 600,
            "dim_level": 0.05,
            "address": 60
        },
        "mic_led_offline": "off",
        "layout": {"keys": [0, 1, 2, 3, 4, 5, 6, 7], "knobs": [0, 1, 2, 3]}
    })
}

// ---- names ---------------------------------------------------------------------------------

pub const MODIFIERS: [(&str, &str); 4] = [("CONTROL", "Ctrl"), ("SHIFT", "Shift"), ("ALT", "Alt"), ("GUI", "Win")];

const NUMBER_NAMES: [&str; 10] = ["ZERO", "ONE", "TWO", "THREE", "FOUR", "FIVE", "SIX", "SEVEN", "EIGHT", "NINE"];

pub const MEDIA: [(&str, &str); 11] = [
    ("PLAY_PAUSE", "Play / pause"),
    ("SCAN_NEXT_TRACK", "Next track"),
    ("SCAN_PREVIOUS_TRACK", "Previous track"),
    ("STOP", "Stop"),
    ("MUTE", "Mute"),
    ("VOLUME_INCREMENT", "Volume up"),
    ("VOLUME_DECREMENT", "Volume down"),
    ("FAST_FORWARD", "Fast forward"),
    ("REWIND", "Rewind"),
    ("BRIGHTNESS_INCREMENT", "Brightness up"),
    ("BRIGHTNESS_DECREMENT", "Brightness down"),
];

/// Extra names adafruit_hid also accepts (settings files may contain them).
const KEY_ALIASES: [&str; 17] = [
    "LEFT_CONTROL", "LEFT_SHIFT", "LEFT_ALT", "LEFT_GUI", "RIGHT_CONTROL", "RIGHT_SHIFT", "RIGHT_ALT",
    "RIGHT_GUI", "WINDOWS", "COMMAND", "OPTION", "RETURN", "SPACE", "POUND", "KEYPAD_EQUALS",
    "KEYPAD_BACKSLASH", "POWER",
];
const MEDIA_ALIASES: [&str; 2] = ["RECORD", "EJECT"];

pub fn key_groups() -> Vec<(&'static str, Vec<(String, String)>)> {
    let pairs = |v: &[(&str, &str)]| v.iter().map(|(n, l)| (n.to_string(), l.to_string())).collect::<Vec<_>>();
    vec![
        ("Letters", ('A'..='Z').map(|c| (c.to_string(), c.to_string())).collect()),
        ("Numbers", NUMBER_NAMES.iter().enumerate().map(|(i, n)| (n.to_string(), i.to_string())).collect()),
        ("Function keys", (1..=24).map(|i| (format!("F{i}"), format!("F{i}"))).collect()),
        ("Editing", pairs(&[("ENTER", "Enter"), ("ESCAPE", "Esc"), ("BACKSPACE", "Backspace"), ("TAB", "Tab"),
            ("SPACEBAR", "Space"), ("DELETE", "Delete"), ("INSERT", "Insert")])),
        ("Navigation", pairs(&[("UP_ARROW", "Up"), ("DOWN_ARROW", "Down"), ("LEFT_ARROW", "Left"),
            ("RIGHT_ARROW", "Right"), ("HOME", "Home"), ("END", "End"), ("PAGE_UP", "Page Up"),
            ("PAGE_DOWN", "Page Down")])),
        ("Symbols", pairs(&[("MINUS", "-"), ("EQUALS", "="), ("LEFT_BRACKET", "["), ("RIGHT_BRACKET", "]"),
            ("BACKSLASH", "\\"), ("SEMICOLON", ";"), ("QUOTE", "'"), ("GRAVE_ACCENT", "`"), ("COMMA", ","),
            ("PERIOD", "."), ("FORWARD_SLASH", "/")])),
        ("System", pairs(&[("PRINT_SCREEN", "Print Screen"), ("SCROLL_LOCK", "Scroll Lock"), ("PAUSE", "Pause"),
            ("CAPS_LOCK", "Caps Lock"), ("APPLICATION", "Menu")])),
        ("Numpad", {
            let mut v: Vec<(String, String)> = NUMBER_NAMES
                .iter()
                .enumerate()
                .map(|(i, n)| (format!("KEYPAD_{n}"), format!("Num {i}")))
                .collect();
            for (n, l) in [("PLUS", "+"), ("MINUS", "-"), ("ASTERISK", "*"), ("FORWARD_SLASH", "/"),
                ("ENTER", "Enter"), ("PERIOD", "."), ("NUMLOCK", "Lock")] {
                v.push((format!("KEYPAD_{n}"), format!("Num {l}")));
            }
            v
        }),
    ]
}

pub fn is_key_name(name: &str) -> bool {
    MODIFIERS.iter().any(|(n, _)| *n == name)
        || KEY_ALIASES.contains(&name)
        || key_groups().iter().any(|(_, keys)| keys.iter().any(|(n, _)| n == name))
}

pub fn is_media_name(name: &str) -> bool {
    MEDIA.iter().any(|(n, _)| *n == name) || MEDIA_ALIASES.contains(&name)
}

/// The options block the settings page expects (`options` in /api/state).
pub fn options_for_ui() -> Value {
    json!({
        "modifiers": MODIFIERS.iter().map(|(n, l)| json!({"name": n, "label": l})).collect::<Vec<_>>(),
        "keyGroups": key_groups().into_iter().map(|(g, keys)| json!({
            "group": g,
            "keys": keys.into_iter().map(|(n, l)| json!({"name": n, "label": l})).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "media": MEDIA.iter().map(|(n, l)| json!({"name": n, "label": l})).collect::<Vec<_>>(),
    })
}

// ---- validation ----------------------------------------------------------------------------

/// A clean copy of one key action, or None if it is unusable.
pub fn check_action(action: &Value) -> Option<Value> {
    let obj = action.as_object()?;
    match obj.get("type")?.as_str()? {
        "media" => {
            let code = obj.get("code")?.as_str()?;
            is_media_name(code).then(|| json!({"type": "media", "code": code}))
        }
        "keys" => {
            let keys = obj.get("keys")?.as_array()?;
            if keys.is_empty() || keys.len() > 6 {
                return None;
            }
            let names: Option<Vec<&str>> = keys.iter().map(|k| k.as_str().filter(|n| is_key_name(n))).collect();
            Some(json!({"type": "keys", "keys": names?}))
        }
        "text" => {
            let text = obj.get("text")?.as_str()?;
            let ok = !text.is_empty()
                && text.chars().count() <= TEXT_MAX
                && text.chars().all(|c| (' '..='~').contains(&c));
            ok.then(|| json!({"type": "text", "text": text}))
        }
        kind @ ("mic" | "pc" | "none") => Some(json!({"type": kind})),
        _ => None,
    }
}

fn int_in(v: Option<&Value>, lo: i64, hi: i64) -> Option<i64> {
    v.and_then(Value::as_i64).filter(|n| (lo..=hi).contains(n))
}

fn num_in(v: Option<&Value>, lo: f64, hi: f64) -> Option<f64> {
    v.filter(|v| v.is_number()).and_then(Value::as_f64).filter(|n| (lo..=hi).contains(n))
}

fn is_permutation(v: &Value, size: usize) -> Option<Vec<i64>> {
    let arr = v.as_array()?;
    let nums: Option<Vec<i64>> = arr.iter().map(|n| if n.is_i64() || n.is_u64() { n.as_i64() } else { None }).collect();
    let nums = nums?;
    let mut sorted = nums.clone();
    sorted.sort_unstable();
    (sorted == (0..size as i64).collect::<Vec<_>>()).then_some(nums)
}

/// Merge raw settings over the defaults, dropping anything invalid.
pub fn normalise(raw: &Value) -> Value {
    let mut out = defaults();
    let Some(raw) = raw.as_object() else { return out };

    if let Some(keys) = raw.get("keys").and_then(Value::as_array) {
        for (i, a) in keys.iter().take(8).enumerate() {
            if let Some(clean) = check_action(a) {
                out["keys"][i] = clean;
            }
        }
    }
    if let Some(clean) = raw.get("master_push").and_then(check_action) {
        out["master_push"] = clean;
    }
    if let Some(d) = raw.get("encoder_divisor").and_then(Value::as_i64).filter(|d| [1, 2, 4].contains(d)) {
        out["encoder_divisor"] = json!(d);
    }
    for name in ["master_reversed", "app_reversed"] {
        if let Some(b) = raw.get(name).and_then(Value::as_bool) {
            out[name] = json!(b);
        }
    }
    if let Some(n) = int_in(raw.get("master_max_steps"), 1, 50) {
        out["master_max_steps"] = json!(n);
    }
    if let Some(m) = raw.get("mic_led_offline").and_then(Value::as_str).filter(|m| ["off", "blink"].contains(m)) {
        out["mic_led_offline"] = json!(m);
    }
    if let Some(layout) = raw.get("layout").and_then(Value::as_object) {
        for (name, size) in [("keys", 8), ("knobs", 4)] {
            if let Some(order) = layout.get(name).and_then(|v| is_permutation(v, size)) {
                out["layout"][name] = json!(order);
            }
        }
    }
    if let Some(s) = raw.get("screen").and_then(Value::as_object) {
        let screen: &mut Map<String, Value> = out["screen"].as_object_mut().expect("defaults have a screen");
        if let Some(r) = s.get("rotation").and_then(Value::as_i64).filter(|r| [0, 180].contains(r)) {
            screen.insert("rotation".into(), json!(r));
        }
        if let Some(m) = s.get("mic_row").and_then(Value::as_str).filter(|m| ["auto", "top", "bottom"].contains(m)) {
            screen.insert("mic_row".into(), json!(m));
        }
        for name in ["dim_after_s", "off_after_s"] {
            if let Some(n) = int_in(s.get(name), 0, 86400) {
                screen.insert(name.into(), json!(n));
            }
        }
        if let Some(n) = num_in(s.get("dim_level"), 0.0, 1.0) {
            screen.insert("dim_level".into(), json!(n));
        }
        if let Some(a) = s.get("address").and_then(Value::as_i64).filter(|a| [0x3C, 0x3D].contains(a)) {
            screen.insert("address".into(), json!(a));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_when_missing_or_garbage() {
        assert_eq!(normalise(&Value::Null), defaults());
        assert_eq!(normalise(&json!("nope")), defaults());
        assert_eq!(normalise(&json!({"keys": "x", "screen": 5, "encoder_divisor": 3})), defaults());
    }

    #[test]
    fn keeps_valid_values_and_drops_bad_ones() {
        let raw = json!({
            "keys": [{"type": "text", "text": "gg wp"}, {"type": "pc"}, {"type": "keys", "keys": ["CONTROL", "M"]},
                     {"type": "keys", "keys": ["NOT_A_KEY"]}, {"type": "media", "code": "NOPE"},
                     {"type": "text", "text": "é"}, {"type": "bogus"}, {"type": "none"}],
            "master_push": {"type": "pc"},
            "encoder_divisor": 2, "master_reversed": true, "app_reversed": "yes",
            "screen": {"rotation": 90, "mic_row": "top", "dim_after_s": true, "off_after_s": 30,
                       "dim_level": 2, "address": 61},
            "mic_led_offline": "blink",
        });
        let out = normalise(&raw);
        let d = defaults();
        assert_eq!(out["keys"].as_array().unwrap()[0..3], raw["keys"].as_array().unwrap()[0..3]);
        for i in 3..7 {
            assert_eq!(out["keys"][i], d["keys"][i], "invalid key {i} falls back to the default");
        }
        assert_eq!(out["keys"][7], json!({"type": "none"}));
        assert_eq!(out["master_push"], json!({"type": "pc"}));
        assert_eq!(out["encoder_divisor"], 2);
        assert_eq!(out["master_reversed"], true);
        assert_eq!(out["app_reversed"], false);
        assert_eq!(out["screen"], json!({"rotation": 180, "mic_row": "top", "dim_after_s": 60,
            "off_after_s": 30, "dim_level": 0.05, "address": 61}));
        assert_eq!(out["mic_led_offline"], "blink");
    }

    #[test]
    fn layout_must_be_a_full_reordering() {
        let good = json!({"layout": {"keys": [4, 5, 6, 7, 0, 1, 2, 3], "knobs": [3, 2, 1, 0]}});
        assert_eq!(normalise(&good)["layout"], good["layout"]);
        for bad in [json!([0, 0, 1, 2, 3, 4, 5, 6]), json!([0, 1, 2]), json!("x"), json!([true, 1, 2, 3, 4, 5, 6, 7])] {
            assert_eq!(normalise(&json!({"layout": {"keys": bad}}))["layout"]["keys"], json!([0, 1, 2, 3, 4, 5, 6, 7]));
        }
    }

    #[test]
    fn matches_the_python_defaults() {
        // firmware/mixpad_settings.py is the reference; check the values that are easy to drift.
        let py = include_str!("../../firmware/mixpad_settings.py");
        for needle in ["\"rotation\": 180", "\"dim_after_s\": 60", "\"off_after_s\": 600", "\"encoder_divisor\": 4",
                       "\"master_max_steps\": 10", "\"dim_level\": 0.05", "{\"type\": \"keys\", \"keys\": [\"GUI\", \"SHIFT\", \"S\"]}"] {
            assert!(py.contains(needle), "firmware default changed: {needle}");
        }
    }
}
