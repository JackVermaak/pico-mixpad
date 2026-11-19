"""Pico Mixpad - Windows host app.

Connects to the macropad's data serial port, turns knob steps into per-app
volume changes, toggles the microphone mute, runs PC actions for keys,
and sends the current state back to the macropad's OLED once a second.
It also serves the settings web app on http://localhost:8765.

Run:        python macropad_host.py              (console, shows log)
            python macropad_host.py --settings   (also opens the settings page)
Background: pythonw macropad_host.py             (no window; log in macropad_host.log)
"""

import argparse
import logging
import os
import subprocess
import sys
import threading
import time
import tomllib
import webbrowser
from dataclasses import dataclass, field
from logging.handlers import RotatingFileHandler
from pathlib import Path
from typing import Dict, List, Optional

import serial
from serial.tools import list_ports

import protocol

HERE = Path(__file__).resolve().parent
CONFIG_PATH = HERE / "config.toml"
log = logging.getLogger("mixpad")

KNOWN_VIDS = {0x239A, 0x2E8A}   # Adafruit (CircuitPython) and Raspberry Pi USB vendor IDs
BAUD = 115200                   # ignored by USB CDC, required by pyserial
READ_TIMEOUT_S = 0.2            # how long readline() waits; sets the idle wake-up rate
STATE_PUSH_S = 1.0              # full state refresh to the OLED (also acts as heartbeat)
MIN_PUSH_GAP_S = 0.05           # fastest refresh after a knob/key event
CONFIG_CHECK_S = 1.0            # how often config.toml is checked for changes


@dataclass
class Channel:
    label: str
    apps: List[str]


@dataclass
class Config:
    port: str = "auto"
    step_percent: float = 2.0
    channels: List[Channel] = field(default_factory=list)
    pc_actions: Dict[str, str] = field(default_factory=dict)   # key id ("1"-"8", "master") -> target
    web_port: int = 8765
    pad_drive: str = "auto"


def load_config(path: Path) -> Config:
    with open(path, "rb") as f:
        raw = tomllib.load(f)
    channels = [Channel(c.get("label", f"App {i + 1}"), list(c.get("apps", [])))
                for i, c in enumerate(raw.get("channel", []))]
    if len(channels) > 4:
        raise ValueError("config.toml defines more than 4 [[channel]] blocks")
    while len(channels) < 4:
        channels.append(Channel(f"App {len(channels) + 1}", []))
    actions = {str(k): str(v) for k, v in raw.get("pc_actions", {}).items()
               if str(k) in protocol.KEY_IDS and str(v).strip()}
    return Config(port=str(raw.get("port", "auto")),
                  step_percent=float(raw.get("step_percent", 2)),
                  channels=channels,
                  pc_actions=actions,
                  web_port=int(raw.get("web_port", 8765)),
                  pad_drive=str(raw.get("pad_drive", "auto")))


def _toml_str(value: str) -> str:
    out = value.replace("\\", "\\\\").replace('"', '\\"')
    return '"' + "".join(c if c.isprintable() else "" for c in out) + '"'


def dump_config(cfg: Config) -> str:
    """config.toml text for a Config (used when the settings page saves)."""
    lines = [
        "# Pico Mixpad host app configuration.",
        "# Edit here or in the settings page (python macropad_host.py --settings).",
        "",
        '# COM port of the macropad\'s DATA port, e.g. "COM7". "auto" finds it by handshake.',
        f"port = {_toml_str(cfg.port)}",
        "",
        "# Volume change per knob click, in percent.",
        f"step_percent = {cfg.step_percent:g}",
        "",
        "# Settings page: http://localhost:<web_port>. pad_drive: the CIRCUITPY drive, or \"auto\".",
        f"web_port = {int(cfg.web_port)}",
        f"pad_drive = {_toml_str(cfg.pad_drive)}",
        "",
        "# Four app knobs, left to right. \"label\" is shown on the OLED (first 7 characters).",
        "# \"apps\" lists process names as shown in Task Manager > Details, plus these specials:",
        "#   \"@focused\"  the app in the foreground window",
        "#   \"@system\"   Windows system sounds",
    ]
    for ch in cfg.channels:
        apps = ", ".join(_toml_str(a) for a in ch.apps)
        lines += ["", "[[channel]]", f"label = {_toml_str(ch.label)}", f"apps = [{apps}]"]
    lines += ["", "# What keys set to \"Open on PC\" open: a program, file, folder or URL.",
              "# Keys \"1\"-\"8\", or \"master\" for the master knob push.", "[pc_actions]"]
    for key in protocol.KEY_IDS:
        if cfg.pc_actions.get(key):
            lines.append(f"{_toml_str(key)} = {_toml_str(cfg.pc_actions[key])}")
    return "\n".join(lines) + "\n"


def save_config(path: Path, cfg: Config) -> None:
    tmp = path.with_suffix(".tmp")
    tmp.write_text(dump_config(cfg), encoding="utf-8")
    os.replace(tmp, path)


def open_target(target: str) -> None:
    """Opens a program, file, folder or URL the way a double-click would."""
    target = target.strip()
    if not target:
        return
    log.info("opening %s", target)
    try:
        if sys.platform == "win32":
            os.startfile(target)  # noqa: S606 - the user configured this target
        elif sys.platform == "darwin":
            subprocess.Popen(["open", target])
        else:
            subprocess.Popen(["xdg-open", target])
    except OSError as err:
        log.warning("could not open %s: %s", target, err)


# ---------------------------------------------------------------------------
# Finding the macropad
# ---------------------------------------------------------------------------

def _probe(device: str) -> Optional[str]:
    """The firmware version if the port answers the handshake like a Pico Mixpad."""
    try:
        with serial.Serial(device, BAUD, timeout=0.3, write_timeout=0.5) as s:
            s.reset_input_buffer()
            s.write(protocol.HANDSHAKE.encode())
            deadline = time.monotonic() + 1.5
            while time.monotonic() < deadline:
                msg = protocol.parse_device_line(s.readline().decode("ascii", "ignore"))
                if isinstance(msg, protocol.Hello):
                    log.info("found macropad firmware %s on %s", msg.version, device)
                    return msg.version
    except (serial.SerialException, OSError):
        pass
    return None


def find_port(configured: str):
    """(port, firmware version or None), or (None, None) if no macropad was found."""
    if configured.lower() != "auto":
        return configured, None
    ports = list(list_ports.comports())
    likely = [p for p in ports if p.vid in KNOWN_VIDS]
    for p in likely or ports:      # only probe unknown ports if no Pico-like port exists
        version = _probe(p.device)
        if version:
            return p.device, version
    return None, None


# ---------------------------------------------------------------------------
# One connected session
# ---------------------------------------------------------------------------

class Bridge:
    """Glue between the serial link and the audio backend. Backend-agnostic so it can be tested."""

    def __init__(self, link, audio, config: Config, status: Optional[dict] = None, opener=open_target):
        self.link = link
        self.audio = audio
        self.config = config
        self.status = status if status is not None else {}
        self.opener = opener
        self.last_push = 0.0
        self.dirty = True

    def identifying(self) -> bool:
        """True while the settings page is finding keys: inputs are reported, not acted on."""
        return time.monotonic() < self.status.get("identify_until", 0)

    def _report(self, kind: str, number: int) -> None:
        seq = self.status.get("input", {}).get("seq", 0) + 1
        self.status["input"] = {"kind": kind, "n": number, "seq": seq}

    def handle(self, msg) -> bool:
        """Act on one device message. Returns True if the OLED should be refreshed now."""
        chans = self.config.channels
        if isinstance(msg, protocol.Button):
            self._report("button", msg.number)
            return False
        if isinstance(msg, protocol.Turn):
            self._report("knob", msg.channel)
        if self.identifying() and isinstance(msg, (protocol.Turn, protocol.Press, protocol.MicToggle,
                                                   protocol.KeyAction)):
            return False
        if isinstance(msg, protocol.Turn) and 0 <= msg.channel < len(chans):
            self.audio.nudge(chans[msg.channel].apps, msg.delta * self.config.step_percent)
            return True
        if isinstance(msg, protocol.Press) and 0 <= msg.channel < len(chans):
            self.audio.toggle_mute(chans[msg.channel].apps)
            return True
        if isinstance(msg, protocol.MicToggle):
            muted = self.audio.toggle_mic()
            log.info("mic %s", "muted" if muted else "live" if muted is False else "not found")
            return True
        if isinstance(msg, protocol.KeyAction):
            target = self.config.pc_actions.get(msg.key)
            if target:
                self.opener(target)
            else:
                log.info("key %s has no PC action set", msg.key)
            return False
        return False

    def push_state(self) -> None:
        lines, levels = [], []
        for i, ch in enumerate(self.config.channels):
            volume, muted = self.audio.level(ch.apps)
            levels.append({"volume": None if volume is None else round(volume), "muted": muted})
            lines.append(protocol.channel_line(i, ch.label, volume, muted))
        mic = self.audio.mic_muted()
        lines.append(protocol.mic_line(mic))
        self.link.write("".join(lines).encode("ascii"))
        self.last_push = time.monotonic()
        self.status.update(levels=levels, mic=mic)
        names = getattr(self.audio, "session_names", None)
        if names is not None:      # read here, on the audio thread, so the web page never touches COM
            self.status["audio_apps"] = names()

    def step(self) -> None:
        """Wait up to READ_TIMEOUT_S for one line, act on it, refresh the OLED when needed."""
        outbox = self.status.get("outbox")
        while outbox:                       # lines queued by the settings page (e.g. QUIET)
            self.link.write(outbox.pop(0).encode("ascii"))
        raw = self.link.readline()
        if raw:
            msg = protocol.parse_device_line(raw.decode("ascii", "ignore"))
            if msg is not None and self.handle(msg):
                self.dirty = True
        since = time.monotonic() - self.last_push
        # After a change: refresh at most 20x/s so a fast spin doesn't flood the link.
        if (self.dirty and since >= MIN_PUSH_GAP_S) or since >= STATE_PUSH_S:
            self.push_state()
            self.dirty = False


class ConfigWatcher:
    """Reloads config.toml when it changes (e.g. saved from the settings page)."""

    def __init__(self, path: Path, config: Config):
        self.path = path
        self.config = config
        self.mtime = self._mtime()
        self.checked = time.monotonic()

    def _mtime(self):
        try:
            return self.path.stat().st_mtime
        except OSError:
            return None

    def poll(self) -> Optional[Config]:
        """The new Config if the file changed since the last call, else None."""
        if time.monotonic() - self.checked < CONFIG_CHECK_S:
            return None
        self.checked = time.monotonic()
        mtime = self._mtime()
        if mtime == self.mtime:
            return None
        self.mtime = mtime
        try:
            self.config = load_config(self.path)
        except (OSError, ValueError, tomllib.TOMLDecodeError) as err:
            log.warning("config.toml not reloaded: %s", err)
            return None
        log.info("config.toml reloaded")
        return self.config


def run(watcher: ConfigWatcher, audio, status: dict) -> None:
    while True:
        port, version = find_port(watcher.config.port)
        if port is None:
            status.update(connected=False, port=None)
            time.sleep(3)
            watcher.poll()
            continue
        try:
            with serial.Serial(port, BAUD, timeout=READ_TIMEOUT_S, write_timeout=1) as link:
                log.info("connected on %s", port)
                status.update(connected=True, port=port, firmware=version)
                bridge = Bridge(link, audio, watcher.config, status)
                while True:
                    bridge.step()
                    new = watcher.poll()
                    if new is not None:
                        bridge.config = new
                        bridge.dirty = True
        except (serial.SerialException, OSError) as err:
            log.warning("connection lost: %s", err)
            status.update(connected=False, port=None)
            time.sleep(2)


def main() -> None:
    ap = argparse.ArgumentParser(description="Pico Mixpad host app")
    ap.add_argument("--settings", action="store_true", help="open the settings page in the browser")
    ap.add_argument("--no-web", action="store_true", help="don't serve the settings page")
    args = ap.parse_args()

    handlers = [RotatingFileHandler(HERE / "macropad_host.log", maxBytes=200_000, backupCount=1)]
    if sys.stdout is not None:          # pythonw has no console
        handlers.append(logging.StreamHandler())
    logging.basicConfig(level=logging.INFO, handlers=handlers,
                        format="%(asctime)s %(levelname)s %(message)s")

    config = load_config(CONFIG_PATH)
    from audio_win import WindowsAudio  # Windows-only import, kept here so tests run anywhere
    audio = WindowsAudio()
    status = {"connected": False, "port": None, "firmware": None, "levels": [], "mic": None, "outbox": []}

    if not args.no_web:
        import webui
        server = webui.make_server(CONFIG_PATH, status, config.web_port)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        url = f"http://localhost:{config.web_port}"
        log.info("settings page: %s", url)
        if args.settings:
            webbrowser.open(url)

    log.info("starting, channels: %s", ", ".join(c.label for c in config.channels))
    run(ConfigWatcher(CONFIG_PATH, config), audio, status)


if __name__ == "__main__":
    main()
