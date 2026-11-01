import logging

import pytest

from viernes.tools.error_handler import (
    AssistantState,
    ToolExceptionMiddleware,
    format_spoken_error,
)


def test_format_spoken_error_translations() -> None:
    timeout_err = TimeoutError("Connection to api.example.com timed out after 5000ms (HTTP 503)")
    spoken_timeout = format_spoken_error(timeout_err)
    assert "servicio no responde" in spoken_timeout
    assert "503" not in spoken_timeout

    perm_err = PermissionError("EACCES: permission denied /var/root")
    spoken_perm = format_spoken_error(perm_err)
    assert "autorización de seguridad" in spoken_perm
    assert "EACCES" not in spoken_perm

    not_found = FileNotFoundError("No such file or directory: '/tmp/test.csv'")
    spoken_nf = format_spoken_error(not_found)
    assert "archivo o recurso solicitado" in spoken_nf

    val_err = ValueError("Invalid input argument")
    spoken_val = format_spoken_error(val_err)
    assert "parámetros indicados" in spoken_val


def test_middleware_captures_exception_logs_trace_and_returns_idle(
    caplog: pytest.LogCaptureFixture,
) -> None:
    spoken_outputs: list[str] = []
    state_history: list[AssistantState] = []

    def fake_speaker(text: str) -> None:
        spoken_outputs.append(text)

    def track_state(new_state: AssistantState) -> None:
        state_history.append(new_state)

    middleware = ToolExceptionMiddleware(
        tts_speaker=fake_speaker,
        on_state_change=track_state,
    )

    def crashing_tool():
        raise RuntimeError("External DB connection refused: port 5432 down")

    with caplog.at_level(logging.ERROR):
        success, result, spoken_msg = middleware.execute_safely(
            crashing_tool, tool_name="system_weather"
        )

    # 1. System did not crash
    assert success is False
    assert result is None
    assert spoken_msg is not None

    # 2. Spoken output generated
    assert len(spoken_outputs) == 1
    assert "servicio no responde" in spoken_outputs[0] or "inconveniente" in spoken_outputs[0]

    # 3. State cleanly transitioned back to IDLE
    assert middleware.current_state == AssistantState.IDLE
    assert state_history == [AssistantState.SPEAKING, AssistantState.IDLE]

    # 4. Diagnostic stack trace logged for journalctl
    assert "External DB connection refused" in caplog.text
    assert "Traceback" in caplog.text


def test_middleware_success_path() -> None:
    middleware = ToolExceptionMiddleware()
    success, result, err = middleware.execute_safely(lambda: 42, tool_name="calculator")

    assert success is True
    assert result == 42
    assert err is None
    assert middleware.current_state == AssistantState.IDLE
