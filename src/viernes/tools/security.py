"""Security schema and strict allowlist enforcement for MCP tools."""


class SecurityViolationError(PermissionError):
    """Raised when an unauthorized MCP tool invocation is attempted."""


DEFAULT_ALLOWLIST: frozenset[str] = frozenset(
    {
        "system_open_app",
        "system_volume_control",
        "system_get_time",
        "system_delete_file",
        "ticktick_list_tasks",
        "ticktick_create_task",
        "ticktick_delete_task",
    }
)


class SecurityPolicy:
    """Enforces strict allowlist rules for tool execution."""

    def __init__(self, allowlist: set[str] | frozenset[str] | None = None) -> None:
        self.allowlist: frozenset[str] = (
            frozenset(allowlist) if allowlist is not None else DEFAULT_ALLOWLIST
        )

    def is_allowed(self, tool_name: str) -> bool:
        """Returns True if the tool is explicitly permitted in the allowlist."""
        return tool_name in self.allowlist

    def validate_tool(self, tool_name: str) -> None:
        """Validates tool against the allowlist, raising SecurityViolationError if forbidden."""
        if not self.is_allowed(tool_name):
            raise SecurityViolationError(
                f"Security violation: tool '{tool_name}' is not in the strict MCP allowlist."
            )
