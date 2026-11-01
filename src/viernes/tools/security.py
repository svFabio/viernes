"""Security schema, strict allowlist enforcement, and audit gate for MCP tools."""

from collections.abc import Generator
from contextlib import contextmanager
from datetime import UTC, datetime
import fnmatch
import logging
import re
import time
from typing import Any

from pydantic import BaseModel, Field


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

SENSITIVE_PATTERNS: tuple[str, ...] = (
    "token",
    "secret",
    "password",
    "passwd",
    "api_key",
    "apikey",
    "auth",
    "authorization",
    "credential",
    "credentials",
    "private_key",
    "bearer",
)


def sanitize_params(data: Any) -> Any:
    """Recursively redacts sensitive values such as tokens or passwords."""
    if isinstance(data, dict):
        sanitized: dict[str, Any] = {}
        for key, value in data.items():
            key_lower = str(key).lower()
            if any(pattern in key_lower for pattern in SENSITIVE_PATTERNS):
                if isinstance(value, dict):
                    sanitized[key] = sanitize_params(value)
                elif isinstance(value, list):
                    sanitized[key] = [sanitize_params(item) for item in value]
                else:
                    sanitized[key] = "[REDACTED]"
            else:
                sanitized[key] = sanitize_params(value)
        return sanitized
    if isinstance(data, list):
        return [sanitize_params(item) for item in data]
    return data


FORBIDDEN_COMMANDS: frozenset[str] = frozenset(
    {
        "bash",
        "sh",
        "zsh",
        "dash",
        "ksh",
        "eval",
        "exec",
        "rm",
        "sudo",
        "su",
        "chmod",
        "chown",
        "dd",
        "mkfs",
    }
)

SHELL_INJECTION_PATTERN: re.Pattern[str] = re.compile(r"[;&|`$<>\n\r]")


def inspect_for_shell_injection(value: Any) -> None:
    """Recursively inspects parameter values to block shell metacharacters and forbidden binaries."""
    if isinstance(value, str):
        if SHELL_INJECTION_PATTERN.search(value):
            raise SecurityViolationError(
                f"Dangerous shell characters detected in input argument: '{value}'"
            )
        tokens = value.lower().split()
        for token in tokens:
            if token in FORBIDDEN_COMMANDS:
                raise SecurityViolationError(
                    f"Forbidden shell command '{token}' detected in argument."
                )
    elif isinstance(value, dict):
        for k, v in value.items():
            inspect_for_shell_injection(k)
            inspect_for_shell_injection(v)
    elif isinstance(value, (list, tuple, set)):
        for item in value:
            inspect_for_shell_injection(item)


class AuditLogEntry(BaseModel):
    """Auditing record for a tool execution attempt."""

    timestamp: str
    tool_name: str
    status: str
    duration_ms: float | None = None
    params: dict[str, Any] = Field(default_factory=dict)
    error: str | None = None


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

    is_tool_allowed = is_allowed

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
        logger: logging.Logger | None = None,
    ) -> None:
        self.policy = policy or SecurityPolicy()
        self.logger = logger or logging.getLogger("viernes.tools.security")
        self._audit_logs: list[AuditLogEntry] = []

    @property
    def audit_logs(self) -> list[AuditLogEntry]:
        """Returns in-memory audit logs."""
        return list(self._audit_logs)

    def clear_audit_logs(self) -> None:
        """Clears in-memory audit logs."""
        self._audit_logs.clear()

    def _record_audit(self, entry: AuditLogEntry) -> None:
        self._audit_logs.append(entry)
        if entry.status == "SUCCESS":
            self.logger.info(
                "Tool execution audit: tool=%s status=%s duration=%.2fms params=%s",
                entry.tool_name,
                entry.status,
                entry.duration_ms or 0.0,
                entry.params,
            )
        elif entry.status == "REJECTED":
            self.logger.warning(
                "Tool execution blocked: tool=%s status=%s error=%s params=%s",
                entry.tool_name,
                entry.status,
                entry.error,
                entry.params,
            )
        else:
            self.logger.error(
                "Tool execution error: tool=%s status=%s error=%s params=%s",
                entry.tool_name,
                entry.status,
                entry.error,
                entry.params,
            )

    @contextmanager
    def audit_execution(
        self,
        tool_name: str,
        params: dict[str, Any] | None = None,
    ) -> Generator[None]:
        """Context manager to audit and protect tool execution."""
        start = time.perf_counter()
        sanitized = sanitize_params(params or {})

        try:
            self.policy.validate_tool(tool_name)
            if params:
                inspect_for_shell_injection(sanitized)
        except SecurityViolationError as err:
            duration = (time.perf_counter() - start) * 1000
            entry = AuditLogEntry(
                timestamp=datetime.now(UTC).isoformat(),
                tool_name=tool_name,
                status="REJECTED",
                duration_ms=round(duration, 3),
                params=sanitized,
                error=str(err),
            )
            self._record_audit(entry)
            raise

        try:
            yield
            duration = (time.perf_counter() - start) * 1000
            entry = AuditLogEntry(
                timestamp=datetime.now(UTC).isoformat(),
                tool_name=tool_name,
                status="SUCCESS",
                duration_ms=round(duration, 3),
                params=sanitized,
            )
            self._record_audit(entry)
        except Exception as exc:
            duration = (time.perf_counter() - start) * 1000
            entry = AuditLogEntry(
                timestamp=datetime.now(UTC).isoformat(),
                tool_name=tool_name,
                status="ERROR",
                duration_ms=round(duration, 3),
                params=sanitized,
                error=str(exc),
            )
            self._record_audit(entry)
            raise


__all__ = [
    "DEFAULT_ALLOWLIST",
    "FORBIDDEN_COMMANDS",
    "FORBIDDEN_TOOLS",
    "SHELL_INJECTION_PATTERN",
    "AuditLogEntry",
    "SecurityGate",
    "SecurityPolicy",
    "SecurityViolationError",
    "inspect_for_shell_injection",
    "sanitize_params",
]
