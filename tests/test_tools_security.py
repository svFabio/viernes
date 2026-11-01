import pytest
from pydantic import BaseModel, Field

from viernes.tools.registry import ToolDefinition, ToolRegistry
from viernes.tools.security import (
    DEFAULT_ALLOWLIST,
    SecurityPolicy,
    SecurityViolationError,
)


class DummyOpenAppParams(BaseModel):
    app_name: str = Field(..., min_length=1)


class DummyVolumeParams(BaseModel):
    level: int = Field(..., ge=0, le=100)


def test_default_allowlist_contains_expected_tools():
    expected = {
        "system_open_app",
        "system_volume_control",
        "system_get_time",
        "system_delete_file",
        "ticktick_list_tasks",
        "ticktick_create_task",
        "ticktick_delete_task",
    }
    assert expected.issubset(DEFAULT_ALLOWLIST)


def test_security_policy_allows_permitted_tools():
    policy = SecurityPolicy()
    assert policy.is_allowed("system_open_app") is True
    assert policy.is_allowed("ticktick_list_tasks") is True


def test_security_policy_rejects_unauthorized_tools():
    policy = SecurityPolicy()
    assert policy.is_allowed("bash_exec") is False
    assert policy.is_allowed("rm_rf") is False
    assert policy.is_allowed("arbitrary_eval") is False

    with pytest.raises(SecurityViolationError, match="not in the strict MCP allowlist"):
        policy.validate_tool("bash_exec")


def test_tool_registry_rejects_registration_outside_allowlist():
    registry = ToolRegistry()

    dangerous_tool = ToolDefinition(
        name="dangerous_shell",
        description="Executes arbitrary commands",
        param_schema=DummyOpenAppParams,
        handler=lambda p: "executed",
    )

    with pytest.raises(SecurityViolationError, match="not in the strict MCP allowlist"):
        registry.register(dangerous_tool)


def test_tool_registry_registers_and_executes_allowed_tool():
    registry = ToolRegistry()

    tool = ToolDefinition(
        name="system_open_app",
        description="Abre una aplicación de escritorio",
        param_schema=DummyOpenAppParams,
        handler=lambda p: f"App {p.app_name} abierta correctamente",
    )
    registry.register(tool)

    assert registry.get("system_open_app") is tool
    assert "system_open_app" in registry.list_registered_tools()

    res = registry.execute("system_open_app", {"app_name": "spotify"})
    assert res == "App spotify abierta correctamente"


def test_tool_registry_execute_rejects_unregistered_or_invalid_params():
    registry = ToolRegistry()

    # Allowlisted tool but not yet registered in registry
    with pytest.raises(KeyError, match="permitted by allowlist but has not been registered"):
        registry.execute("system_volume_control", {"level": 50})

    # Register it
    registry.register(
        ToolDefinition(
            name="system_volume_control",
            description="Control de volumen",
            param_schema=DummyVolumeParams,
            handler=lambda p: f"Volumen al {p.level}%",
        )
    )

    # Invalid param (out of bounds)
    with pytest.raises(ValueError):
        registry.execute("system_volume_control", {"level": 200})
