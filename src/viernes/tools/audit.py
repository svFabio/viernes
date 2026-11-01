import logging
from dataclasses import dataclass
from datetime import UTC, datetime
from typing import Any

logger = logging.getLogger("viernes.tools.audit")

SENSITIVE_KEYS = {"key", "token", "password", "secret", "auth", "credential", "api_key"}


@dataclass
class AuditRecord:
    timestamp: datetime
    tool_name: str
    status: str  # "SUCCESS", "REJECTED", "ERROR"
    duration_ms: float
    params: dict[str, Any]
    error_message: str | None = None


def sanitize_params(data: dict[str, Any]) -> dict[str, Any]:
    """Recursively redacts values for keys resembling secrets or tokens."""
    sanitized: dict[str, Any] = {}
    for k, v in data.items():
        if any(secret_term in k.lower() for secret_term in SENSITIVE_KEYS):
            sanitized[k] = "[REDACTED]"
        elif isinstance(v, dict):
            sanitized[k] = sanitize_params(v)
        elif isinstance(v, list):
            sanitized[k] = [
                sanitize_params(elem) if isinstance(elem, dict) else elem for elem in v
            ]
        else:
            sanitized[k] = v
    return sanitized


class AuditLogger:
    """Stores and logs tool invocations with sanitized parameter values."""

    def __init__(self) -> None:
        self.records: list[AuditRecord] = []

    def log(
        self,
        tool_name: str,
        status: str,
        duration_ms: float,
        params: dict[str, Any],
        error_message: str | None = None,
    ) -> AuditRecord:
        clean_params = sanitize_params(params)
        record = AuditRecord(
            timestamp=datetime.now(UTC),
            tool_name=tool_name,
            status=status,
            duration_ms=duration_ms,
            params=clean_params,
            error_message=error_message,
        )
        self.records.append(record)
        logger.info(
            "Tool audit: tool=%s status=%s duration=%.2fms error=%s",
            tool_name,
            status,
            duration_ms,
            error_message,
        )
        return record
