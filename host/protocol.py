"""Line protocol between the Pico Mixpad and the PC.

ASCII text, one message per line, terminated by "\\n". Human-readable on purpose:
you can debug it by opening the data COM port in any serial terminal.

Device -> PC
    HELLO MACROPAD <version>   reply to "?"
    E <ch> <delta>             app knob <ch> (0-3) turned <delta> steps; positive = clockwise
    P <ch>                     app knob <ch> pushed
    M                          mic-mute key pressed
    K <id>                     key with a PC action pressed: 1-8, or "master" for the master knob push
    B <n>                      any button pressed (0-7 keys, 8-11 app knob pushes, 12 master push);
                               used by the settings page to find which key is where

PC -> device
    ?                                  handshake / "who are you"
    V <ch> <volume|-> <muted 0|1> <label>   state of one app channel; "-" = app not running
    MIC <1|0|->                        mic muted / live / no microphone
    QUIET <1|0>                        key finder on/off: buttons only report, without acting (max 60 s)
"""

from dataclasses import dataclass
from typing import Optional, Union

LABEL_MAX = 10


@dataclass(frozen=True)
class Hello:
    version: str


@dataclass(frozen=True)
class Turn:
    channel: int
    delta: int


@dataclass(frozen=True)
class Press:
    channel: int


@dataclass(frozen=True)
class MicToggle:
    pass


@dataclass(frozen=True)
class KeyAction:
    key: str


@dataclass(frozen=True)
class Button:
    number: int


KEY_IDS = ("1", "2", "3", "4", "5", "6", "7", "8", "master")

DeviceMessage = Union[Hello, Turn, Press, MicToggle, KeyAction, Button]


def parse_device_line(line: str) -> Optional[DeviceMessage]:
    """Parse one line from the device. Returns None for anything unrecognised."""
    parts = line.strip().split()
    if not parts:
        return None
    kind = parts[0]
    try:
        if kind == "HELLO" and len(parts) >= 3 and parts[1] == "MACROPAD":
            return Hello(parts[2])
        if kind == "E" and len(parts) == 3:
            return Turn(int(parts[1]), int(parts[2]))
        if kind == "P" and len(parts) == 2:
            return Press(int(parts[1]))
        if kind == "M" and len(parts) == 1:
            return MicToggle()
        if kind == "K" and len(parts) == 2 and parts[1] in KEY_IDS:
            return KeyAction(parts[1])
        if kind == "B" and len(parts) == 2 and 0 <= int(parts[1]) <= 12:
            return Button(int(parts[1]))
    except ValueError:
        return None
    return None


def clean_label(label: str) -> str:
    """The OLED font is ASCII only; keep printable ASCII, no newlines, max LABEL_MAX chars."""
    cleaned = "".join(c for c in label if c.isascii() and c.isprintable())
    return cleaned.strip()[:LABEL_MAX] or "?"


def channel_line(channel: int, label: str, volume: Optional[float], muted: bool) -> str:
    vol = "-" if volume is None else str(max(0, min(100, round(volume))))
    return f"V {channel} {vol} {int(bool(muted))} {clean_label(label)}\n"


def mic_line(muted: Optional[bool]) -> str:
    return "MIC -\n" if muted is None else f"MIC {int(bool(muted))}\n"


def quiet_line(on: bool) -> str:
    return "QUIET 1\n" if on else "QUIET 0\n"


HANDSHAKE = "?\n"
