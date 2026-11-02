import asyncio

import pytest

from viernes.tools.confirmation import (
    ConfirmationDecision,
    ConfirmationManager,
)
from viernes.tools.dispatcher import ToolDispatcher
from viernes.tools.registry import (
    SystemDeleteFileParams,
    ToolDefinition,
    ToolRegistry,
)


def _setup_critical_dispatcher() -> tuple[ToolDispatcher, ToolRegistry, ConfirmationManager]:
    registry = ToolRegistry()
    confirmation_mgr = ConfirmationManager(timeout_seconds=0.2)

    def delete_handler(params: SystemDeleteFileParams) -> str:
        return f"Archivo {params.target_path} eliminado con éxito"

    tool = ToolDefinition(
        name="system_delete_file",
        description="Elimina un archivo",
        param_schema=SystemDeleteFileParams,
        handler=delete_handler,
        requires_confirmation=True,
        impact_formatter=lambda p: f"eliminar el archivo {p.target_path}",
    )
    registry.register(tool)
    dispatcher = ToolDispatcher(registry=registry, confirmation_manager=confirmation_mgr)
    return dispatcher, registry, confirmation_mgr


def test_critical_tool_demands_confirmation() -> None:
    dispatcher, _, _ = _setup_critical_dispatcher()
    prompt = dispatcher.dispatch("system_delete_file", {"target_path": "/tmp/notes.txt"})

    assert "¿Confirmas que deseas eliminar el archivo /tmp/notes.txt?" in prompt
    assert "Di 'Sí' para proceder o 'Cancelar'." in prompt


@pytest.mark.anyio
async def test_confirmation_branch_approved() -> None:
    dispatcher, _, confirmation_mgr = _setup_critical_dispatcher()
    dispatcher.dispatch("system_delete_file", {"target_path": "/tmp/notes.txt"})
    action = confirmation_mgr.pending_action
    assert action is not None

    async def fake_affirmative_voice():
        return "Sí, confirmo"

    decision, msg = await confirmation_mgr.wait_for_voice_confirmation(
        action, fake_affirmative_voice
    )
    assert decision == ConfirmationDecision.APPROVED
    assert "Confirmación recibida" in msg

    # Now dispatch with is_confirmed=True
    result = dispatcher.dispatch(
        "system_delete_file", {"target_path": "/tmp/notes.txt"}, is_confirmed=True
    )
    assert result == "Archivo /tmp/notes.txt eliminado con éxito"
    assert confirmation_mgr.pending_action is None


@pytest.mark.anyio
async def test_confirmation_branch_rejected() -> None:
    dispatcher, _, confirmation_mgr = _setup_critical_dispatcher()
    dispatcher.dispatch("system_delete_file", {"target_path": "/tmp/notes.txt"})
    action = confirmation_mgr.pending_action
    assert action is not None

    async def fake_negative_voice():
        return "No, cancela por favor"

    decision, msg = await confirmation_mgr.wait_for_voice_confirmation(action, fake_negative_voice)
    assert decision == ConfirmationDecision.REJECTED
    assert msg == "Acción cancelada."
    assert confirmation_mgr.pending_action is None


@pytest.mark.anyio
async def test_confirmation_branch_timeout() -> None:
    dispatcher, _, confirmation_mgr = _setup_critical_dispatcher()
    dispatcher.dispatch("system_delete_file", {"target_path": "/tmp/notes.txt"})
    action = confirmation_mgr.pending_action
    assert action is not None

    async def silent_voice():
        await asyncio.sleep(0.5)
        return ""

    decision, msg = await confirmation_mgr.wait_for_voice_confirmation(action, silent_voice)
    assert decision == ConfirmationDecision.TIMEOUT
    assert "Tiempo de espera agotado" in msg
    assert confirmation_mgr.pending_action is None


def test_no_cumulative_lingering_state() -> None:
    dispatcher, _, confirmation_mgr = _setup_critical_dispatcher()
    dispatcher.dispatch("system_delete_file", {"target_path": "/tmp/notes.txt"})
    # Abort manually or reset
    confirmation_mgr.pending_action = None

    # Next unrelated action does not execute the delete
    open_tool = ToolDefinition(
        name="system_open_app",
        description="Open app",
        param_schema=SystemDeleteFileParams,
        handler=lambda p: "App opened",
    )
    dispatcher.registry.register(open_tool)
    res = dispatcher.dispatch("system_open_app", {"target_path": "calculator"})
    assert res == "App opened"
