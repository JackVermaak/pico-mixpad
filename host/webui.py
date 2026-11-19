"""Settings web app for the Pico Mixpad, served on http://localhost:8765.

Started by macropad_host.py. Can also run on its own to edit settings while the host app
is closed (or on a Mac/Linux machine):  python webui.py

What it edits:
  config.toml          host side: app knob labels and apps, volume step, PC actions
  CIRCUITPY/settings.json   pad side: key actions, knob direction, screen, LED
"""

import json
import sys
import time
import webbrowser
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

import circuitpy
import hid_names
import macropad_host as host
import mixpad_settings
import protocol

HERE = Path(__file__).resolve().parent
PAGE = HERE / "web" / "index.html"
LABEL_MAX = 10
APPS_MAX = 20
TARGET_MAX = 500


class BadRequest(ValueError):
    pass


def config_to_dict(cfg: host.Config) -> dict:
    return {
        "port": cfg.port,
        "stepPercent": cfg.step_percent,
        "channels": [{"label": c.label, "apps": list(c.apps)} for c in cfg.channels],
        "pcActions": {k: cfg.pc_actions.get(k, "") for k in protocol.KEY_IDS},
        "padDrive": cfg.pad_drive,
        "webPort": cfg.web_port,
    }


def dict_to_config(d: dict, base: host.Config) -> host.Config:
    """Validates the page's host settings. Raises BadRequest with a readable message."""
    if not isinstance(d, dict):
        raise BadRequest("host settings missing")
    try:
        step = float(d.get("stepPercent", base.step_percent))
    except (TypeError, ValueError):
        raise BadRequest("volume step must be a number")
    if not 0.5 <= step <= 20:
        raise BadRequest("volume step must be between 0.5 and 20 %")
    chans = d.get("channels")
    if not isinstance(chans, list) or len(chans) != 4:
        raise BadRequest("expected 4 app knobs")
    channels = []
    for i, c in enumerate(chans):
        label = str(c.get("label", "")).strip()
        if not label or len(label) > LABEL_MAX or not label.isascii() or not label.isprintable():
            raise BadRequest(f"knob {i + 1}: label must be 1-{LABEL_MAX} plain characters")
        apps = c.get("apps", [])
        if not isinstance(apps, list) or len(apps) > APPS_MAX:
            raise BadRequest(f"knob {i + 1}: too many apps")
        apps = [str(a).strip() for a in apps if str(a).strip()]
        channels.append(host.Channel(label, apps))
    actions = {}
    for key, target in (d.get("pcActions") or {}).items():
        target = str(target).strip()
        if key in protocol.KEY_IDS and target:
            if len(target) > TARGET_MAX:
                raise BadRequest(f"key {key}: the PC action is too long")
            actions[key] = target
    port = str(d.get("port", base.port)).strip() or "auto"
    pad_drive = str(d.get("padDrive", base.pad_drive)).strip() or "auto"
    return host.Config(port=port, step_percent=step, channels=channels, pc_actions=actions,
                       web_port=base.web_port, pad_drive=pad_drive)


class Handler(BaseHTTPRequestHandler):
    server_version = "PicoMixpad"

    # ---- helpers ----------------------------------------------------------

    def log_message(self, fmt, *args):  # keep the console quiet
        pass

    def _host_ok(self) -> bool:
        """Only answer requests addressed to this machine (blocks DNS-rebinding tricks)."""
        name = (self.headers.get("Host") or "").rsplit(":", 1)[0]
        return name in ("localhost", "127.0.0.1", "[::1]")

    def _send(self, code: int, body: bytes, ctype: str) -> None:
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.send_header("X-Content-Type-Options", "nosniff")
        self.end_headers()
        self.wfile.write(body)

    def _json(self, code: int, data) -> None:
        self._send(code, json.dumps(data).encode(), "application/json")

    def _drive(self):
        cfg = self.server.load()
        return circuitpy.find_drive(cfg.pad_drive)

    def _status(self) -> dict:
        drive = self._drive()
        raw = self.server.status
        st = {k: v for k, v in raw.items() if k not in ("outbox", "identify_until")}
        st["identifying"] = time.monotonic() < raw.get("identify_until", 0)
        return {"status": st, "padDrive": None if drive is None else str(drive)}

    # ---- routes -------------------------------------------------------------

    def do_GET(self):
        if not self._host_ok():
            return self._send(403, b"forbidden", "text/plain")
        path = self.path.split("?", 1)[0]
        if path in ("/", "/index.html"):
            return self._send(200, PAGE.read_bytes(), "text/html; charset=utf-8")
        if path == "/api/state":
            cfg = self.server.load()
            drive = circuitpy.find_drive(cfg.pad_drive)
            data = self._status()
            data.update(host=config_to_dict(cfg), pad=circuitpy.read_settings(drive),
                        defaults=mixpad_settings.DEFAULTS, options=hid_names.for_ui(),
                        keyIds=list(protocol.KEY_IDS))
            return self._json(200, data)
        if path == "/api/status":
            return self._json(200, self._status())
        self._send(404, b"not found", "text/plain")

    def do_POST(self):
        if not self._host_ok():
            return self._send(403, b"forbidden", "text/plain")
        # A custom header can't be sent cross-site without a CORS preflight, which we never allow.
        if self.headers.get("X-Mixpad") != "1":
            return self._send(403, b"forbidden", "text/plain")
        if self.path == "/api/identify":
            return self._identify()
        if self.path != "/api/save":
            return self._send(404, b"not found", "text/plain")
        try:
            length = int(self.headers.get("Content-Length") or 0)
            if length > 200_000:
                raise BadRequest("request too large")
            body = json.loads(self.rfile.read(length) or b"{}")
            base = self.server.load()
            cfg = dict_to_config(body.get("host"), base)
            pad = body.get("pad")
            if not isinstance(pad, dict):
                raise BadRequest("pad settings missing")
        except (BadRequest, ValueError) as err:
            return self._json(400, {"error": str(err)})

        host.save_config(self.server.config_path, cfg)
        drive = circuitpy.find_drive(cfg.pad_drive)
        pad_written, pad_error = False, None
        if drive is not None:
            try:
                circuitpy.write_settings(drive, pad)
                pad_written = True
            except OSError as err:
                pad_error = f"could not write to {drive}: {err}"
        self._json(200, {"ok": True, "padWritten": pad_written, "padError": pad_error,
                         "padDrive": None if drive is None else str(drive),
                         "pad": circuitpy.clean_settings(pad)})


    def _identify(self):
        """Key finder: the pad reports presses without acting on them, for up to 60 s."""
        try:
            length = int(self.headers.get("Content-Length") or 0)
            on = bool(json.loads(self.rfile.read(min(length, 1000)) or b"{}").get("on"))
        except ValueError:
            return self._json(400, {"error": "bad request"})
        st = self.server.status
        st["identify_until"] = time.monotonic() + 60 if on else 0
        st.setdefault("outbox", []).append(protocol.quiet_line(on))
        self._json(200, {"ok": True, "identifying": on})


class SettingsServer(ThreadingHTTPServer):
    daemon_threads = True

    def __init__(self, config_path: Path, status: dict, port: int):
        self.config_path = Path(config_path)
        self.status = status
        super().__init__(("127.0.0.1", port), Handler)

    def load(self) -> host.Config:
        return host.load_config(self.config_path)


def make_server(config_path: Path, status: dict, port: int = 8765) -> SettingsServer:
    return SettingsServer(config_path, status, port)


def main():
    cfg = host.load_config(host.CONFIG_PATH)
    status = {"connected": False, "port": None, "firmware": None, "levels": [], "mic": None,
              "standalone": True}
    server = make_server(host.CONFIG_PATH, status, cfg.web_port)
    url = f"http://localhost:{cfg.web_port}"
    print(f"Pico Mixpad settings: {url}  (Ctrl+C to stop)")
    if "--no-browser" not in sys.argv:
        webbrowser.open(url)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass


if __name__ == "__main__":
    main()
