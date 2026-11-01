"""MCP Tool registry and schema definition."""

from collections.abc import Callable
from dataclasses import dataclass
from typing import Any

from pydantic import BaseModel, Field

from viernes.tools.security import (
    DEFAULT_ALLOWLIST,
    SecurityGate,
    SecurityPolicy,
    SecurityViolationError,
)


class OpenAppParams(BaseModel):
    app_name: str = Field(..., min_length=1, max_length=50, pattern=r"^[a-zA-Z0-9_\-\.]+$")


class VolumeControlParams(BaseModel):
    level_percent: int = Field(..., ge=0, le=100)


class TickTickListParams(BaseModel):
    project_id: str | None = Field(default=None, max_length=50)


class TickTickCreateParams(BaseModel):
    title: str = Field(..., min_length=1, max_length=200)
    project_id: str | None = Field(default=None, max_length=50)


class TickTickDeleteParams(BaseModel):
    task_id: str = Field(..., min_length=1, max_length=50)


class SystemDeleteFileParams(BaseModel):
    target_path: str = Field(..., min_length=1, max_length=200)


@dataclass
class ToolDefinition:
    """Represents a validated, securable MCP tool."""

    name: str
    description: str
    param_schema: type[BaseModel]
    handler: Callable[[BaseModel], Any]
    requires_confirmation: bool = False
    impact_formatter: Callable[[BaseModel], str] | None = None


class ToolRegistry:
    """Central registry for discovering and executing MCP tools under strict security policy."""

    def __init__(
        self,
        policy: SecurityPolicy | None = None,
        gate: SecurityGate | None = None,
        allowlist: set[str] | frozenset[str] | None = None,
    ) -> None:
        if gate is not None:
            self.gate = gate
            self.policy = gate.policy
        else:
            self.policy = policy or SecurityPolicy(allowlist=allowlist)
            self.gate = SecurityGate(policy=self.policy)
        self._tools: dict[str, ToolDefinition] = {}

    def register(self, tool: ToolDefinition) -> None:
        """Registers a tool after checking that it conforms to the strict allowlist."""
        self.policy.validate_tool(tool.name)
        self._tools[tool.name] = tool

    def is_allowed(self, tool_name: str) -> bool:
        """Returns True if the tool name is permitted by security policy."""
        return self.policy.is_allowed(tool_name)

    def get(self, tool_name: str) -> ToolDefinition | None:
        """Retrieves a registered tool if it is permitted by policy."""
        if not self.policy.is_allowed(tool_name):
            return None
        return self._tools.get(tool_name)

    def get_tool(self, tool_name: str) -> ToolDefinition | None:
        """Alias for get() to retrieve a tool by name."""
        return self._tools.get(tool_name)

    def format_impact(self, name: str, params: BaseModel) -> str:
        """Formats a human-readable impact description for critical/destructive actions."""
        tool = self.get_tool(name)
        if tool and tool.impact_formatter:
            return tool.impact_formatter(params)
        return f"ejecutar la acción '{name}'"

    def execute(self, tool_name: str, raw_params: dict[str, Any] | None = None) -> Any:
        """Validates tool against allowlist, parses Pydantic parameters, and executes."""
        params_dict = raw_params or {}

        with self.gate.audit_execution(tool_name, params_dict):
            tool = self._tools.get(tool_name)
            if not tool:
                raise KeyError(
                    f"Tool '{tool_name}' is permitted by allowlist but has not been registered."
                )

            validated_params = tool.param_schema.model_validate(params_dict)
            return tool.handler(validated_params)

    def list_allowed_tools(self) -> list[str]:
        """Returns sorted list of currently allowlisted tool names."""
        return sorted(self.policy.allowlist)

    def list_registered_tools(self) -> list[str]:
        """Returns sorted list of actively registered tool names."""
        return sorted(self._tools.keys())


__all__ = [
    "DEFAULT_ALLOWLIST",
    "OpenAppParams",
    "SystemDeleteFileParams",
    "TickTickCreateParams",
    "TickTickDeleteParams",
    "TickTickListParams",
    "ToolDefinition",
    "ToolRegistry",
    "VolumeControlParams",
]
