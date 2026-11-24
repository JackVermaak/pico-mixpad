# Pico Mixpad: Windows app (Rust)

The PC side of the Pico Mixpad as one small Windows program: no Python, no terminal window.
It does everything the Python app in `../host` does, with the same serial protocol, the same
`config.toml` and the same settings page:

- per-app volume and mute for the four app knobs (Windows Core Audio)
- mic mute and the red/green LED
- "Open on PC" keys
- the settings page on http://localhost:8765, including the key finder
- an icon in the notification area: left-click opens the settings page, right-click shows the
  connection status and **Quit**

| | Python app | This app |
| --- | --- | --- |
| Needs installed | Python 3.11+ and pip packages | nothing |
| Download | folder of scripts | one ~0.9 MB `.exe` |
| Memory (rough) | tens of MB | a few MB |
| Start/stop | terminal window | tray icon |

## Use it

1. Get `pico-mixpad.exe`: from the repo's **Releases** page, from the latest **Actions** run
   (artifact `pico-mixpad-windows`), or build it (below).
2. Put it in its own folder, e.g. `C:\Users\<you>\Apps\Pico Mixpad\`. It keeps `config.toml` and
   `macropad_host.log` next to itself, and creates a default `config.toml` on first start.
   Coming from the Python app? Copy your `host\config.toml` into that folder.
3. Double-click it. The tray icon (bottom right; it may be under the ^ arrow) shows a green dot
   when the pad is connected. Click it to open the settings page.

Starting it again while it runs just opens the settings page. To start it with Windows, put a
shortcut to it in the `shell:startup` folder (Win + R, `shell:startup`).

Options: `--settings` opens the settings page on start; `--no-tray` runs without the tray icon.
Started from a terminal, it also prints its log there.

## Build it

With [Rust](https://rustup.rs) installed:

```
cd host-rs
cargo test
cargo build --release
```

The program is `target\release\pico-mixpad.exe`. The tests run on any OS: everything except the
Windows audio, tray and drive code is platform-independent and tested with fakes.

## Code layout

| File | What it does |
| --- | --- |
| `src/main.rs` | Start-up, the connect / reconnect loop |
| `src/bridge.rs` | One connected session: pad messages in, volume changes and OLED state out |
| `src/protocol.rs` | The text protocol (same as `host/protocol.py`) |
| `src/serial.rs` | Finding the pad by handshake, line-based serial link |
| `src/audio.rs` | `Audio` interface and the Windows Core Audio implementation |
| `src/config.rs` | `config.toml` read/write (byte-identical to the Python app's) and live reload |
| `src/padsettings.rs` | `settings.json` defaults and validation (same rules as the firmware) |
| `src/drive.rs` | Finding the `CIRCUITPY` drive, reading/writing `settings.json` |
| `src/web.rs` | Settings page and API (`host/web/index.html` is built into the program) |
| `src/tray.rs` | Notification-area icon and menu |
| `src/actions.rs` | Opening programs, files and URLs for "Open on PC" keys |
