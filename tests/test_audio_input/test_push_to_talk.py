from unittest.mock import MagicMock, patch

from viernes.input.push_to_talk import (
    DEFAULT_NOTIFICATION_REPLACE_ID,
    PushToTalkController,
    PushToTalkState,
    get_active_notification_id,
    send_desktop_notification,
    set_active_notification_id,
)


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


def test_send_desktop_notification_silenced_in_test(monkeypatch):
    monkeypatch.setenv("PYTEST_CURRENT_TEST", "test_active")
    with patch("subprocess.Popen") as mock_popen:
        send_desktop_notification("Viernes", "Escuchando...")
        mock_popen.assert_not_called()


def test_send_desktop_notification_replaces_card(monkeypatch):
    monkeypatch.delenv("PYTEST_CURRENT_TEST", raising=False)
    with (
        patch("shutil.which", return_value="/usr/bin/notify-send"),
        patch("subprocess.Popen") as mock_popen,
    ):
        send_desktop_notification("Viernes", "Escuchando...")
        mock_popen.assert_called_once()
        args = mock_popen.call_args[0][0]
        assert "string:x-canonical-private-synchronous:viernes" in args
        assert "-t" in args
        assert "2000" in args
        assert "-r" in args
        assert str(DEFAULT_NOTIFICATION_REPLACE_ID) in args


def test_send_desktop_notification_consecutive_replaces_same_id(monkeypatch):
    monkeypatch.delenv("PYTEST_CURRENT_TEST", raising=False)
    with (
        patch("shutil.which", return_value="/usr/bin/notify-send"),
        patch("subprocess.Popen") as mock_popen,
    ):
        # Fire multiple notifications in sequence
        for msg in ["Escuchando...", "Procesando...", "Respondiendo..."]:
            send_desktop_notification("Viernes", msg)

        assert mock_popen.call_count == 3
        for call_item in mock_popen.call_args_list:
            args = call_item[0][0]
            assert "-r" in args
            r_idx = args.index("-r")
            assert args[r_idx + 1] == str(DEFAULT_NOTIFICATION_REPLACE_ID)


def test_send_desktop_notification_custom_replace_id(monkeypatch):
    monkeypatch.delenv("PYTEST_CURRENT_TEST", raising=False)
    with (
        patch("shutil.which", return_value="/usr/bin/notify-send"),
        patch("subprocess.Popen") as mock_popen,
    ):
        target = send_desktop_notification("Viernes", "Test", replace_id=7777)
        assert target == 7777
        args = mock_popen.call_args[0][0]
        assert "-r" in args
        r_idx = args.index("-r")
        assert args[r_idx + 1] == "7777"


def test_active_notification_id_getter_and_setter():
    orig = get_active_notification_id()
    try:
        set_active_notification_id(8888)
        assert get_active_notification_id() == 8888
    finally:
        set_active_notification_id(orig)


def test_push_to_talk_concurrency_rapid_events():
    import threading

    controller = PushToTalkController()
    errors = []

    def run_rapid_cycle():
        try:
            for _ in range(50):
                controller.handle_key_down()
                controller.handle_key_up()
                controller.handle_key_down()
                controller.handle_escape()
        except Exception as e:
            errors.append(e)

    threads = [threading.Thread(target=run_rapid_cycle) for _ in range(5)]
    for t in threads:
        t.start()
    for t in threads:
        t.join()

    assert not errors, f"Encountered concurrency errors: {errors}"
    assert controller.state in (
        PushToTalkState.IDLE,
        PushToTalkState.LISTENING,
        PushToTalkState.PROCESSING,
        PushToTalkState.CANCELLED,
    )
