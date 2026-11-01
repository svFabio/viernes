import logging
import shutil
import subprocess
from pathlib import Path

logger = logging.getLogger(__name__)


class AudioPlayer:
    """Dispatches audio playback to system audio daemons (mpv, pw-play, paplay, aplay)."""

    SUPPORTED_BACKENDS = ("mpv", "pw-play", "paplay", "aplay")

    def __init__(self, backend: str | None = None):
        self.player_bin = backend or self._resolve_player()

    def _resolve_player(self) -> str:
        for candidate in self.SUPPORTED_BACKENDS:
            if shutil.which(candidate):
                return candidate
        raise RuntimeError(
            "No supported audio player found in PATH (install mpv, pipewire, or pulseaudio, "
            "e.g. 'sudo pacman -S mpv' or 'sudo apt install mpv')."
        )

    def play(self, audio_path: Path | str, blocking: bool = True) -> subprocess.Popen:
        """Plays the specified audio file.

        When blocking=False, immediately returns the spawned Popen process.
        """
        path = Path(audio_path)
        if not path.is_file():
            raise FileNotFoundError(f"Audio file does not exist: {path}")

        cmd = [self.player_bin]
        if self.player_bin == "mpv":
            cmd.extend(["--no-terminal", "--no-video"])
        cmd.append(str(path))

        proc = subprocess.Popen(cmd)
        if blocking:
            proc.wait()
        return proc
