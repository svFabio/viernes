import time
from pathlib import Path
from unittest.mock import MagicMock

from viernes.input.ipc import PushToTalkIPCServer, send_ptt_ipc_command
from viernes.input.push_to_talk import PushToTalkController, PushToTalkState


def test_ipc_server_lifecycle_and_command_dispatch(tmp_path: Path):
    socket_path = tmp_path / "test_ptt.sock"
    mock_start = MagicMock()
    mock_finish = MagicMock()
    mock_cancel = MagicMock()

    controller = PushToTalkController(
        on_start=mock_start,
        on_finish=mock_finish,
        on_cancel=mock_cancel,
        notify_fn=lambda t, m: None,
    )

    with PushToTalkIPCServer(controller=controller, socket_path=socket_path):
        assert socket_path.exists()

        # Send press command
        res_press = send_ptt_ipc_command("press", socket_path=socket_path)
        assert res_press is True
        time.sleep(0.1)
        assert controller.state == PushToTalkState.LISTENING
        mock_start.assert_called_once()

        # Send release command
        res_release = send_ptt_ipc_command("release", socket_path=socket_path)
        assert res_release is True
        time.sleep(0.1)
        assert controller.state == PushToTalkState.PROCESSING
        mock_finish.assert_called_once()

        # Reset and test cancel
        controller.handle_key_down()
        assert controller.state == PushToTalkState.LISTENING
        res_cancel = send_ptt_ipc_command("cancel", socket_path=socket_path)
        assert res_cancel is True
        time.sleep(0.1)
        assert controller.state == PushToTalkState.IDLE
        mock_cancel.assert_called_once()

    # After exit, socket file should be cleaned up
    assert not socket_path.exists()


def test_ipc_client_handles_non_existent_socket(tmp_path: Path):
    non_existent = tmp_path / "missing.sock"
    assert send_ptt_ipc_command("press", socket_path=non_existent) is False
