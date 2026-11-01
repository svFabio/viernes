"""Security schema, strict allowlist enforcement, and audit gate for MCP tools."""

from collections.abc import Generator
from contextlib import contextmanager
import fnmatch
import logging
import re
import time
from typing import Any

from viernes.tools.audit import AuditLogger, AuditRecord, sanitize_params

# Alias for backwards compatibility
AuditLogEntry = AuditRecord


class SecurityViolationError(PermissionError):
    """Raised when an unauthorized MCP tool invocation is attempted."""


DEFAULT_ALLOWLIST: frozenset[str] = frozenset(
    {
        "system_open_app",
        "system_volume_control",
        "system_get_time",
        "system_delete_file",
        "system_time",
        "system_volume",
        "get_weather",
        "ticktick_*",
        "ticktick_list_tasks",
        "ticktick_create_task",
        "ticktick_delete_task",
    }
)

FORBIDDEN_TOOLS: frozenset[str] = frozenset(
    {
        "bash",
        "sh",
        "eval",
        "exec",
        "zsh",
        "csh",
        "ksh",
        "cmd",
        "powershell",
        "system_exec",
        "shell_exec",
        "rm_rf",
    }
)

SHELL_INJECTION_REGEX = re.compile(r"[;&|`$<>\n\r]")
FORBIDDEN_COMMAND_TOKENS: frozenset[str] = frozenset(
    {
        "bash",
        "sh",
        "zsh",
        "dash",
        "ksh",
        "csh",
        "eval",
        "exec",
        "rm",
        "sudo",
        "su",
        "chmod",
        "chown",
        "dd",
        "mkfs",
        "nc",
        "curl",
        "wget",
    }
)


def inspect_for_shell_injection(data: Any) -> None:
    """Recursively checks arguments for shell metacharacters and dangerous shell commands."""
    if isinstance(data, str):
        if SHELL_INJECTION_REGEX.search(data):
            raise SecurityViolationError(
                f"Security violation: dangerous shell metacharacters detected in parameter: '{data}'"
            )
        tokens = data.lower().split()
        for token in tokens:
            if token in FORBIDDEN_COMMAND_TOKENS:
                raise SecurityViolationError(
                    f"Security violation: dangerous command '{token}' detected in parameter."
                )
    elif isinstance(data, dict):
        for k, v in data.items():
            inspect_for_shell_injection(k)
            inspect_for_shell_injection(v)
    elif isinstance(data, (list, tuple, set, frozenset)):
        for item in data:
            inspect_for_shell_injection(item)


class SecurityPolicy:
    """Enforces strict allowlist and forbidden rules for tool execution."""

    def __init__(
        self,
        allowlist: set[str] | frozenset[str] | None = None,
        forbidden_tools: set[str] | frozenset[str] | None = None,
    ) -> None:
        self.allowlist: frozenset[str] = (
            frozenset(allowlist) if allowlist is not None else DEFAULT_ALLOWLIST
        )
        self.forbidden_tools: frozenset[str] = (
            frozenset(forbidden_tools)
            if forbidden_tools is not None
            else FORBIDDEN_TOOLS
        )

    def is_forbidden(self, tool_name: str) -> bool:
        """Returns True if the tool name matches explicitly forbidden shell/eval tools."""
        tool_clean = tool_name.strip().lower()
        return any(
            tool_clean == forbidden or fnmatch.fnmatch(tool_clean, forbidden)
            for forbidden in self.forbidden_tools
        )

    def is_allowed(self, tool_name: str) -> bool:
        """Returns True if the tool is explicitly permitted and not forbidden."""
        if self.is_forbidden(tool_name):
            return False

        tool_clean = tool_name.strip()
        return any(
            tool_clean == allowed or fnmatch.fnmatch(tool_clean, allowed)
            for allowed in self.allowlist
        )

    def validate_tool(self, tool_name: str) -> None:
        """Validates tool against allowlist, raising SecurityViolationError if forbidden."""
        if self.is_forbidden(tool_name):
            raise SecurityViolationError(
                f"Security violation: tool '{tool_name}' is not in the strict MCP allowlist (forbidden command)."
            )
        if not self.is_allowed(tool_name):
            raise SecurityViolationError(
                f"Security violation: tool '{tool_name}' is not in the strict MCP allowlist."
            )


class SecurityGate:
    """Security gate that enforces policy validation and redacting audit logging."""

    def __init__(
        self,
        policy: SecurityPolicy | None = None,
        audit_logger: AuditLogger | None = None,
        logger: logging.Logger | None = None,
    ) -> None:
        self.policy = policy or SecurityPolicy()
        self.audit_logger = audit_logger or AuditLogger(log=logger)

    @property
    def audit_logs(self) -> list[AuditRecord]:
        """Returns in-memory audit logs."""
        return self.audit_logger.get_records()

    def clear_audit_logs(self) -> None:
        """Clears in-memory audit logs."""
        self.audit_logger.clear()

    @contextmanager
    def audit_execution(
        self,
        tool_name: str,
        params: dict[str, Any] | None = None,
    ) -> Generator[None]:
        """Context manager to audit and protect tool execution."""
        start = time.perf_counter()

        try:
            self.policy.validate_tool(tool_name)
            inspect_for_shell_injection(params or {})
        except SecurityViolationError as err:
            duration = (time.perf_counter() - start) * 1000
            self.audit_logger.record(
                tool_name=tool_name,
                status="REJECTED",
                duration_ms=duration,
                params=params,
                error=str(err),
            )
            raise

        try:
            yield
            duration = (time.perf_counter() - start) * 1000
            self.audit_logger.record(
                tool_name=tool_name,
                status="SUCCESS",
                duration_ms=duration,
                params=params,
            )
        except Exception as exc:
            duration = (time.perf_counter() - start) * 1000
            self.audit_logger.record(
                tool_name=tool_name,
                status="ERROR",
                duration_ms=duration,
                params=params,
                error=str(exc),
            )
            raise


__all__ = [
    "DEFAULT_ALLOWLIST",
    "FORBIDDEN_TOOLS",
    "AuditLogEntry",
    "AuditRecord",
    "SecurityGate",
    "SecurityPolicy",
    "SecurityViolationError",
    "inspect_for_shell_injection",
    "sanitize_params",
]
