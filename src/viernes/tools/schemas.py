"""Strict Pydantic parameter schemas for MCP tools."""

from typing import Any, Literal
import re

from pydantic import BaseModel, ConfigDict, Field, ValidationError, field_validator


class ToolParameterValidationError(ValueError):
    """Raised when tool arguments fail strict Pydantic validation."""

    def __init__(self, message: str, errors: list[dict[str, Any]] | None = None) -> None:
        super().__init__(message)
        self.errors = errors or []


class StrictToolParams(BaseModel):
    """Base class for all tool parameter models, enforcing extra forbidding and frozen state."""

    model_config = ConfigDict(
        extra="forbid",
        frozen=True,
        str_strip_whitespace=True,
    )


class OpenAppParams(StrictToolParams):
    """Parameters for opening an application."""

    app_name: str = Field(
        ...,
        min_length=1,
        max_length=100,
        pattern=r"^[a-zA-Z0-9_\-\. ]+$",
        description="Name of the permitted application executable or bundle.",
    )

    @field_validator("app_name")
    @classmethod
    def validate_no_traversal(cls, v: str) -> str:
        clean = v.strip()
        if ".." in clean or "/" in clean or "\\" in clean:
            raise ValueError("Path traversal or separators are forbidden in app_name.")
        return clean


class VolumeControlParams(StrictToolParams):
    """Parameters for system volume adjustments."""

    level_percent: int = Field(
        ...,
        ge=0,
        le=100,
        description="Target volume level from 0 to 100.",
    )
    action: Literal["set", "mute", "unmute", "increase", "decrease"] = Field(
        default="set",
        description="Action type for volume control.",
    )
    step: int | None = Field(
        default=None,
        ge=1,
        le=50,
        description="Optional relative adjustment step.",
    )


class SystemTimeParams(StrictToolParams):
    """Parameters for querying system time."""

    timezone: str | None = Field(
        default=None,
        max_length=50,
        pattern=r"^[a-zA-Z0-9_\/\+\-]+$",
        description="Optional IANA timezone name.",
    )


class TickTickListParams(StrictToolParams):
    """Parameters for listing TickTick tasks."""

    project_id: str | None = Field(
        default=None,
        max_length=100,
        pattern=r"^[a-zA-Z0-9_\-]+$",
        description="Filter by TickTick project ID.",
    )
    limit: int = Field(
        default=20,
        ge=1,
        le=100,
        description="Maximum number of tasks to return.",
    )
    completed: bool = Field(
        default=False,
        description="Whether to include completed tasks.",
    )


class TickTickCreateParams(StrictToolParams):
    """Parameters for creating a TickTick task."""

    title: str = Field(
        ...,
        min_length=1,
        max_length=200,
        description="Title of the task to create.",
    )
    project_id: str | None = Field(
        default=None,
        max_length=100,
        pattern=r"^[a-zA-Z0-9_\-]+$",
        description="TickTick project ID.",
    )
    content: str | None = Field(
        default=None,
        max_length=1000,
        description="Additional description or note for the task.",
    )
    priority: int = Field(
        default=0,
        ge=0,
        le=5,
        description="Priority level: 0 (none) to 5 (highest).",
    )


class TickTickDeleteParams(StrictToolParams):
    """Parameters for deleting a TickTick task."""

    task_id: str = Field(
        ...,
        min_length=1,
        max_length=100,
        pattern=r"^[a-zA-Z0-9_\-]+$",
        description="ID of the task to delete.",
    )
    project_id: str | None = Field(
        default=None,
        max_length=100,
        pattern=r"^[a-zA-Z0-9_\-]+$",
        description="Optional project ID where the task resides.",
    )


class WeatherParams(StrictToolParams):
    """Parameters for fetching weather."""

    city: str = Field(
        ...,
        min_length=1,
        max_length=100,
        pattern=r"^[a-zA-Z0-9\s,\.\-]+$",
        description="City or location name.",
    )
    units: Literal["metric", "imperial"] = Field(
        default="metric",
        description="Measurement units system.",
    )


class FileDeleteParams(StrictToolParams):
    """Parameters for deleting a file (dangerous destructive action)."""

    file_path: str = Field(
        ...,
        min_length=1,
        max_length=500,
        description="Path of the target file to delete.",
    )
    force: bool = Field(
        default=False,
        description="Whether to force removal.",
    )

    @field_validator("file_path")
    @classmethod
    def validate_no_injection(cls, v: str) -> str:
        clean = v.strip()
        if re.search(r"[;&|`$<>\n\r]", clean):
            raise ValueError("Shell metacharacters are strictly forbidden in file_path.")
        return clean


def validate_tool_params(
    param_schema: type[BaseModel],
    raw_params: dict[str, Any] | None,
) -> BaseModel:
    """Validates raw parameters dictionary strictly against the given Pydantic schema.

    Raises:
        ToolParameterValidationError: If validation fails due to missing fields,
            extra unexpected fields, or constraint violations.
    """
    payload = raw_params if raw_params is not None else {}
    try:
        return param_schema.model_validate(payload)
    except ValidationError as exc:
        details = [
            {
                "loc": ".".join(str(item) for item in err["loc"]),
                "msg": err["msg"],
                "type": err["type"],
            }
            for err in exc.errors()
        ]
        err_msg = f"Parameter validation failed for {param_schema.__name__}: {details}"
        raise ToolParameterValidationError(err_msg, errors=details) from exc


__all__ = [
    "FileDeleteParams",
    "OpenAppParams",
    "StrictToolParams",
    "SystemTimeParams",
    "TickTickCreateParams",
    "TickTickDeleteParams",
    "TickTickListParams",
    "ToolParameterValidationError",
    "VolumeControlParams",
    "WeatherParams",
    "validate_tool_params",
]
