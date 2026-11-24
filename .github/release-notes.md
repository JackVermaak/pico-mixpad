Everything needed to build a Pico Mixpad without cloning the repository.

| File | What it is |
| --- | --- |
| `pico-mixpad.exe` | The Windows app, written in Rust. Put it in its own folder and double-click it: no install and no Python. Its tray icon opens the settings page. |
| `pico-mixpad-firmware-*.zip` | `boot.py`, `code.py` and `mixpad_settings.py`. Copy them to the `CIRCUITPY` drive (CircuitPython 10.x, Raspberry Pi Pico 2 build), then unplug and replug. |
| `pico-mixpad-case-*.zip` | Print-ready STLs in print orientation, the parametric OpenSCAD source and the print guide. Print `test.stl` and `oled_test.stl` first. |
| `pico-mixpad-host-*.zip` | The same Windows app in Python, with the settings page. `python -m pip install -r requirements.txt`, then `python macropad_host.py --settings`. |

Before flashing, install the libraries: `circup install adafruit_hid adafruit_display_text adafruit_displayio_ssd1306`.

Full build guide (parts, wiring, assembly, testing): [docs/BUILD.md](https://github.com/JackVermaak/pico-mixpad/blob/main/docs/BUILD.md).
