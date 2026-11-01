"""Tests for critical tool classification and requires_confirmation flag enforcement."""

from viernes.tools.registry import DEFAULT_CRITICAL_TOOLS, ToolDefinition, ToolRegistry
from viernes.tools.schemas import (
    FileDeleteParams,
    OpenAppParams,
    TickTickDeleteParams,
    VolumeControlParams,
)


def test_explicit_requires_confirmation_flag() -> None:
    tool = ToolDefinition(
        name="custom_critical_tool",
        description="A dangerous custom action",
        param_schema=VolumeControlParams,
        handler=lambda p: "executed",
        requires_confirmation=True,
    )
    assert tool.requires_confirmation is True


def test_default_critical_tools_auto_enforced_in_registry() -> None:
    registry = ToolRegistry()

    # Even if registered with requires_confirmation=False, critical tools are forced True
    del_tool = ToolDefinition(
        name="system_delete_file",
        description="Deletes a file",
        param_schema=FileDeleteParams,
        handler=lambda p: "deleted",
        requires_confirmation=False,
    )
    registry.register(del_tool)

    assert del_tool.requires_confirmation is True
    assert registry.requires_confirmation("system_delete_file") is True


def test_non_critical_tools_have_requires_confirmation_false() -> None:
    registry = ToolRegistry()

    open_tool = ToolDefinition(
        name="system_open_app",
        description="Opens app",
        param_schema=OpenAppParams,
        handler=lambda p: "opened",
        requires_confirmation=False,
    )
    registry.register(open_tool)

    assert open_tool.requires_confirmation is False
    assert registry.requires_confirmation("system_open_app") is False


def test_list_critical_tools() -> None:
    registry = ToolRegistry()

    registry.register(
        ToolDefinition(
            name="system_open_app",
            description="Open app",
            param_schema=OpenAppParams,
            handler=lambda p: "ok",
        )
    )
    registry.register(
        ToolDefinition(
            name="system_delete_file",
            description="Delete file",
            param_schema=FileDeleteParams,
            handler=lambda p: "ok",
        )
    )
    registry.register(
        ToolDefinition(
            name="ticktick_delete_task",
            description="Delete task",
            param_schema=TickTickDeleteParams,
            handler=lambda p: "ok",
        )
    )

    critical = registry.list_critical_tools()
    assert critical == ["system_delete_file", "ticktick_delete_task"]


def test_unregistered_critical_check() -> None:
    registry = ToolRegistry()
    assert "system_delete_file" in DEFAULT_CRITICAL_TOOLS
    # Even if unregistered, checking whether the tool name is considered critical returns True
    assert registry.requires_confirmation("system_delete_file") is True
    # Unknown safe tool returns False
    assert registry.requires_confirmation("system_get_time") is False
