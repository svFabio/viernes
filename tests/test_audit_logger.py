"""Tests for audit logging and secret redaction in MCP tool executions."""

from datetime import datetime

from viernes.tools.audit import AuditLogger, sanitize_params
from viernes.tools.registry import ToolDefinition, ToolRegistry
from viernes.tools.schemas import TickTickCreateParams
from viernes.tools.security import SecurityGate


def test_sanitize_params_simple_and_nested() -> None:
    raw = {
        "api_key": "sk-1234567890abcdef",
        "user_token": "secret_token_val",
        "password": "supersecretpassword",
        "safe_param": "hello world",
        "nested": {
            "oauth_credential": "top-secret-oauth",
            "normal_number": 42,
        },
        "items": [
            {"auth_bearer": "secret1"},
            {"normal_field": "ok"},
        ],
    }

    sanitized = sanitize_params(raw)

    assert sanitized["api_key"] == "[REDACTED]"
    assert sanitized["user_token"] == "[REDACTED]"
    assert sanitized["password"] == "[REDACTED]"
    assert sanitized["safe_param"] == "hello world"
    assert sanitized["nested"]["oauth_credential"] == "[REDACTED]"
    assert sanitized["nested"]["normal_number"] == 42
    assert sanitized["items"][0]["auth_bearer"] == "[REDACTED]"
    assert sanitized["items"][1]["normal_field"] == "ok"


def test_sanitize_bearer_token_in_string() -> None:
    raw = "Authorization header: Bearer abcdef1234567890.xyz"
    sanitized = sanitize_params(raw)
    assert sanitized == "Authorization header: Bearer [REDACTED]"


def test_audit_logger_records_and_filters() -> None:
    audit = AuditLogger()

    rec1 = audit.record(
        tool_name="ticktick_list_tasks",
        status="SUCCESS",
        duration_ms=12.456,
        params={"project_id": "work", "token": "sensitive_val"},
    )
    assert rec1.tool_name == "ticktick_list_tasks"
    assert rec1.status == "SUCCESS"
    assert rec1.params["project_id"] == "work"
    assert rec1.params["token"] == "[REDACTED]"
    # Verify ISO format timestamp
    datetime.fromisoformat(rec1.timestamp)

    rec2 = audit.record(
        tool_name="system_open_app",
        status="REJECTED",
        duration_ms=1.2,
        params={"app_name": "bash"},
        error="Security violation",
    )
    assert rec2.status == "REJECTED"
    assert rec2.error == "Security violation"

    records = audit.get_records()
    assert len(records) == 2

    # Filtering by tool
    tick_records = audit.filter_by_tool("ticktick_list_tasks")
    assert len(tick_records) == 1
    assert tick_records[0].tool_name == "ticktick_list_tasks"

    # Filtering by status
    rejected = audit.filter_by_status("REJECTED")
    assert len(rejected) == 1
    assert rejected[0].tool_name == "system_open_app"

    # Clear
    audit.clear()
    assert len(audit.get_records()) == 0


def test_security_gate_integration_with_audit_logger() -> None:
    audit_logger = AuditLogger()
    gate = SecurityGate(audit_logger=audit_logger)
    registry = ToolRegistry(gate=gate)

    tool = ToolDefinition(
        name="ticktick_create_task",
        description="Creates task in TickTick",
        param_schema=TickTickCreateParams,
        handler=lambda p: f"Task created: {p.title}",
    )
    registry.register(tool)

    result = registry.execute(
        "ticktick_create_task",
        {"title": "Buy groceries", "priority": 2},
    )
    assert result == "Task created: Buy groceries"

    records = audit_logger.get_records()
    assert len(records) == 1
    assert records[0].tool_name == "ticktick_create_task"
    assert records[0].status == "SUCCESS"
    assert records[0].duration_ms > 0
