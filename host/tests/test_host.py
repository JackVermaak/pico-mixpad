"""Tests for the protocol and the Bridge logic. Run from the host/ folder:  python -m pytest
They use a fake serial link and fake audio backend, so they run on any OS (no macropad, no Windows)."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import macropad_host as host  # noqa: E402
import protocol  # noqa: E402


# ---- protocol --------------------------------------------------------------

def test_parse_device_messages():
    assert protocol.parse_device_line("E 2 -3\n") == protocol.Turn(2, -3)
    assert protocol.parse_device_line("E 0 1") == protocol.Turn(0, 1)
    assert protocol.parse_device_line("P 3\r\n") == protocol.Press(3)
    assert protocol.parse_device_line("M\n") == protocol.MicToggle()
    assert protocol.parse_device_line("HELLO MACROPAD 1.0") == protocol.Hello("1.0")


def test_parse_rejects_garbage():
    for bad in ["", "   ", "E", "E x 1", "P", "M 1", "HELLO", "HELLO OTHER 1", "Z 1 2", "E 1 2 3"]:
        assert protocol.parse_device_line(bad) is None, bad


def test_channel_line_formatting():
    assert protocol.channel_line(0, "Game", 71.6, False) == "V 0 72 0 Game\n"
    assert protocol.channel_line(3, "Music", None, False) == "V 3 - 0 Music\n"
    assert protocol.channel_line(1, "Discord", 150, True) == "V 1 100 1 Discord\n"
    assert protocol.channel_line(2, "Browsér\nX", 0, False) == "V 2 0 0 BrowsrX\n"
    assert protocol.channel_line(2, "A very long label", 5, False) == "V 2 5 0 A very lon\n"
    assert protocol.channel_line(2, "éé", 5, False) == "V 2 5 0 ?\n"


def test_mic_line():
    assert protocol.mic_line(True) == "MIC 1\n"
    assert protocol.mic_line(False) == "MIC 0\n"
    assert protocol.mic_line(None) == "MIC -\n"


# ---- fakes -----------------------------------------------------------------

class FakeLink:
    def __init__(self, incoming):
        self.incoming = list(incoming)
        self.written = b""

    def readline(self):
        return self.incoming.pop(0).encode() if self.incoming else b""

    def write(self, data):
        self.written += data


class FakeAudio:
    def __init__(self):
        self.volumes = {"cs2.exe": 50.0, "discord.exe": 80.0}
        self.muted = set()
        self.mic = False

    def _hits(self, apps):
        return [a for a in apps if a in self.volumes]

    def nudge(self, apps, percent):
        for a in self._hits(apps):
            self.volumes[a] = min(100.0, max(0.0, self.volumes[a] + percent))

    def toggle_mute(self, apps):
        for a in self._hits(apps):
            self.muted ^= {a}

    def level(self, apps):
        hits = self._hits(apps)
        return (self.volumes[hits[0]], hits[0] in self.muted) if hits else (None, False)

    def mic_muted(self):
        return self.mic

    def toggle_mic(self):
        self.mic = not self.mic
        return self.mic


def make_config():
    return host.Config(step_percent=2, channels=[
        host.Channel("Game", ["cs2.exe"]),
        host.Channel("Discord", ["discord.exe"]),
        host.Channel("Browser", ["chrome.exe"]),
        host.Channel("Music", ["spotify.exe"]),
    ])


# ---- bridge ----------------------------------------------------------------

def test_turn_changes_volume_and_refreshes_display():
    audio = FakeAudio()
    link = FakeLink(["E 0 3\n", "E 1 -5\n"])
    bridge = host.Bridge(link, audio, make_config())
    bridge.step()
    assert audio.volumes["cs2.exe"] == 56.0
    assert b"V 0 56 0 Game\n" in link.written
    bridge.last_push = 0  # pretend the 50 ms gap has passed
    bridge.step()
    assert audio.volumes["discord.exe"] == 70.0
    assert b"V 1 70 0 Discord\n" in link.written


def test_volume_is_clamped():
    audio = FakeAudio()
    bridge = host.Bridge(FakeLink(["E 0 100\n"]), audio, make_config())
    bridge.step()
    assert audio.volumes["cs2.exe"] == 100.0


def test_press_mutes_app_and_mic_toggle():
    audio = FakeAudio()
    link = FakeLink(["P 1\n", "M\n"])
    bridge = host.Bridge(link, audio, make_config())
    bridge.step()
    bridge.last_push = 0
    bridge.step()
    assert "discord.exe" in audio.muted
    assert audio.mic is True
    assert link.written.endswith(b"MIC 1\n")
    assert b"V 1 80 1 Discord\n" in link.written


def test_app_not_running_and_bad_channel():
    audio = FakeAudio()
    link = FakeLink(["E 9 1\n", "garbage\n"])
    bridge = host.Bridge(link, audio, make_config())
    bridge.step()
    bridge.step()
    assert audio.volumes == {"cs2.exe": 50.0, "discord.exe": 80.0}
    assert b"V 3 - 0 Music\n" in link.written


def test_idle_step_sends_heartbeat_only_once_per_second():
    link = FakeLink([])
    bridge = host.Bridge(link, FakeAudio(), make_config())
    bridge.step()                      # first push (nothing sent yet)
    first = len(link.written)
    bridge.step()                      # within 1 s: nothing new
    assert len(link.written) == first
    bridge.last_push -= 1.0
    bridge.step()
    assert len(link.written) == 2 * first


def test_load_config(tmp_path):
    cfg_file = tmp_path / "config.toml"
    cfg_file.write_text('port = "COM7"\nstep_percent = 5\n[[channel]]\nlabel = "Game"\napps = ["cs2.exe"]\n')
    cfg = host.load_config(cfg_file)
    assert cfg.port == "COM7" and cfg.step_percent == 5
    assert [c.label for c in cfg.channels] == ["Game", "App 2", "App 3", "App 4"]


def test_shipped_config_is_valid():
    cfg = host.load_config(Path(__file__).resolve().parents[1] / "config.toml")
    assert len(cfg.channels) == 4 and cfg.port == "auto"
