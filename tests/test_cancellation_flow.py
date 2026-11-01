"""Tests for automatic cancellation by timeout or negative voice response."""

import asyncio
import pytest

from viernes.tools.confirmation import (
    ConfirmationDecision,
    ConfirmationManager,
    ConfirmationQuestionBuilder,
)
from viernes.tools.schemas import FileDeleteParams


@pytest.mark.parametrize(
    "negative_input",
    ["no", "cancela", "cancelar", "espera", "pará", "para", "detén", "abortar"],
)
def test_negative_variants_recognized(negative_input: str) -> None:
    manager = ConfirmationManager()
    assert manager.is_negative(negative_input) is True
    assert manager.evaluate_response(negative_input) == ConfirmationDecision.REJECTED


def test_explicit_rejection_branch() -> None:
    async def run_test() -> None:
        manager = ConfirmationManager(timeout_seconds=2.0)
        action = ConfirmationQuestionBuilder.build_pending_action(
            tool_name="system_delete_file",
            params=FileDeleteParams(file_path="/home/user/document.pdf"),
        )

        async def reject_listener() -> str:
            await asyncio.sleep(0.01)
            return "No, pará, cancelá todo"

        decision, msg = await manager.wait_for_voice_confirmation(action, reject_listener)
        assert decision == ConfirmationDecision.REJECTED
        assert msg == "Acción cancelada."
        assert manager.pending_action is None

    asyncio.run(run_test())


def test_timeout_cancellation_branch() -> None:
    async def run_test() -> None:
        manager = ConfirmationManager(timeout_seconds=0.05)
        action = ConfirmationQuestionBuilder.build_pending_action(
            tool_name="system_delete_file",
            params=FileDeleteParams(file_path="/tmp/test.txt"),
        )

        async def silent_listener() -> str:
            await asyncio.sleep(0.2)
            return ""

        decision, msg = await manager.wait_for_voice_confirmation(action, silent_listener)
        assert decision == ConfirmationDecision.TIMEOUT
        assert "Tiempo de espera agotado" in msg
        assert manager.pending_action is None

    asyncio.run(run_test())


def test_manual_abort_action() -> None:
    manager = ConfirmationManager()
    manager.pending_action = ConfirmationQuestionBuilder.build_pending_action(
        tool_name="system_delete_file",
        params=FileDeleteParams(file_path="/tmp/test.txt"),
    )
    assert manager.pending_action is not None

    msg = manager.abort_action()
    assert msg == "Acción cancelada."
    assert manager.pending_action is None


def test_no_cumulative_lingering_state() -> None:
    manager = ConfirmationManager()
    action = ConfirmationQuestionBuilder.build_pending_action(
        tool_name="system_delete_file",
        params=FileDeleteParams(file_path="/tmp/sensitive.db"),
    )
    manager.pending_action = action

    # Abort
    manager.abort_action()
    assert manager.pending_action is None

    # Subsequent check shows no leftover action
    assert manager.pending_action is None
