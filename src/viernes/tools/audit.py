"""Audit logging for MCP tool execution with secret sanitization and query capabilities."""

from dataclasses import dataclass, field
from datetime import UTC, datetime
import logging
import re
from typing import Any

logger = logging.getLogger("viernes.tools.audit")

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

BEARER_TOKEN_REGEX = re.compile(r"Bearer\s+[A-Za-z0-9\-\._~\+\/]+=*", re.IGNORECASE)


def sanitize_params(data: Any) -> Any:
    """Recursively redacts sensitive values such as tokens or passwords."""
    if isinstance(data, dict):
        sanitized: dict[str, Any] = {}
        for key, value in data.items():
            key_lower = str(key).lower()
            if any(pattern in key_lower for pattern in SENSITIVE_PATTERNS):
                sanitized[key] = "[REDACTED]"
            else:
                sanitized[key] = sanitize_params(value)
        return sanitized
    if isinstance(data, list):
        return [sanitize_params(item) for item in data]
    if isinstance(data, tuple):
        return tuple(sanitize_params(item) for item in data)
    if isinstance(data, str):
        # Redact embedded bearer tokens if found in raw strings
        if BEARER_TOKEN_REGEX.search(data):
            return BEARER_TOKEN_REGEX.sub("Bearer [REDACTED]", data)
        return data
    return data


@dataclass
class AuditRecord:
    """Immutable audit record for tool invocation attempts."""

    timestamp: str
    tool_name: str
    status: str  # "SUCCESS", "REJECTED", "ERROR"
    duration_ms: float
    params: dict[str, Any] = field(default_factory=dict)
    error: str | None = None


class AuditLogger:
    """In-memory and structured audit logger for MCP tool executions."""

    def __init__(self, log: logging.Logger | None = None) -> None:
        self.logger = log or logger
        self._records: list[AuditRecord] = []

    def record(
        self,
        tool_name: str,
        status: str,
        duration_ms: float,
        params: dict[str, Any] | None = None,
        error: str | None = None,
    ) -> AuditRecord:
        """Sanitizes parameters and appends an audit record."""
        clean_params = sanitize_params(params or {})
        rec = AuditRecord(
            timestamp=datetime.now(UTC).isoformat(),
            tool_name=tool_name,
            status=status,
            duration_ms=round(duration_ms, 3),
            params=clean_params,
            error=error,
        )
        self._records.append(rec)

        if status == "SUCCESS":
            self.logger.info(
                "Tool execution audit: tool=%s status=%s duration=%.2fms params=%s",
                rec.tool_name,
                rec.status,
                rec.duration_ms,
                rec.params,
            )
        elif status == "REJECTED":
            self.logger.warning(
                "Tool execution blocked: tool=%s status=%s error=%s params=%s",
                rec.tool_name,
                rec.status,
                rec.error,
                rec.params,
            )
        else:
            self.logger.error(
                "Tool execution error: tool=%s status=%s error=%s params=%s",
                rec.tool_name,
                rec.status,
                rec.error,
                rec.params,
            )
        return rec

    def get_records(self) -> list[AuditRecord]:
        """Returns shallow copy of all audit records in chronological order."""
        return list(self._records)

    def filter_by_tool(self, tool_name: str) -> list[AuditRecord]:
        """Filters audit records by tool name."""
        return [r for r in self._records if r.tool_name == tool_name]

    def filter_by_status(self, status: str) -> list[AuditRecord]:
        """Filters audit records by execution status."""
        return [r for r in self._records if r.status == status]

    def clear(self) -> None:
        """Clears all audit records."""
        self._records.clear()


__all__ = [
    "AuditLogger",
    "AuditRecord",
    "sanitize_params",
]
