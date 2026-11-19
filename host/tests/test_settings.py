"""Tests for the pad settings, config.toml writing, PC actions and the settings web app."""

import json
import sys
import threading
import urllib.error
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

import circuitpy  # noqa: E402
import hid_names  # noqa: E402
import macropad_host as host  # noqa: E402
import mixpad_settings as ms  # noqa: E402
import protocol  # noqa: E402
import webui  # noqa: E402


# ---- pad settings (shared with the firmware) ---------------------------------

def test_host_copy_matches_firmware():
    firmware = ROOT.parent / "firmware" / "mixpad_settings.py"
    assert (ROOT / "mixpad_settings.py").read_text() == firmware.read_text(), \
        "host/mixpad_settings.py must be an exact copy of firmware/mixpad_settings.py"


def test_defaults_when_missing_or_garbage():
    assert ms.normalise(None) == ms.DEFAULTS
    assert ms.normalise("nope") == ms.DEFAULTS
    assert ms.normalise({"keys": "x", "screen": 5, "encoder_divisor": 3}) == ms.DEFAULTS


def test_valid_values_are_kept_and_bad_ones_dropped():
    raw = {
        "keys": [{"type": "text", "text": "gg wp"}, {"type": "pc"}, {"type": "keys", "keys": ["CONTROL", "M"]},
                 {"type": "keys", "keys": ["NOT_A_KEY"]}, {"type": "media", "code": "NOPE"},
                 {"type": "text", "text": "é"}, {"type": "bogus"}, {"type": "none"}],
        "master_push": {"type": "pc"},
        "encoder_divisor": 2, "master_reversed": True, "app_reversed": "yes",
        "screen": {"rotation": 90, "mic_row": "top", "dim_after_s": True, "off_after_s": 30,
                   "dim_level": 2, "address": 0x3D},
        "mic_led_offline": "blink",
    }
    out = ms.normalise(raw, hid_names.KEYCODE, hid_names.MEDIA_CODE)
    assert out["keys"][:3] == raw["keys"][:3]
    assert out["keys"][3:7] == ms.DEFAULTS["keys"][3:7]      # invalid ones fall back to the default
    assert out["keys"][7] == {"type": "none"}
    assert out["master_push"] == {"type": "pc"}
    assert out["encoder_divisor"] == 2 and out["master_reversed"] is True and out["app_reversed"] is False
    assert out["screen"] == {"rotation": 180, "mic_row": "top", "dim_after_s": 60, "off_after_s": 30,
                             "dim_level": 0.05, "address": 0x3D}
    assert out["mic_led_offline"] == "blink"


def test_row_positions_follow_the_yellow_strip():
    s = dict(ms.DEFAULTS["screen"])
    assert ms.row_positions(dict(s, rotation=0))[0] == 2          # mic line at the top
    assert ms.row_positions(dict(s, rotation=180))[0] == 51       # mic line at the bottom
    assert ms.row_positions(dict(s, rotation=180, mic_row="top"))[0] == 2
    for rot in (0, 180):
        ys = ms.row_positions(dict(s, rotation=rot))
        assert max(ys) + 12 <= 64 and len(set(ys)) == 5


# ---- config.toml -------------------------------------------------------------

def test_dump_and_load_round_trip(tmp_path):
    cfg = host.Config(port="COM7", step_percent=2.5, web_port=9000, pad_drive="E:\\",
                      channels=[host.Channel('Ga"me', ["cs2.exe", "@focused"]), host.Channel("B", []),
                                host.Channel("C", ["x.exe"]), host.Channel("D", ["@system"])],
                      pc_actions={"2": r"C:\Program Files\Spotify\Spotify.exe", "master": "https://example.com"})
    path = tmp_path / "config.toml"
    host.save_config(path, cfg)
    assert host.load_config(path) == cfg


# ---- PC actions --------------------------------------------------------------

class Link:
    def __init__(self, lines):
        self.lines = list(lines)
        self.written = b""

    def readline(self):
        return self.lines.pop(0).encode() if self.lines else b""

    def write(self, data):
        self.written += data


class Audio:
    def level(self, apps):
        return None, False

    def mic_muted(self):
        return None

    def session_names(self):
        return ["cs2.exe"]


def test_key_message_opens_the_configured_target():
    assert protocol.parse_device_line("K 3") == protocol.KeyAction("3")
    assert protocol.parse_device_line("K master") == protocol.KeyAction("master")
    assert protocol.parse_device_line("K 9") is None
    opened = []
    cfg = host.Config(channels=[host.Channel("A", [])] * 4, pc_actions={"3": "notepad.exe"})
    status = {}
    bridge = host.Bridge(Link(["K 3\n", "K 4\n"]), Audio(), cfg, status, opener=opened.append)
    bridge.step()
    bridge.step()
    assert opened == ["notepad.exe"]
    assert status["audio_apps"] == ["cs2.exe"]


def test_key_finder_reports_inputs_without_acting():
    assert protocol.parse_device_line("B 12") == protocol.Button(12)
    assert protocol.parse_device_line("B 13") is None
    nudged = []

    class A(Audio):
        def nudge(self, apps, pct):
            nudged.append(pct)

    cfg = host.Config(channels=[host.Channel("A", ["a.exe"])] * 4)
    status = {"outbox": ["QUIET 1\n"], "identify_until": host.time.monotonic() + 60}
    link = Link(["B 5\n", "E 2 1\n"])
    bridge = host.Bridge(link, A(), cfg, status)
    bridge.step()
    assert link.written.startswith(b"QUIET 1\n") and status["outbox"] == []
    assert status["input"] == {"kind": "button", "n": 5, "seq": 1}
    bridge.step()
    assert status["input"] == {"kind": "knob", "n": 2, "seq": 2} and nudged == []   # reported, not acted on
    status["identify_until"] = 0
    bridge.link = Link(["E 2 1\n"])
    bridge.step()
    assert nudged == [2.0]


def test_layout_must_be_a_full_reordering():
    good = {"layout": {"keys": [4, 5, 6, 7, 0, 1, 2, 3], "knobs": [3, 2, 1, 0]}}
    assert ms.normalise(good)["layout"] == good["layout"]
    for bad in ([0, 0, 1, 2, 3, 4, 5, 6], [0, 1, 2], "x", [True, 1, 2, 3, 4, 5, 6, 7]):
        assert ms.normalise({"layout": {"keys": bad}})["layout"]["keys"] == list(range(8))


def test_config_watcher_reloads_changes(tmp_path):
    path = tmp_path / "config.toml"
    host.save_config(path, host.Config(channels=[host.Channel("A", [])] * 4))
    watcher = host.ConfigWatcher(path, host.load_config(path))
    host.save_config(path, host.Config(step_percent=7, channels=[host.Channel("A", [])] * 4))
    watcher.mtime = None          # pretend the timestamp changed (filesystems can be coarse)
    watcher.checked = 0
    assert watcher.poll().step_percent == 7


# ---- settings web app ---------------------------------------------------------

def start_server(tmp_path):
    drive = tmp_path / "CIRCUITPY"
    drive.mkdir()
    (drive / "code.py").write_text("")
    cfg_path = tmp_path / "config.toml"
    host.save_config(cfg_path, host.Config(pad_drive=str(drive), channels=[host.Channel("A", [])] * 4))
    server = webui.make_server(cfg_path, {"connected": False}, 0)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server, f"http://127.0.0.1:{server.server_address[1]}", drive, cfg_path


def request(url, body=None, headers=None):
    data = None if body is None else json.dumps(body).encode()
    req = urllib.request.Request(url, data=data, headers=headers or {})
    try:
        with urllib.request.urlopen(req) as r:
            return r.status, json.loads(r.read() or b"null")
    except urllib.error.HTTPError as err:
        text = err.read()
        try:
            return err.code, json.loads(text)
        except ValueError:
            return err.code, text.decode()


def test_web_state_and_save(tmp_path):
    server, url, drive, cfg_path = start_server(tmp_path)
    try:
        code, state = request(url + "/api/state")
        assert code == 200 and state["padDrive"] == str(drive)
        assert state["pad"] == ms.DEFAULTS and len(state["host"]["channels"]) == 4

        state["host"]["channels"][0] = {"label": "Game", "apps": ["cs2.exe"]}
        state["host"]["pcActions"]["2"] = "notepad.exe"
        state["pad"]["keys"][1] = {"type": "pc"}
        state["pad"]["screen"]["rotation"] = 0
        hdr = {"Content-Type": "application/json", "X-Mixpad": "1"}
        code, res = request(url + "/api/save", {"host": state["host"], "pad": state["pad"]}, hdr)
        assert code == 200 and res["padWritten"]
        written = json.loads((drive / "settings.json").read_text())
        assert written["keys"][1] == {"type": "pc"} and written["screen"]["rotation"] == 0
        cfg = host.load_config(cfg_path)
        assert cfg.channels[0] == host.Channel("Game", ["cs2.exe"]) and cfg.pc_actions == {"2": "notepad.exe"}

        bad = dict(state["host"], channels=state["host"]["channels"][:3])
        code, res = request(url + "/api/save", {"host": bad, "pad": state["pad"]}, hdr)
        assert code == 400 and "4 app knobs" in res["error"]

        code, res = request(url + "/api/identify", {"on": True}, hdr)
        assert code == 200 and server.status["outbox"] == ["QUIET 1\n"]
        assert request(url + "/api/status")[1]["status"]["identifying"] is True

        code, _ = request(url + "/api/save", {"host": state["host"], "pad": state["pad"]},
                          {"Content-Type": "application/json"})
        assert code == 403                            # no X-Mixpad header: refused
        code, _ = request(url + "/api/state", headers={"Host": "evil.example"})
        assert code == 403                            # not addressed to localhost: refused
    finally:
        server.shutdown()
        server.server_close()


def test_settings_survive_without_the_drive(tmp_path):
    assert circuitpy.read_settings(None) == ms.DEFAULTS
    assert circuitpy.find_drive(str(tmp_path / "missing")) is None
