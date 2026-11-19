"""Windows audio control through the Core Audio API (via pycaw).

Only public Windows APIs are used: per-app session volume (ISimpleAudioVolume)
and the default microphone's endpoint mute (IAudioEndpointVolume).
Nothing is injected into or hooked onto other processes.
"""

import ctypes
import time
from ctypes import POINTER, cast
from typing import Iterable, List, Optional, Tuple

import psutil
from comtypes import CLSCTX_ALL
from pycaw.pycaw import AudioUtilities, IAudioEndpointVolume

SESSION_CACHE_S = 1.0  # re-list audio sessions at most once a second


def _foreground_exe() -> Optional[str]:
    user32 = ctypes.windll.user32
    hwnd = user32.GetForegroundWindow()
    if not hwnd:
        return None
    pid = ctypes.c_ulong()
    user32.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
    try:
        return psutil.Process(pid.value).name().lower()
    except psutil.Error:
        return None


def _endpoint_volume(device):
    """Works with both newer pycaw (AudioDevice.EndpointVolume) and raw IMMDevice objects."""
    volume = getattr(device, "EndpointVolume", None)
    if volume is not None:
        return volume
    interface = device.Activate(IAudioEndpointVolume._iid_, CLSCTX_ALL, None)
    return cast(interface, POINTER(IAudioEndpointVolume))


class WindowsAudio:
    """Targets in a channel's app list:
        "discord.exe"  a process name (case-insensitive)
        "@focused"     whatever app is in the foreground window
        "@system"      Windows system sounds
    """

    def __init__(self) -> None:
        self._sessions = []
        self._sessions_at = 0.0

    # ---- sessions -------------------------------------------------------

    def _all_sessions(self):
        now = time.monotonic()
        if now - self._sessions_at > SESSION_CACHE_S:
            self._sessions = AudioUtilities.GetAllSessions()
            self._sessions_at = now
        return self._sessions

    def _matching(self, targets: Iterable[str]) -> List:
        targets = [t.lower() for t in targets]
        names = {t for t in targets if not t.startswith("@")}
        if "@focused" in targets:
            fg = _foreground_exe()
            if fg:
                names.add(fg)
        want_system = "@system" in targets

        found = []
        for session in self._all_sessions():
            try:
                if session.ProcessId == 0:
                    if want_system:
                        found.append(session)
                    continue
                process = session.Process
                if process is not None and process.name().lower() in names:
                    found.append(session)
            except (psutil.Error, OSError):
                continue  # process exited between listing and reading
        return found

    def session_names(self) -> List[str]:
        """Process names of the apps that currently have an audio session (for the settings page)."""
        names = set()
        for session in self._all_sessions():
            try:
                if session.ProcessId and session.Process is not None:
                    names.add(session.Process.name().lower())
            except (psutil.Error, OSError):
                continue
        return sorted(names)

    def nudge(self, targets: Iterable[str], percent: float) -> None:
        for session in self._matching(targets):
            vol = session.SimpleAudioVolume
            level = min(1.0, max(0.0, vol.GetMasterVolume() + percent / 100.0))
            vol.SetMasterVolume(level, None)

    def toggle_mute(self, targets: Iterable[str]) -> None:
        sessions = self._matching(targets)
        if not sessions:
            return
        new_state = 0 if sessions[0].SimpleAudioVolume.GetMute() else 1
        for session in sessions:
            session.SimpleAudioVolume.SetMute(new_state, None)

    def level(self, targets: Iterable[str]) -> Tuple[Optional[float], bool]:
        """(volume 0-100, muted) of the first matching session, or (None, False) if none is running."""
        sessions = self._matching(targets)
        if not sessions:
            return None, False
        vol = sessions[0].SimpleAudioVolume
        return vol.GetMasterVolume() * 100.0, bool(vol.GetMute())

    # ---- microphone -----------------------------------------------------

    def _mic(self):
        device = AudioUtilities.GetMicrophone()
        return None if device is None else _endpoint_volume(device)

    def mic_muted(self) -> Optional[bool]:
        mic = self._mic()
        return None if mic is None else bool(mic.GetMute())

    def toggle_mic(self) -> Optional[bool]:
        mic = self._mic()
        if mic is None:
            return None
        muted = not bool(mic.GetMute())
        mic.SetMute(int(muted), None)
        return muted
