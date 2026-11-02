import logging
import os
import socket
import threading
from pathlib import Path
from typing import Self

from viernes.input.push_to_talk import PushToTalkController

logger = logging.getLogger(__name__)


def get_default_socket_path() -> Path:
    return Path(os.getenv("VIERNES_PTT_SOCKET", "/tmp/viernes_ptt.sock"))


def send_ptt_ipc_command(command: str, socket_path: Path | str | None = None) -> bool:
    """Sends a Push-to-Talk IPC command ('press', 'release', 'cancel') to the daemon socket."""
    sock_p = Path(socket_path or get_default_socket_path())
    if not sock_p.exists():
        logger.debug("Push-to-Talk IPC socket does not exist: %s", sock_p)
        return False

    try:
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
            client.settimeout(1.0)
            client.connect(str(sock_p))
            client.sendall(f"{command.strip()}\n".encode())
            return True
    except OSError as exc:
        logger.warning("Failed to send command to Push-to-Talk IPC socket: %s", exc)
        return False


class PushToTalkIPCServer:
    """Unix Domain Socket server for Push-to-Talk IPC triggers (Hyprland / Wayland)."""

    def __init__(
        self,
        controller: PushToTalkController,
        socket_path: Path | str | None = None,
    ) -> None:
        self.controller = controller
        self.socket_path = Path(socket_path or get_default_socket_path())
        self._running = False
        self._server_sock: socket.socket | None = None
        self._thread: threading.Thread | None = None

    def start(self) -> None:
        """Starts the Unix Domain Socket listener in a background thread."""
        if self._running:
            return

        if self.socket_path.exists():
            try:
                self.socket_path.unlink()
            except OSError as exc:
                logger.warning("Could not remove stale socket %s: %s", self.socket_path, exc)

        self._server_sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self._server_sock.bind(str(self.socket_path))
        self._server_sock.listen(5)
        self._server_sock.settimeout(0.5)

        self._running = True
        self._thread = threading.Thread(
            target=self._listen_loop, daemon=True, name="viernes-ptt-ipc"
        )
        self._thread.start()
        logger.info("Push-to-Talk IPC server listening on %s", self.socket_path)

    def _listen_loop(self) -> None:
        while self._running:
            if self._server_sock is None:
                break
            try:
                conn, _ = self._server_sock.accept()
            except TimeoutError:
                continue
            except OSError:
                break

            with conn:
                try:
                    conn.settimeout(1.0)
                    data = conn.recv(128).decode("utf-8").strip()
                    if data:
                        logger.debug("Received Push-to-Talk IPC command: '%s'", data)
                        self.controller.handle_ipc_command(data)
                except OSError as exc:
                    logger.debug("Error reading from Push-to-Talk IPC client: %s", exc)

    def stop(self) -> None:
        """Stops the socket server and cleans up the socket file."""
        self._running = False
        if self._server_sock:
            try:
                self._server_sock.close()
            except OSError:
                pass
            self._server_sock = None

        if self._thread and self._thread.is_alive():
            self._thread.join(timeout=1.0)
            self._thread = None

        if self.socket_path.exists():
            try:
                self.socket_path.unlink()
            except OSError:
                pass
        logger.info("Push-to-Talk IPC server stopped.")

    def __enter__(self) -> Self:
        self.start()
        return self

    def __exit__(self, exc_type, exc_val, exc_tb) -> None:
        self.stop()
