"""Adversarial security injection and tool bypass test suite for MCP tools."""

from typing import Any
import pytest

from viernes.tools.registry import ToolDefinition, ToolRegistry
from viernes.tools.schemas import (
    OpenAppParams,
    TickTickCreateParams,
    ToolParameterValidationError,
    VolumeControlParams,
)
from viernes.tools.security import (
    SecurityGate,
    SecurityPolicy,
    SecurityViolationError,
    inspect_for_shell_injection,
)


@pytest.mark.parametrize(
    "payload",
    [
        "spotify; rm -rf /",
        "app && whoami",
        "app | nc -l 1337",
        "test`id`",
        "test$(cat /etc/passwd)",
        "test > /dev/null",
        "test < /dev/zero",
        "app\nrm -rf /",
        "app\r\nreboot",
    ],
)
def test_shell_metacharacters_rejected(payload: str) -> None:
    with pytest.raises(SecurityViolationError) as exc_info:
        inspect_for_shell_injection(payload)
    assert "dangerous shell metacharacters detected" in str(exc_info.value)


@pytest.mark.parametrize(
    "dangerous_token",
    [
        "bash",
        "sh",
        "zsh",
        "dash",
        "ksh",
        "csh",
        "eval",
        "exec",
        "sudo",
        "su",
        "chmod",
        "chown",
        "dd",
        "mkfs",
        "nc",
        "curl",
        "wget",
    ],
)
def test_dangerous_shell_command_tokens_rejected(dangerous_token: str) -> None:
    with pytest.raises(SecurityViolationError) as exc_info:
        inspect_for_shell_injection(f"launch {dangerous_token}")
    assert "dangerous command" in str(exc_info.value)


def test_nested_injection_in_dicts_and_lists() -> None:
    nested_payload: dict[str, Any] = {
        "outer": {
            "inner_list": [
                "safe_item",
                {"deep_key": "malicious; rm -rf /"},
            ]
        }
    }
    with pytest.raises(SecurityViolationError):
        inspect_for_shell_injection(nested_payload)


@pytest.mark.parametrize(
    "forbidden_tool",
    [
        "bash",
        "sh",
        "eval",
        "exec",
        "zsh",
        "csh",
        "powershell",
        "cmd",
        "system_exec",
        "shell_exec",
        "rm_rf",
        "BASH",
        "  eval  ",
    ],
)
def test_forbidden_tools_blocked_by_policy(forbidden_tool: str) -> None:
    policy = SecurityPolicy()
    assert not policy.is_allowed(forbidden_tool)
    with pytest.raises(SecurityViolationError):
        policy.validate_tool(forbidden_tool)


def test_unregistered_and_unallowlisted_tool_rejection() -> None:
    gate = SecurityGate()
    registry = ToolRegistry(gate=gate)

    with pytest.raises(SecurityViolationError) as exc_info:
        registry.execute("arbitrary_hacker_tool", {"arg": "value"})

    assert "not in the strict MCP allowlist" in str(exc_info.value)
    # Check that audit log recorded rejection
    records = gate.audit_logs
    assert len(records) == 1
    assert records[0].status == "REJECTED"
    assert records[0].tool_name == "arbitrary_hacker_tool"


def test_path_traversal_and_injection_blocked_during_execution() -> None:
    gate = SecurityGate()
    registry = ToolRegistry(gate=gate)
    executed = False

    def dummy_handler(p: Any) -> str:
        nonlocal executed
        executed = True
        return f"Ran {p}"

    tool = ToolDefinition(
        name="system_open_app",
        description="Opens application",
        param_schema=OpenAppParams,
        handler=dummy_handler,
    )
    registry.register(tool)

    # 1. Shell injection attempt in params
    with pytest.raises(SecurityViolationError):
        registry.execute("system_open_app", {"app_name": "spotify; rm -rf /"})
    assert not executed
    assert gate.audit_logs[-1].status == "REJECTED"

    # 2. Path traversal attempt in params
    with pytest.raises(ToolParameterValidationError):
        registry.execute("system_open_app", {"app_name": "../../../usr/bin/firefox"})
    assert not executed
    assert gate.audit_logs[-1].status == "ERROR"


def test_schema_tampering_and_parameter_overflow() -> None:
    gate = SecurityGate()
    registry = ToolRegistry(gate=gate)

    tool = ToolDefinition(
        name="system_volume_control",
        description="Controls volume",
        param_schema=VolumeControlParams,
        handler=lambda p: f"Volume set to {p.level_percent}",
    )
    registry.register(tool)

    # Overly large integer
    with pytest.raises(ToolParameterValidationError):
        registry.execute("system_volume_control", {"level_percent": 999999})

    # Injected extra unauthorized parameters
    with pytest.raises(ToolParameterValidationError):
        registry.execute(
            "system_volume_control",
            {"level_percent": 50, "__proto__": "tampered", "cmd": "reboot"},
        )

    # DoS attack with massive string length
    tick_tool = ToolDefinition(
        name="ticktick_create_task",
        description="Creates task",
        param_schema=TickTickCreateParams,
        handler=lambda p: "Created",
    )
    registry.register(tick_tool)

    with pytest.raises(ToolParameterValidationError):
        registry.execute(
            "ticktick_create_task",
            {"title": "A" * 500},  # Max length is 200
        )
