"""Tests for voice confirmation listening window and affirmative recognition."""

import asyncio
import pytest

from viernes.tools.confirmation import (
    DEFAULT_CONFIRMATION_TIMEOUT_SECONDS,
    ConfirmationDecision,
    ConfirmationManager,
    ConfirmationQuestionBuilder,
)
from viernes.tools.schemas import FileDeleteParams


def test_default_timeout_is_eight_seconds() -> None:
    manager = ConfirmationManager()
    assert manager.timeout_seconds == DEFAULT_CONFIRMATION_TIMEOUT_SECONDS
    assert manager.timeout_seconds == 8.0


@pytest.mark.parametrize(
    "affirmative_input",
    ["sí", "si", "confirmo", "adelante", "procede", "dale", "claro", "seguro"],
)
def test_affirmative_variants_recognized(affirmative_input: str) -> None:
    manager = ConfirmationManager()
    assert manager.is_affirmative(affirmative_input) is True
    assert manager.evaluate_response(affirmative_input) == ConfirmationDecision.APPROVED


def test_successful_confirmation_within_window() -> None:
    async def run_test() -> None:
        manager = ConfirmationManager(timeout_seconds=2.0)
        action = ConfirmationQuestionBuilder.build_pending_action(
            tool_name="system_delete_file",
            params=FileDeleteParams(file_path="/tmp/test.txt"),
        )

        async def mock_listener() -> str:
            await asyncio.sleep(0.01)
            return "sí, confirmo"

        decision, msg = await manager.wait_for_voice_confirmation(action, mock_listener)
        assert decision == ConfirmationDecision.APPROVED
        assert "Confirmación recibida" in msg
        assert manager.pending_action is None

    asyncio.run(run_test())


def test_window_timeout_expired() -> None:
    async def run_test() -> None:
        # Use a tiny timeout for fast test execution
        manager = ConfirmationManager(timeout_seconds=0.05)
        action = ConfirmationQuestionBuilder.build_pending_action(
            tool_name="system_delete_file",
            params=FileDeleteParams(file_path="/tmp/test.txt"),
        )

        async def slow_listener() -> str:
            await asyncio.sleep(0.2)
            return "sí"

        decision, msg = await manager.wait_for_voice_confirmation(action, slow_listener)
        assert decision == ConfirmationDecision.TIMEOUT
        assert "Tiempo de espera agotado" in msg
        assert manager.pending_action is None

    asyncio.run(run_test())
