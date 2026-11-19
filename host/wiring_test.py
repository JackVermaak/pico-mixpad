"""Wiring test for the Pico Mixpad - runs on macOS, Windows or Linux.

Pretends to be the PC app with four fake channels, so you can check every knob,
the mic key, the OLED and the red/green LED without Windows audio.

    pip3 install pyserial
    python3 wiring_test.py            # finds the macropad by itself
    python3 wiring_test.py /dev/cu.usbmodem1103   # or name the data port

What to expect:
    app knobs 1-4   turn -> that bar moves on the OLED and the line prints here
                    push -> that channel shows muted
    key 1           mic toggles: LED red (muted) / green (live), banner on the OLED
    master knob     changes the computer's own volume (no output here)
    keys 2-8        send media keys / F16-F20 - check them in a key viewer
Ctrl-C to quit.
"""

import sys
import time

import serial
from serial.tools import list_ports

LABELS = ["Knob 1", "Knob 2", "Knob 3", "Knob 4"]


def find_port():
    for p in list_ports.comports():
        if "Bluetooth" in p.device or "debug-console" in p.device:
            continue
        try:
            s = serial.Serial(p.device, 115200, timeout=0.3)
        except serial.SerialException:
            continue
        s.reset_input_buffer()
        s.write(b"?\n")
        end = time.monotonic() + 1.0
        while time.monotonic() < end:
            line = s.readline().decode("ascii", "ignore").strip()
            if line.startswith("HELLO MACROPAD"):
                print(f"Found macropad on {p.device} (firmware {line.split()[-1]})")
                return s
        s.close()
    return None


def main():
    if len(sys.argv) > 1:
        port = serial.Serial(sys.argv[1], 115200, timeout=0.05)
    else:
        port = find_port()
        if port is None:
            sys.exit("Macropad not found. Is it plugged in, with boot.py copied and the Pico replugged?")
    port.timeout = 0.05

    vol = [50, 50, 50, 50]
    muted = [False] * 4
    mic = False
    last_send = 0.0
    print(__doc__.split("What to expect:")[1].split("Ctrl-C")[0])
    try:
        while True:
            line = port.readline().decode("ascii", "ignore").strip()
            parts = line.split()
            if parts[:1] == ["E"] and len(parts) == 3:
                ch, d = int(parts[1]), int(parts[2])
                vol[ch] = max(0, min(100, vol[ch] + 2 * d))
                way = "clockwise" if d > 0 else "anticlockwise"
                print(f"App knob {ch + 1} turned {way:13} -> {vol[ch]:3d} %")
                last_send = 0
            elif parts[:1] == ["P"] and len(parts) == 2:
                ch = int(parts[1])
                muted[ch] = not muted[ch]
                print(f"App knob {ch + 1} pushed -> {'muted' if muted[ch] else 'unmuted'}")
                last_send = 0
            elif parts == ["M"]:
                mic = not mic
                print(f"Key 1 (mic) -> {'MUTED, LED should be RED' if mic else 'LIVE, LED should be GREEN'}")
                last_send = 0
            if time.monotonic() - last_send > 0.5:
                msg = "".join(f"V {i} {vol[i]} {int(muted[i])} {LABELS[i]}\n" for i in range(4))
                msg += f"MIC {int(mic)}\n"
                port.write(msg.encode())
                last_send = time.monotonic()
    except KeyboardInterrupt:
        print("\nbye")
    except serial.SerialException as err:
        sys.exit(f"Lost the macropad: {err}")


if __name__ == "__main__":
    main()
