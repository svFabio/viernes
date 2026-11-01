import logging
import shutil
import signal
import subprocess
import sys
from pathlib import Path

logger = logging.getLogger(__name__)


class AudioPlayer:
    """Dispatches audio playback to system audio daemons (mpv, pw-play, paplay, aplay)."""

    SUPPORTED_BACKENDS = ("mpv", "pw-play", "paplay", "aplay")

    def __init__(self, backend: str | None = None):
        self.player_bin = backend or self._resolve_player()
        self._active_processes: list[subprocess.Popen] = []

    def _resolve_player(self) -> str:
        for candidate in self.SUPPORTED_BACKENDS:
            if shutil.which(candidate):
                return candidate
        raise RuntimeError(
            "No supported audio player found in PATH (install mpv, pipewire, or pulseaudio, "
            "e.g. 'sudo pacman -S mpv' or 'sudo apt install mpv')."
        )

    def _cleanup_finished_processes(self) -> None:
        """Prunes finished processes from active tracker to prevent zombie accumulation."""
        still_running = []
        for proc in self._active_processes:
            if proc.poll() is None:
                still_running.append(proc)
        self._active_processes = still_running

    def play(self, audio_path: Path | str, blocking: bool = True) -> subprocess.Popen:
        """Plays the specified audio file.

        When blocking=False, immediately returns the spawned Popen process.
        """
        path = Path(audio_path)
        if not path.is_file():
            raise FileNotFoundError(f"Audio file does not exist: {path}")

        self._cleanup_finished_processes()

        cmd = [self.player_bin]
        if self.player_bin == "mpv":
            cmd.extend(["--no-terminal", "--no-video"])

        cmd.append(str(path))
        logger.debug("Playing audio with command: %s", " ".join(cmd))

        proc = subprocess.Popen(
            cmd,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        self._active_processes.append(proc)

        if blocking:
            proc.wait()
            self._cleanup_finished_processes()

        return proc

    def stop_all(self) -> None:
        """Terminates all currently running playback processes."""
        for proc in self._active_processes:
            if proc.poll() is None:
                try:
                    proc.terminate()
                    proc.wait(timeout=1.0)
                except Exception as exc:
                    logger.warning("Error stopping audio player process %s: %s", getattr(proc, "pid", "unknown"), exc)
        self._active_processes.clear()


def register_signal_handlers(player: AudioPlayer | None = None) -> None:
    """Registers clean signal handlers for SIGINT and SIGTERM to stop active audio."""

    def _handler(signum, _frame):
        logger.info("Received termination signal %s. Cleaning up audio processes...", signum)
        if player:
            player.stop_all()
        sys.exit(0)

    try:
        signal.signal(signal.SIGINT, _handler)
        signal.signal(signal.SIGTERM, _handler)
    except (ValueError, AttributeError) as exc:
        logger.debug("Signal handlers could not be registered: %s", exc)
