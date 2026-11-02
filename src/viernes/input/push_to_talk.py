import logging
import os
import shutil
import subprocess
import threading
from collections.abc import Callable
from enum import Enum
from typing import Any

logger = logging.getLogger(__name__)


class PushToTalkState(str, Enum):
    IDLE = "IDLE"
    LISTENING = "LISTENING"
    PROCESSING = "PROCESSING"
    CANCELLED = "CANCELLED"


def send_desktop_notification(title: str, message: str) -> None:
    """Sends a desktop notification using notify-send if available."""
    if "PYTEST_CURRENT_TEST" in os.environ:
        return

    notify_send_bin = shutil.which("notify-send")
    if notify_send_bin:
        try:
            subprocess.Popen(
                [
                    notify_send_bin,
                    "-a",
                    "Viernes",
                    "-i",
                    "audio-input-microphone",
                    "-t",
                    "2000",
                    "-h",
                    "string:x-canonical-private-synchronous:viernes",
                    title,
                    message,
                ],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
        except OSError as exc:
            logger.debug("Failed to spawn notify-send: %s", exc)


class PushToTalkController:
    """Manages Push-to-Talk activation, key press/release events, and cancellation."""

    def __init__(
        self,
        shortcut: str | None = None,
        on_start: Callable[[], Any] | None = None,
        on_finish: Callable[[], Any] | None = None,
        on_cancel: Callable[[], Any] | None = None,
        on_interrupt: Callable[[], Any] | None = None,
        notify_fn: Callable[[str, str], None] | None = None,
    ) -> None:
        self.shortcut = (shortcut or os.getenv("PUSH_TO_TALK_KEY") or "Super+V").strip()

        self.on_start = on_start
        self.on_finish = on_finish
        self.on_cancel = on_cancel
        self.on_interrupt = on_interrupt
        self.notify_fn = notify_fn or send_desktop_notification

        self.state: PushToTalkState = PushToTalkState.IDLE
        self._lock = threading.Lock()
        self._parsed_keys = self._parse_shortcut(self.shortcut)

    @staticmethod
    def _parse_shortcut(shortcut: str) -> set[str]:
        """Parses shortcut string (e.g. 'Super+V', 'Ctrl+Alt+Space') into normalized lower-case tokens."""
        parts = [p.strip().lower() for p in shortcut.split("+") if p.strip()]
        normalized = set()
        for p in parts:
            if p in ("super", "win", "windows", "mod4"):
                normalized.add("cmd")
            elif p in ("ctrl", "control"):
                normalized.add("ctrl")
            elif p in ("alt", "option"):
                normalized.add("alt")
            elif p in ("shift",):
                normalized.add("shift")
            else:
                normalized.add(p)
        return normalized

    def handle_key_down(self) -> None:
        """Handles key-down trigger (e.g. shortcut pressed or IPC 'press' command)."""
        with self._lock:
            if self.state == PushToTalkState.PROCESSING:
                logger.info("Interrupting previous processing via barge-in.")
                if self.on_interrupt is not None:
                    try:
                        self.on_interrupt()
                    except (RuntimeError, OSError, TypeError) as exc:
                        logger.warning("Error in on_interrupt callback: %s", exc)

            self.state = PushToTalkState.LISTENING
            logger.info("Push-to-Talk activated: State -> LISTENING")

            if self.notify_fn is not None:
                try:
                    self.notify_fn("Viernes", "Escuchando...")
                except (RuntimeError, OSError) as exc:
                    logger.debug("Error delivering listening notification: %s", exc)

            if self.on_start is not None:
                try:
                    self.on_start()
                except (RuntimeError, OSError, TypeError) as exc:
                    logger.error("Error executing on_start callback: %s", exc)

    def handle_key_up(self) -> None:
        """Handles key-up trigger (e.g. shortcut released or IPC 'release' command)."""
        with self._lock:
            if self.state != PushToTalkState.LISTENING:
                logger.debug("Ignored key-up event because state is %s", self.state)
                return

            self.state = PushToTalkState.PROCESSING
            logger.info("Push-to-Talk released: State -> PROCESSING")

            if self.on_finish is not None:
                try:
                    self.on_finish()
                except (RuntimeError, OSError, TypeError) as exc:
                    logger.error("Error executing on_finish callback: %s", exc)

    def handle_escape(self) -> None:
        """Handles Escape trigger: safely cancels current recording and discards audio."""
        with self._lock:
            if self.state != PushToTalkState.LISTENING:
                logger.debug("Escape pressed but not currently in LISTENING state.")
                return

            self.state = PushToTalkState.CANCELLED
            logger.info("Push-to-Talk cancelled by Escape key.")

            if self.notify_fn is not None:
                try:
                    self.notify_fn("Viernes", "Grabación cancelada")
                except (RuntimeError, OSError) as exc:
                    logger.debug("Error delivering cancel notification: %s", exc)

            if self.on_cancel is not None:
                try:
                    self.on_cancel()
                except (RuntimeError, OSError, TypeError) as exc:
                    logger.error("Error executing on_cancel callback: %s", exc)

            self.state = PushToTalkState.IDLE

    def handle_ipc_command(self, command: str) -> None:
        """Dispatches external IPC / CLI commands (press, release, cancel)."""
        cmd = command.strip().lower()
        if cmd in ("press", "start", "down"):
            self.handle_key_down()
        elif cmd in ("release", "finish", "up"):
            self.handle_key_up()
        elif cmd in ("cancel", "escape", "abort"):
            self.handle_escape()
        else:
            logger.warning("Unrecognized IPC command for Push-to-Talk: '%s'", command)
