"""Finds the macropad's CIRCUITPY drive and reads/writes settings.json on it."""

import json
import os
import string
import sys
from pathlib import Path
from typing import Optional

import hid_names
import mixpad_settings

LABEL = "CIRCUITPY"


def _windows_drives():
    import ctypes
    kernel32 = ctypes.windll.kernel32
    old_mode = kernel32.SetErrorMode(1)   # SEM_FAILCRITICALERRORS: no "insert a disk" pop-ups
    try:
        mask = kernel32.GetLogicalDrives()
        for i, letter in enumerate(string.ascii_uppercase):
            if not mask & (1 << i):
                continue
            root = letter + ":\\"
            if kernel32.GetDriveTypeW(root) != 2:   # DRIVE_REMOVABLE
                continue
            name = ctypes.create_unicode_buffer(64)
            if kernel32.GetVolumeInformationW(root, name, 64, None, None, None, None, 0):
                if name.value.upper() == LABEL:
                    yield Path(root)
    finally:
        kernel32.SetErrorMode(old_mode)


def _posix_drives():
    user = os.environ.get("USER", "")
    for base in ("/Volumes", f"/media/{user}", f"/run/media/{user}", "/media"):
        p = Path(base) / LABEL
        if p.is_dir():
            yield p


def find_drive(configured: str = "auto") -> Optional[Path]:
    """The CIRCUITPY drive, or None. `configured` may name a path instead of "auto"."""
    if configured and configured.lower() != "auto":
        p = Path(configured)
        return p if p.is_dir() else None
    drives = _windows_drives() if sys.platform == "win32" else _posix_drives()
    for drive in drives:
        if (drive / "code.py").exists() or (drive / "boot_out.txt").exists():
            return drive
    return None


def read_settings(drive: Optional[Path]) -> dict:
    """The pad's settings (defaults filled in), whether or not the drive is present."""
    path = None if drive is None else drive / "settings.json"
    if path is None or not path.exists():
        return mixpad_settings.normalise(None)
    return mixpad_settings.load(str(path), hid_names.KEYCODE, hid_names.MEDIA_CODE)


def clean_settings(raw: dict) -> dict:
    return mixpad_settings.normalise(raw, hid_names.KEYCODE, hid_names.MEDIA_CODE)


def write_settings(drive: Path, settings: dict) -> None:
    """Writes settings.json. CircuitPython restarts code.py when it sees the change."""
    data = json.dumps(clean_settings(settings), indent=2) + "\n"
    path = drive / "settings.json"
    with open(path, "w", encoding="ascii", newline="\n") as f:
        f.write(data)
        f.flush()
        os.fsync(f.fileno())
