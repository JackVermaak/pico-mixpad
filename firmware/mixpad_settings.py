# mixpad_settings.py - loads /settings.json for the Pico Mixpad firmware.
#
# settings.json is written by the settings web app (host/macropad_host.py --settings).
# Every value is optional: anything missing or invalid falls back to DEFAULTS, so a broken
# file can never stop the pad from working. Runs on CircuitPython and on desktop Python (tests).

import json

DEFAULTS = {
    "keys": [
        {"type": "mic"},
        {"type": "media", "code": "PLAY_PAUSE"},
        {"type": "media", "code": "SCAN_NEXT_TRACK"},
        {"type": "keys", "keys": ["F16"]},
        {"type": "keys", "keys": ["GUI", "SHIFT", "S"]},
        {"type": "keys", "keys": ["F18"]},
        {"type": "keys", "keys": ["F19"]},
        {"type": "keys", "keys": ["F20"]},
    ],
    "master_push": {"type": "media", "code": "MUTE"},
    "encoder_divisor": 4,
    "master_reversed": False,
    "app_reversed": False,
    "master_max_steps": 10,
    "screen": {
        "rotation": 180,        # the case mounts the OLED upside down; 0 if your text is upside down
        "mic_row": "auto",      # "auto" = in the yellow strip, "top" or "bottom"
        "dim_after_s": 60,
        "off_after_s": 600,
        "dim_level": 0.05,
        "address": 0x3C,
    },
    "mic_led_offline": "off",   # "off" or "blink"
    # Where each key and app knob sits on the pad: position -> wiring number (key 0 = GP0).
    # Positions run left to right, back key row first. Used by the settings page; the firmware
    # uses the knob order to list the apps on the screen in the same left-to-right order.
    "layout": {"keys": [0, 1, 2, 3, 4, 5, 6, 7], "knobs": [0, 1, 2, 3]},
}

ACTION_TYPES = ("mic", "media", "keys", "text", "pc", "none")
TEXT_MAX = 64


def _copy(value):
    return json.loads(json.dumps(value))


def _num(value, default, lo, hi, cast=int):
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        return default
    value = cast(value)
    if value < lo or value > hi:
        return default
    return value


def check_action(action, keycode=None, media=None):
    """A clean copy of one key action, or None if it is unusable.
    keycode / media: the adafruit_hid classes, used to check names (skipped when None)."""
    if not isinstance(action, dict):
        return None
    kind = action.get("type")
    if kind not in ACTION_TYPES:
        return None
    if kind == "media":
        code = action.get("code")
        if not isinstance(code, str) or (media is not None and not hasattr(media, code)):
            return None
        return {"type": "media", "code": code}
    if kind == "keys":
        names = action.get("keys")
        if not isinstance(names, list) or not 1 <= len(names) <= 6:
            return None
        for name in names:
            if not isinstance(name, str) or (keycode is not None and not hasattr(keycode, name)):
                return None
        return {"type": "keys", "keys": list(names)}
    if kind == "text":
        text = action.get("text")
        if not isinstance(text, str) or not text or len(text) > TEXT_MAX:
            return None
        if any(ord(c) < 32 or ord(c) > 126 for c in text):
            return None
        return {"type": "text", "text": text}
    return {"type": kind}


def normalise(raw, keycode=None, media=None):
    """Merge raw settings over DEFAULTS, dropping anything invalid."""
    out = _copy(DEFAULTS)
    if not isinstance(raw, dict):
        return out
    keys = raw.get("keys")
    if isinstance(keys, list):
        for i in range(min(len(keys), len(out["keys"]))):
            action = check_action(keys[i], keycode, media)
            if action is not None:
                out["keys"][i] = action
    action = check_action(raw.get("master_push"), keycode, media)
    if action is not None:
        out["master_push"] = action
    if raw.get("encoder_divisor") in (1, 2, 4):
        out["encoder_divisor"] = raw["encoder_divisor"]
    for name in ("master_reversed", "app_reversed"):
        if isinstance(raw.get(name), bool):
            out[name] = raw[name]
    out["master_max_steps"] = _num(raw.get("master_max_steps"), out["master_max_steps"], 1, 50)
    if raw.get("mic_led_offline") in ("off", "blink"):
        out["mic_led_offline"] = raw["mic_led_offline"]

    layout = raw.get("layout")
    if isinstance(layout, dict):
        for name, size in (("keys", 8), ("knobs", 4)):
            order = layout.get(name)
            if isinstance(order, list) and sorted(order) == list(range(size)) \
                    and all(isinstance(n, int) and not isinstance(n, bool) for n in order):
                out["layout"][name] = list(order)

    screen_raw = raw.get("screen")
    if isinstance(screen_raw, dict):
        s = out["screen"]
        if screen_raw.get("rotation") in (0, 180):
            s["rotation"] = screen_raw["rotation"]
        if screen_raw.get("mic_row") in ("auto", "top", "bottom"):
            s["mic_row"] = screen_raw["mic_row"]
        s["dim_after_s"] = _num(screen_raw.get("dim_after_s"), s["dim_after_s"], 0, 86400)
        s["off_after_s"] = _num(screen_raw.get("off_after_s"), s["off_after_s"], 0, 86400)
        s["dim_level"] = _num(screen_raw.get("dim_level"), s["dim_level"], 0.0, 1.0, float)
        if screen_raw.get("address") in (0x3C, 0x3D):
            s["address"] = screen_raw["address"]
    return out


def load(path="/settings.json", keycode=None, media=None):
    try:
        with open(path) as f:
            raw = json.load(f)
    except (OSError, ValueError) as err:
        if not isinstance(err, OSError):
            print("settings.json is not valid JSON, using defaults:", err)
        raw = None
    return normalise(raw, keycode, media)


def row_positions(screen):
    """Top edge of the 5 text rows (mic line first, then 4 apps), 12 px font.
    On yellow/blue screens the yellow strip is the panel's first 16 pixel rows: at the top
    when rotation is 0, at the bottom when it is 180."""
    mic_row = screen["mic_row"]
    if mic_row == "auto":
        mic_row = "top" if screen["rotation"] == 0 else "bottom"
    if mic_row == "top":
        return (2, 16, 28, 40, 52)
    return (51, 0, 12, 24, 36)
