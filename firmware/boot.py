# boot.py - runs once at power-up, before code.py.
# Enables a second USB serial port ("data") that the PC app talks to.
# The first port ("console") stays available for the REPL and error messages.
import usb_cdc

usb_cdc.enable(console=True, data=True)
