from collections.abc import Callable
from dataclasses import dataclass
from typing import Any

from pydantic import BaseModel, Field


class OpenAppParams(BaseModel):
    app_name: str = Field(..., min_length=1, max_length=50, pattern=r"^[a-zA-Z0-9_\-\.]+$")


class VolumeControlParams(BaseModel):
    level_percent: int = Field(..., ge=0, le=100)


class TickTickListParams(BaseModel):
    project_id: str | None = Field(default=None, max_length=50)


class TickTickCreateParams(BaseModel):
    title: str = Field(..., min_length=1, max_length=200)
    project_id: str | None = Field(default=None, max_length=50)


@dataclass
class ToolDefinition:
    name: str
    description: str
    param_schema: type[BaseModel]
    handler: Callable[[BaseModel], Any]
    requires_confirmation: bool = False


# Default strict allowlist of tools permitted in Viernes
DEFAULT_ALLOWLIST = {
    "system_open_app",
    "system_volume_control",
    "ticktick_list_tasks",
    "ticktick_create_task",
    "system_get_time",
}


class ToolRegistry:
    """Manages allowed tools and provides schema validation lookups."""

    def __init__(self, allowlist: set[str] | None = None) -> None:
        self.allowlist = set(allowlist) if allowlist is not None else set(DEFAULT_ALLOWLIST)
        self._tools: dict[str, ToolDefinition] = {}

    def register(self, tool: ToolDefinition) -> None:
        if tool.name not in self.allowlist:
            raise PermissionError(
                f"Tool '{tool.name}' is not in the strict security allowlist."
            )
        self._tools[tool.name] = tool

    def get_tool(self, name: str) -> ToolDefinition | None:
        if name not in self.allowlist:
            return None
        return self._tools.get(name)

    def is_allowed(self, name: str) -> bool:
        return name in self.allowlist and name in self._tools
