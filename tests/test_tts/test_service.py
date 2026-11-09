import signal
from pathlib import Path
from unittest.mock import MagicMock, patch

from viernes.audio.player import AudioPlayer, register_signal_handlers


def test_service_unit_file_structure():
    repo_root = Path(__file__).resolve().parent.parent.parent
    service_file = repo_root / "viernes.service"
    assert service_file.exists(), "viernes.service must exist in repo root"

    content = service_file.read_text()
    assert "[Unit]" in content
    assert "[Service]" in content
    assert "[Install]" in content

    # Acceptance criteria validations
    assert "Type=simple" in content
    assert "Restart=on-failure" in content
    assert ("RestartSec=5s" in content or "RestartSec=3s" in content)
    assert "ExecStart=" in content
    assert ("uv run" in content or "viernes" in content)
    assert "PassEnvironment=" in content
    assert "DISPLAY" in content
    assert "WAYLAND_DISPLAY" in content
    assert "XDG_RUNTIME_DIR" in content
    assert "PULSE_SERVER" in content
    assert "WantedBy=default.target" in content


def test_install_script_valid():
    repo_root = Path(__file__).resolve().parent.parent.parent
    script_file = repo_root / "scripts" / "install_service.sh"
    assert script_file.exists(), "scripts/install_service.sh must exist"

    content = script_file.read_text()
    assert "systemctl --user daemon-reload" in content
    assert "viernes.service" in content


def test_register_signal_handlers_invokes_stop_all():
    mock_player = MagicMock(spec=AudioPlayer)

    handlers = {}

    def mock_signal(sig, handler):
        handlers[sig] = handler

    with (
        patch("signal.signal", side_effect=mock_signal),
        patch("sys.exit") as mock_exit,
    ):
        register_signal_handlers(mock_player)

        assert signal.SIGINT in handlers
        assert signal.SIGTERM in handlers

        # Trigger SIGTERM handler
        handlers[signal.SIGTERM](signal.SIGTERM, None)
        mock_player.stop_all.assert_called_once()
        mock_exit.assert_called_once_with(0)
