import asyncio
import time
from collections.abc import AsyncIterator

import pytest

from viernes.brain.agent import Agent
from viernes.brain.timeout import (
    FALLBACK_DEGRADED_RESPONSE,
    ResilientAgent,
)


class SlowAgent(Agent):
    """Simulated slow agent for timeout testing."""

    def __init__(self, delay_seconds: float) -> None:
        self.delay_seconds = delay_seconds

    def ask(self, prompt: str) -> str:
        time.sleep(self.delay_seconds)
        return f"Completed: {prompt}"

    async def ask_stream(self, prompt: str) -> AsyncIterator[str]:
        await asyncio.sleep(self.delay_seconds)
        yield f"Completed: {prompt}"


def test_fast_execution_succeeds_without_timeout():
    agent = ResilientAgent(inner_agent=SlowAgent(delay_seconds=0.01), timeout=0.5)
    result = agent.ask("Hola")
    assert result == "Completed: Hola"


def test_slow_sync_execution_triggers_graceful_degradation():
    agent = ResilientAgent(inner_agent=SlowAgent(delay_seconds=0.3), timeout=0.1)
    result = agent.ask("Consulta pesada")
    assert result == FALLBACK_DEGRADED_RESPONSE


@pytest.mark.anyio
async def test_slow_stream_execution_triggers_graceful_degradation():
    agent = ResilientAgent(inner_agent=SlowAgent(delay_seconds=0.3), timeout=0.1)
    chunks = [chunk async for chunk in agent.ask_stream("Consulta stream lenta")]
    assert "".join(chunks) == FALLBACK_DEGRADED_RESPONSE
