"""MCP Tool registry and schema definition."""

from collections.abc import Callable
from dataclasses import dataclass
from typing import Any

from pydantic import BaseModel

from viernes.tools.security import SecurityPolicy, SecurityViolationError


@dataclass
class ToolDefinition:
    """Represents a validated, securable MCP tool."""

    name: str
    description: str
    param_schema: type[BaseModel]
    handler: Callable[[BaseModel], Any]
    requires_confirmation: bool = False


class ToolRegistry:
    """Central registry for discovering and executing MCP tools under strict security policy."""

    def __init__(self, policy: SecurityPolicy | None = None) -> None:
        self.policy = policy or SecurityPolicy()
        self._tools: dict[str, ToolDefinition] = {}

    def register(self, tool: ToolDefinition) -> None:
        """Registers a tool after checking that it conforms to the strict allowlist."""
        self.policy.validate_tool(tool.name)
        self._tools[tool.name] = tool

    def get(self, tool_name: str) -> ToolDefinition | None:
        """Retrieves a registered tool if it is permitted by policy."""
        if not self.policy.is_allowed(tool_name):
            return None
        return self._tools.get(tool_name)

    def execute(self, tool_name: str, raw_params: dict[str, Any] | None = None) -> Any:
        """Validates tool against allowlist, parses Pydantic parameters, and executes."""
        self.policy.validate_tool(tool_name)

        tool = self._tools.get(tool_name)
        if not tool:
            raise KeyError(f"Tool '{tool_name}' is permitted by allowlist but has not been registered.")

        params_dict = raw_params or {}
        validated_params = tool.param_schema.model_validate(params_dict)
        return tool.handler(validated_params)

    def list_allowed_tools(self) -> list[str]:
        """Returns sorted list of currently allowlisted tool names."""
        return sorted(self.policy.allowlist)

    def list_registered_tools(self) -> list[str]:
        """Returns sorted list of actively registered tool names."""
        return sorted(self._tools.keys())
