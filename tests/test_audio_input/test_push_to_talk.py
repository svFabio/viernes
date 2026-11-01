from unittest.mock import MagicMock, patch

from viernes.input.push_to_talk import PushToTalkController, PushToTalkState


def test_push_to_talk_default_and_env_shortcut(monkeypatch):
    c1 = PushToTalkController()
    assert c1.shortcut == "Super+V"
    assert "cmd" in c1._parsed_keys
    assert "v" in c1._parsed_keys

    monkeypatch.setenv("PUSH_TO_TALK_KEY", "Ctrl+Alt+Space")
    c2 = PushToTalkController()
    assert c2.shortcut == "Ctrl+Alt+Space"
    assert "ctrl" in c2._parsed_keys
    assert "alt" in c2._parsed_keys
    assert "space" in c2._parsed_keys


def test_push_to_talk_key_down_activation():
    mock_start = MagicMock()
    mock_notify = MagicMock()

    controller = PushToTalkController(on_start=mock_start, notify_fn=mock_notify)
    assert controller.state == PushToTalkState.IDLE

    controller.handle_key_down()

    assert controller.state == PushToTalkState.LISTENING
    mock_start.assert_called_once()
    mock_notify.assert_called_once_with("Viernes", "Escuchando...")


def test_push_to_talk_key_up_completion():
    mock_finish = MagicMock()

    controller = PushToTalkController(on_finish=mock_finish)
    controller.handle_key_down()
    assert controller.state == PushToTalkState.LISTENING

    controller.handle_key_up()

    assert controller.state == PushToTalkState.PROCESSING
    mock_finish.assert_called_once()


def test_push_to_talk_key_up_ignored_if_not_listening():
    mock_finish = MagicMock()

    controller = PushToTalkController(on_finish=mock_finish)
    assert controller.state == PushToTalkState.IDLE

    controller.handle_key_up()

    assert controller.state == PushToTalkState.IDLE
    mock_finish.assert_not_called()


def test_push_to_talk_escape_cancellation():
    mock_cancel = MagicMock()
    mock_notify = MagicMock()

    controller = PushToTalkController(on_cancel=mock_cancel, notify_fn=mock_notify)
    controller.handle_key_down()
    assert controller.state == PushToTalkState.LISTENING

    controller.handle_escape()

    assert controller.state == PushToTalkState.IDLE
    mock_cancel.assert_called_once()
    mock_notify.assert_called_with("Viernes", "Grabación cancelada")


def test_push_to_talk_escape_ignored_when_idle():
    mock_cancel = MagicMock()

    controller = PushToTalkController(on_cancel=mock_cancel)
    controller.handle_escape()

    assert controller.state == PushToTalkState.IDLE
    mock_cancel.assert_not_called()


def test_push_to_talk_barge_in_concurrency():
    mock_interrupt = MagicMock()

    controller = PushToTalkController(on_interrupt=mock_interrupt)
    controller.handle_key_down()
    controller.handle_key_up()
    assert controller.state == PushToTalkState.PROCESSING

    # User presses key again while assistant was processing
    controller.handle_key_down()

    mock_interrupt.assert_called_once()
    assert controller.state == PushToTalkState.LISTENING


def test_push_to_talk_ipc_commands():
    controller = PushToTalkController()

    with patch.object(controller, "handle_key_down") as mock_down, \
         patch.object(controller, "handle_key_up") as mock_up, \
         patch.object(controller, "handle_escape") as mock_esc:

        controller.handle_ipc_command("press")
        mock_down.assert_called_once()

        controller.handle_ipc_command("release")
        mock_up.assert_called_once()

        controller.handle_ipc_command("cancel")
        mock_esc.assert_called_once()
