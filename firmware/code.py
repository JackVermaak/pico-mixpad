# code.py - Pico Mixpad firmware
# CircuitPython 10.x on Raspberry Pi Pico 2 (RP2350) or Pico (RP2040).
#
# What it does:
#   * 8 keys        -> keyboard shortcuts, media keys, or messages to the PC app
#   * master knob   -> Windows master volume (standard media volume keys, works without the PC app)
#   * 4 app knobs   -> relative steps sent to the PC app, which changes per-app volume
#   * knob pushes   -> master: mute PC (configurable); app knobs: mute that app (via the PC app)
#   * OLED          -> app names, volume bars, mic state (pushed by the PC app)
#   * LED pair      -> red = mic muted, green = mic live, off = PC app offline
#
# Files on CIRCUITPY: boot.py, code.py, mixpad_settings.py, and optionally settings.json
# (written by the settings web app: what each key does, knob direction, screen rotation, ...).
# Required libraries in /lib (install with: circup install adafruit_hid adafruit_display_text adafruit_displayio_ssd1306):
#   adafruit_hid, adafruit_display_text, adafruit_displayio_ssd1306

import time

import board
import busio
import digitalio
import keypad
import rotaryio
import usb_cdc
import usb_hid
from adafruit_hid.consumer_control import ConsumerControl
from adafruit_hid.consumer_control_code import ConsumerControlCode
from adafruit_hid.keyboard import Keyboard
from adafruit_hid.keycode import Keycode

import mixpad_settings

FW_VERSION = "1.3"

# --------------------------------------------------------------------------
# Wiring - fixed by the build
# --------------------------------------------------------------------------

KEY_PINS = (board.GP0, board.GP1, board.GP2, board.GP3,
            board.GP4, board.GP5, board.GP6, board.GP7)

# (A, B, push) - A and B MUST be consecutive GPIO numbers (PIO requirement).
APP_ENCODERS = (
    (board.GP8, board.GP9, board.GP10),
    (board.GP11, board.GP12, board.GP13),
    (board.GP14, board.GP15, board.GP16),
    (board.GP17, board.GP18, board.GP19),
)
MASTER_ENCODER = (board.GP20, board.GP21, board.GP22)

OLED_SDA = board.GP26
OLED_SCL = board.GP27

# Mic status LED pair on one pin (set to None if you don't fit LEDs):
#   GP28 -> 680R -> red LED -> GND     (red = mic muted, pin driven high)
#   3V3  -> 560R -> green LED -> GP28  (green = mic live, pin driven low)
# Both off = PC app offline / no mic (pin switched to input).
MIC_LED_PIN = board.GP28

# --------------------------------------------------------------------------
# Settings - from /settings.json, written by the settings web app.
# Defaults (and what each one means) are in mixpad_settings.py.
# --------------------------------------------------------------------------

SETTINGS = mixpad_settings.load("/settings.json", Keycode, ConsumerControlCode)

ENCODER_DIVISOR = SETTINGS["encoder_divisor"]   # 4 suits 20-detent/20-pulse EC11s; 2 for 30/15 types
MASTER_REVERSED = SETTINGS["master_reversed"]
APP_REVERSED = SETTINGS["app_reversed"]
MASTER_MAX_STEPS = SETTINGS["master_max_steps"]  # volume key presses per loop on a fast spin

SCREEN_ROTATION = SETTINGS["screen"]["rotation"]
SCREEN_ROW_Y = mixpad_settings.row_positions(SETTINGS["screen"])
OLED_ADDRESS = SETTINGS["screen"]["address"]
# OLEDs burn in when they show the same image for months: dim after SCREEN_DIM_AFTER_S seconds
# without input, off after SCREEN_OFF_AFTER_S. Any input or a mic mute change wakes it. 0 = never.
SCREEN_DIM_AFTER_S = SETTINGS["screen"]["dim_after_s"]
SCREEN_OFF_AFTER_S = SETTINGS["screen"]["off_after_s"]
SCREEN_DIM_LEVEL = SETTINGS["screen"]["dim_level"]
MIC_LED_OFFLINE_MODE = SETTINGS["mic_led_offline"]   # "off" or "blink" while the PC app is offline


def resolve_action(action, ident):
    """Turn a settings action into (kind, value) for the main loop."""
    kind = action["type"]
    if kind == "media":
        return ("media", getattr(ConsumerControlCode, action["code"]))
    if kind == "keys":
        return ("keys", tuple(getattr(Keycode, name) for name in action["keys"]))
    if kind == "text":
        return ("text", action["text"])
    if kind == "pc":
        return ("pc", ident)
    return (kind, None)   # "mic" or "none"


KEYMAP = tuple(resolve_action(a, str(i + 1)) for i, a in enumerate(SETTINGS["keys"]))
MASTER_PUSH_ACTION = resolve_action(SETTINGS["master_push"], "master")

HOST_TIMEOUT_MS = 5000  # PC app counts as offline after this long without a message
QUIET_MAX_MS = 60000    # key-finder mode switches itself off after this long

# --------------------------------------------------------------------------
# Hardware setup
# --------------------------------------------------------------------------

def ticks_ms():
    return time.monotonic_ns() // 1000000


# All buttons in one scanner: keys 0-7, app-knob pushes 8-11, master push 12.
button_pins = KEY_PINS + tuple(e[2] for e in APP_ENCODERS) + (MASTER_ENCODER[2],)
buttons = keypad.Keys(button_pins, value_when_pressed=False, pull=True)
FIRST_APP_PUSH = len(KEY_PINS)
MASTER_PUSH = FIRST_APP_PUSH + len(APP_ENCODERS)

app_knobs = [rotaryio.IncrementalEncoder(a, b, divisor=ENCODER_DIVISOR) for a, b, _ in APP_ENCODERS]
master_knob = rotaryio.IncrementalEncoder(MASTER_ENCODER[0], MASTER_ENCODER[1], divisor=ENCODER_DIVISOR)

time.sleep(1)  # give the PC a moment to enumerate the USB HID devices
keyboard = Keyboard(usb_hid.devices)
media = ConsumerControl(usb_hid.devices)

data_port = usb_cdc.data  # None if boot.py did not enable it
if data_port is not None:
    data_port.timeout = 0
    try:
        data_port.write_timeout = 0.05
    except AttributeError:
        pass


# --------------------------------------------------------------------------
# OLED (optional - firmware runs fine without it)
# --------------------------------------------------------------------------

class Screen:
    ROWS = len(SCREEN_ROW_Y)

    def __init__(self):
        self.shown = [None] * self.ROWS
        self.labels = None
        self.display = None
        self.state = "on"            # "on", "dim" or "off"
        self.last_activity = ticks_ms()
        try:
            import displayio
            import i2cdisplaybus
            import terminalio
            import adafruit_displayio_ssd1306
            from adafruit_display_text import label
        except ImportError as err:
            print("OLED disabled, missing library:", err)
            return
        displayio.release_displays()
        try:
            i2c = busio.I2C(scl=OLED_SCL, sda=OLED_SDA, frequency=400000)
            bus = i2cdisplaybus.I2CDisplayBus(i2c, device_address=OLED_ADDRESS)
            display = adafruit_displayio_ssd1306.SSD1306(bus, width=128, height=64, rotation=SCREEN_ROTATION)
        except (ValueError, RuntimeError, OSError) as err:
            print("OLED not found:", err)
            return
        group = displayio.Group()
        self.labels = []
        for row in range(self.ROWS):
            lbl = label.Label(terminalio.FONT, text="", color=0xFFFFFF,
                              anchor_point=(0, 0), anchored_position=(0, SCREEN_ROW_Y[row]))
            group.append(lbl)
            self.labels.append(lbl)
        display.root_group = group
        self.display = display

    def _set_brightness(self, level):
        try:
            self.display.brightness = level
        except (AttributeError, RuntimeError, ValueError):
            pass  # driver without brightness control: stays at full brightness until it sleeps

    def activity(self):
        """Call on any user input: resets the idle timer and wakes the screen."""
        self.last_activity = ticks_ms()
        if self.display is None or self.state == "on":
            return
        if self.state == "off":
            try:
                self.display.wake()
            except AttributeError:
                pass
        self._set_brightness(1.0)
        self.state = "on"

    def tick(self):
        """Dim or switch off the screen after the configured idle time."""
        if self.display is None:
            return
        idle_s = (ticks_ms() - self.last_activity) / 1000
        if self.state != "off" and SCREEN_OFF_AFTER_S and idle_s >= SCREEN_OFF_AFTER_S:
            try:
                self.display.sleep()
                self.state = "off"
            except AttributeError:
                pass
        elif self.state == "on" and SCREEN_DIM_AFTER_S and idle_s >= SCREEN_DIM_AFTER_S:
            self._set_brightness(SCREEN_DIM_LEVEL)
            self.state = "dim"

    def show(self, lines):
        if self.labels is None:
            return
        for row in range(self.ROWS):
            text = lines[row] if row < len(lines) else ""
            if text != self.shown[row]:  # only redraw rows that changed
                self.labels[row].text = text
                self.shown[row] = text


class MicLed:
    """Red/green LED pair on one pin: high = red, low = green, input = both off."""

    def __init__(self, pin):
        self.io = None
        self.mode = None
        if pin is None:
            return
        self.io = digitalio.DigitalInOut(pin)
        self.io.switch_to_input(pull=None)
        self.mode = "off"

    def update(self, online, mic_muted):
        if self.io is None:
            return
        if online and mic_muted is not None:
            want = "red" if mic_muted else "green"
        elif MIC_LED_OFFLINE_MODE == "blink":
            want = "red" if (ticks_ms() // 500) % 2 else "green"
        else:
            want = "off"
        if want == self.mode:
            return
        if want == "off":
            self.io.switch_to_input(pull=None)
        else:
            self.io.switch_to_output(value=(want == "red"))
        self.mode = want


def channel_row(name, volume, muted):
    """21 characters: 7 name, 8 bar, 4 value."""
    if volume is None:
        bar, value = "........", "  --"
    else:
        filled = (volume * 8 + 50) // 100
        bar = "#" * filled + "." * (8 - filled)
        value = "MUTE" if muted else "{:>3}%".format(volume)
    return "{:<7} {} {}".format(name[:7], bar, value)


# --------------------------------------------------------------------------
# State shared with the PC app
# --------------------------------------------------------------------------

state = {
    "names": ["App 1", "App 2", "App 3", "App 4"],
    "volumes": [None, None, None, None],
    "muted": [False, False, False, False],
    "mic": None,          # True = muted, False = live, None = unknown / no mic
    "last_rx": -HOST_TIMEOUT_MS,
    "quiet_until": None,  # while set, keys and the master knob only report to the PC (key finder)
}


def send(message):
    if data_port is None or not data_port.connected:
        return
    try:
        data_port.write((message + "\n").encode())
    except OSError:
        pass


def handle_host_line(line):
    state["last_rx"] = ticks_ms()
    if line == "?":
        send("HELLO MACROPAD " + FW_VERSION)
        return
    parts = line.split(" ", 4)
    try:
        if parts[0] == "V" and len(parts) >= 4:
            ch = int(parts[1])
            if 0 <= ch < len(APP_ENCODERS):
                state["volumes"][ch] = None if parts[2] == "-" else max(0, min(100, int(parts[2])))
                state["muted"][ch] = parts[3] == "1"
                if len(parts) == 5 and parts[4]:
                    state["names"][ch] = parts[4]
        elif parts[0] == "QUIET" and len(parts) == 2:
            # Settings page "rearrange keys": report presses without acting on them, for at most 60 s.
            state["quiet_until"] = ticks_ms() + QUIET_MAX_MS if parts[1] == "1" else None
        elif parts[0] == "MIC" and len(parts) == 2:
            mic = {"1": True, "0": False}.get(parts[1])
            if mic is not None and state["mic"] is not None and mic != state["mic"]:
                screen.activity()  # wake the screen so a mute change is always visible
            state["mic"] = mic
    except ValueError:
        pass  # ignore malformed lines


class LineReader:
    MAX = 256

    def __init__(self):
        self.buf = b""

    def poll(self):
        if data_port is None:
            return
        waiting = data_port.in_waiting
        if not waiting:
            return
        self.buf += data_port.read(waiting)
        while True:
            end = self.buf.find(b"\n")
            if end < 0:
                break
            raw, self.buf = self.buf[:end], self.buf[end + 1:]
            try:
                line = raw.decode("utf-8").strip()
            except UnicodeError:
                continue
            if line:
                handle_host_line(line)
        if len(self.buf) > self.MAX:  # garbage without newlines
            self.buf = b""


def host_online():
    return (data_port is not None and data_port.connected
            and ticks_ms() - state["last_rx"] < HOST_TIMEOUT_MS)


def screen_lines():
    if not host_online():
        return ["PC app offline", "Master knob + keys", "still work.", "", "Start macropad_host"]
    mic = state["mic"]
    header = ">>> MIC MUTED <<<" if mic is True else ("MIC live" if mic is False else "MIC: none found")
    rows = [header]
    for ch in SETTINGS["layout"]["knobs"]:   # app rows in the knobs' left-to-right order
        rows.append(channel_row(state["names"][ch], state["volumes"][ch], state["muted"][ch]))
    return rows


# --------------------------------------------------------------------------
# Input handling
# --------------------------------------------------------------------------

_layout = None


def type_text(text):
    global _layout
    if _layout is None:
        from adafruit_hid.keyboard_layout_us import KeyboardLayoutUS
        _layout = KeyboardLayoutUS(keyboard)
    _layout.write(text)


def run_action(action, pressed):
    kind, value = action
    if kind == "mic":
        if pressed:
            send("M")
    elif kind == "keys":
        if pressed:
            keyboard.press(*value)
        else:
            keyboard.release(*value)
    elif kind == "media":
        if pressed:
            media.press(value)
        else:
            media.release()
    elif kind == "text":
        if pressed:
            type_text(value)
    elif kind == "pc":
        if pressed:
            send("K " + value)


def quiet():
    until = state["quiet_until"]
    if until is not None and ticks_ms() >= until:
        state["quiet_until"] = until = None
    return until is not None


def on_button(number, pressed):
    if pressed:
        send("B {}".format(number))   # raw button number, lets the settings page find each key
    if quiet():
        return
    if number < len(KEYMAP):
        run_action(KEYMAP[number], pressed)
    elif FIRST_APP_PUSH <= number < MASTER_PUSH:
        if pressed:
            send("P {}".format(number - FIRST_APP_PUSH))
    elif number == MASTER_PUSH:
        run_action(MASTER_PUSH_ACTION, pressed)


app_last = [k.position for k in app_knobs]
master_last = master_knob.position


def poll_knobs():
    """Returns True if any knob moved."""
    global master_last
    moved = False
    for ch, knob in enumerate(app_knobs):
        pos = knob.position
        delta = pos - app_last[ch]
        if delta:
            app_last[ch] = pos
            moved = True
            send("E {} {}".format(ch, -delta if APP_REVERSED else delta))

    pos = master_knob.position
    delta = pos - master_last
    if delta:
        master_last = pos
        moved = True
        if quiet():
            return moved
        if MASTER_REVERSED:
            delta = -delta
        code = ConsumerControlCode.VOLUME_INCREMENT if delta > 0 else ConsumerControlCode.VOLUME_DECREMENT
        for _ in range(min(abs(delta), MASTER_MAX_STEPS)):
            media.send(code)
    return moved


# --------------------------------------------------------------------------
# Main loop
# --------------------------------------------------------------------------

screen = Screen()
mic_led = MicLed(MIC_LED_PIN)
reader = LineReader()
event = keypad.Event()
print("Pico Mixpad", FW_VERSION, "running")

while True:
    while buttons.events.get_into(event):
        on_button(event.key_number, event.pressed)
        screen.activity()
    if poll_knobs():
        screen.activity()
    reader.poll()
    screen.show(screen_lines())
    screen.tick()
    mic_led.update(host_online(), state["mic"])
    time.sleep(0.002)
