"""Key and media names the pad understands (they match adafruit_hid's Keycode and
ConsumerControlCode), grouped and labelled for the settings web app."""

MODIFIERS = [("CONTROL", "Ctrl"), ("SHIFT", "Shift"), ("ALT", "Alt"), ("GUI", "Win")]

KEY_GROUPS = [
    ("Letters", [(c, c) for c in "ABCDEFGHIJKLMNOPQRSTUVWXYZ"]),
    ("Numbers", [(n, str(i)) for i, n in enumerate(
        ["ZERO", "ONE", "TWO", "THREE", "FOUR", "FIVE", "SIX", "SEVEN", "EIGHT", "NINE"])]),
    ("Function keys", [("F%d" % i, "F%d" % i) for i in range(1, 25)]),
    ("Editing", [("ENTER", "Enter"), ("ESCAPE", "Esc"), ("BACKSPACE", "Backspace"), ("TAB", "Tab"),
                 ("SPACEBAR", "Space"), ("DELETE", "Delete"), ("INSERT", "Insert")]),
    ("Navigation", [("UP_ARROW", "Up"), ("DOWN_ARROW", "Down"), ("LEFT_ARROW", "Left"),
                    ("RIGHT_ARROW", "Right"), ("HOME", "Home"), ("END", "End"),
                    ("PAGE_UP", "Page Up"), ("PAGE_DOWN", "Page Down")]),
    ("Symbols", [("MINUS", "-"), ("EQUALS", "="), ("LEFT_BRACKET", "["), ("RIGHT_BRACKET", "]"),
                 ("BACKSLASH", "\\"), ("SEMICOLON", ";"), ("QUOTE", "'"), ("GRAVE_ACCENT", "`"),
                 ("COMMA", ","), ("PERIOD", "."), ("FORWARD_SLASH", "/")]),
    ("System", [("PRINT_SCREEN", "Print Screen"), ("SCROLL_LOCK", "Scroll Lock"), ("PAUSE", "Pause"),
                ("CAPS_LOCK", "Caps Lock"), ("APPLICATION", "Menu")]),
    ("Numpad", [("KEYPAD_%s" % n, "Num %s" % l) for n, l in [
        ("ZERO", "0"), ("ONE", "1"), ("TWO", "2"), ("THREE", "3"), ("FOUR", "4"), ("FIVE", "5"),
        ("SIX", "6"), ("SEVEN", "7"), ("EIGHT", "8"), ("NINE", "9"), ("PLUS", "+"), ("MINUS", "-"),
        ("ASTERISK", "*"), ("FORWARD_SLASH", "/"), ("ENTER", "Enter"), ("PERIOD", "."),
        ("NUMLOCK", "Lock")]]),
]

MEDIA = [
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
]

KEY_NAMES = {name for name, _ in MODIFIERS} | {name for _, keys in KEY_GROUPS for name, _ in keys}
MEDIA_NAMES = {name for name, _ in MEDIA}


class _Names:
    """Stands in for adafruit_hid's classes so mixpad_settings can check names on the PC."""

    def __init__(self, names):
        for name in names:
            setattr(self, name, name)


KEYCODE = _Names(KEY_NAMES | {"LEFT_CONTROL", "LEFT_SHIFT", "LEFT_ALT", "LEFT_GUI", "RIGHT_CONTROL",
                              "RIGHT_SHIFT", "RIGHT_ALT", "RIGHT_GUI", "WINDOWS", "COMMAND", "OPTION",
                              "RETURN", "SPACE", "POUND", "KEYPAD_EQUALS", "KEYPAD_BACKSLASH", "POWER"})
MEDIA_CODE = _Names(MEDIA_NAMES | {"RECORD", "EJECT"})


def for_ui():
    return {
        "modifiers": [{"name": n, "label": l} for n, l in MODIFIERS],
        "keyGroups": [{"group": g, "keys": [{"name": n, "label": l} for n, l in keys]}
                      for g, keys in KEY_GROUPS],
        "media": [{"name": n, "label": l} for n, l in MEDIA],
    }
