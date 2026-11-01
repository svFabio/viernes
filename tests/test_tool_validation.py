"""Tests for strict Pydantic parameter schemas and parameter validation in MCP tools."""

import pytest

from viernes.tools.registry import ToolDefinition, ToolRegistry
from viernes.tools.schemas import (
    FileDeleteParams,
    OpenAppParams,
    SystemTimeParams,
    TickTickCreateParams,
    TickTickDeleteParams,
    TickTickListParams,
    ToolParameterValidationError,
    VolumeControlParams,
    WeatherParams,
    validate_tool_params,
)


def test_open_app_params_valid() -> None:
    params = OpenAppParams(app_name="spotify")
    assert params.app_name == "spotify"

    params_dotted = OpenAppParams(app_name="code.desktop")
    assert params_dotted.app_name == "code.desktop"


def test_open_app_params_extra_forbidden() -> None:
    with pytest.raises(ToolParameterValidationError) as exc_info:
        validate_tool_params(OpenAppParams, {"app_name": "spotify", "extra_arg": "unauthorized"})
    assert "extra_forbidden" in str(exc_info.value) or "extra" in str(exc_info.value)


def test_open_app_params_path_traversal_rejected() -> None:
    with pytest.raises(ToolParameterValidationError):
        validate_tool_params(OpenAppParams, {"app_name": "../bin/bash"})

    with pytest.raises(ToolParameterValidationError):
        validate_tool_params(OpenAppParams, {"app_name": "/usr/bin/firefox"})

    with pytest.raises(ToolParameterValidationError):
        validate_tool_params(OpenAppParams, {"app_name": "app\\nested"})


def test_volume_control_params_valid() -> None:
    params = VolumeControlParams(level_percent=50, action="set")
    assert params.level_percent == 50
    assert params.action == "set"

    params_mute = VolumeControlParams(level_percent=0, action="mute")
    assert params_mute.action == "mute"


def test_volume_control_params_bounds_and_invalid_action() -> None:
    with pytest.raises(ToolParameterValidationError):
        validate_tool_params(VolumeControlParams, {"level_percent": 150})

    with pytest.raises(ToolParameterValidationError):
        validate_tool_params(VolumeControlParams, {"level_percent": -5})

    with pytest.raises(ToolParameterValidationError):
        validate_tool_params(
            VolumeControlParams, {"level_percent": 50, "action": "explode"}
        )


def test_ticktick_create_params_valid_and_bounds() -> None:
    valid = validate_tool_params(
        TickTickCreateParams,
        {"title": "Buy groceries", "priority": 3, "content": "Milk and eggs"},
    )
    assert isinstance(valid, TickTickCreateParams)
    assert valid.title == "Buy groceries"
    assert valid.priority == 3

    # Empty title rejected
    with pytest.raises(ToolParameterValidationError):
        validate_tool_params(TickTickCreateParams, {"title": ""})

    # Priority out of bounds rejected
    with pytest.raises(ToolParameterValidationError):
        validate_tool_params(TickTickCreateParams, {"title": "Task", "priority": 10})

    # Extra keys rejected
    with pytest.raises(ToolParameterValidationError):
        validate_tool_params(
            TickTickCreateParams, {"title": "Task", "sql_injection": "DROP TABLE"}
        )


def test_ticktick_list_and_delete_params() -> None:
    list_params = validate_tool_params(
        TickTickListParams, {"limit": 10, "completed": True}
    )
    assert isinstance(list_params, TickTickListParams)
    assert list_params.limit == 10

    # Limit > 100 rejected
    with pytest.raises(ToolParameterValidationError):
        validate_tool_params(TickTickListParams, {"limit": 500})

    del_params = validate_tool_params(TickTickDeleteParams, {"task_id": "task_12345"})
    assert isinstance(del_params, TickTickDeleteParams)
    assert del_params.task_id == "task_12345"


def test_file_delete_params_shell_injection_rejected() -> None:
    valid = validate_tool_params(
        FileDeleteParams, {"file_path": "/tmp/test.txt", "force": False}
    )
    assert isinstance(valid, FileDeleteParams)

    with pytest.raises(ToolParameterValidationError):
        validate_tool_params(FileDeleteParams, {"file_path": "/tmp/test.txt; rm -rf /"})


def test_weather_and_system_time_params() -> None:
    weather = validate_tool_params(
        WeatherParams, {"city": "Buenos Aires", "units": "metric"}
    )
    assert isinstance(weather, WeatherParams)
    assert weather.city == "Buenos Aires"

    with pytest.raises(ToolParameterValidationError):
        validate_tool_params(
            WeatherParams, {"city": "Buenos Aires", "units": "kelvin"}
        )

    sys_time = validate_tool_params(
        SystemTimeParams, {"timezone": "America/Argentina/Buenos_Aires"}
    )
    assert isinstance(sys_time, SystemTimeParams)


def test_tool_registry_executes_with_strict_params_and_records_validation_error() -> None:
    registry = ToolRegistry()
    called_with = []

    tool = ToolDefinition(
        name="system_open_app",
        description="Opens an allowed application",
        param_schema=OpenAppParams,
        handler=lambda p: called_with.append(p.app_name) or f"Opened {p.app_name}",
    )
    registry.register(tool)

    # Valid execution
    res = registry.execute("system_open_app", {"app_name": "spotify"})
    assert res == "Opened spotify"
    assert called_with == ["spotify"]
    assert registry.gate.audit_logs[-1].status == "SUCCESS"

    # Invalid execution with extra forbidden parameter
    with pytest.raises(ToolParameterValidationError):
        registry.execute(
            "system_open_app",
            {"app_name": "spotify", "injected_param": "malicious"},
        )
    assert registry.gate.audit_logs[-1].status == "ERROR"
    assert "Parameter validation failed" in (registry.gate.audit_logs[-1].error or "")
