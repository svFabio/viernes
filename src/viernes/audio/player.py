import logging
import shutil
import subprocess
from pathlib import Path

logger = logging.getLogger(__name__)


class AudioPlayer:
    """Dispatches audio playback to system audio daemons (mpv, pw-play, paplay, aplay)."""

    def __init__(self):
        self.player_bin = self._resolve_player()

    def _resolve_player(self) -> str:
        for candidate in ("mpv", "pw-play", "paplay", "aplay"):
            if shutil.which(candidate):
                return candidate
        raise RuntimeError("No supported audio player found (install mpv, pipewire, or pulseaudio).")

    def play(self, audio_path: Path, blocking: bool = True) -> subprocess.Popen:
        """Plays the specified audio file."""
        if not audio_path.exists():
            raise FileNotFoundError(f"Audio file does not exist: {audio_path}")

        cmd = [self.player_bin]
        if self.player_bin == "mpv":
            cmd.extend(["--no-terminal", "--no-video"])

        cmd.append(str(audio_path))
        logger.debug("Playing audio with command: %s", " ".join(cmd))

        proc = subprocess.Popen(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        if blocking:
            proc.wait()
        return proc
