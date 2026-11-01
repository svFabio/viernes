import asyncio
import time
from collections.abc import AsyncIterator

from viernes.brain.agent import Agent


class MockAgent(Agent):
    """Predictable mock agent for automated testing without cloud tokens or external runtimes."""

    def __init__(
        self,
        default_response: str = "Respuesta de prueba de Viernes.",
        responses: list[str] | None = None,
        delay_seconds: float = 0.0,
    ) -> None:
        self.default_response = default_response
        self._responses = list(responses) if responses else []
        self.delay_seconds = delay_seconds
        self.received_prompts: list[str] = []

    def ask(self, prompt: str) -> str:
        self.received_prompts.append(prompt)
        if self.delay_seconds > 0:
            time.sleep(self.delay_seconds)

        if self._responses:
            return self._responses.pop(0)
        return self.default_response

    async def ask_stream(self, prompt: str) -> AsyncIterator[str]:
        self.received_prompts.append(prompt)
        if self.delay_seconds > 0:
            await asyncio.sleep(self.delay_seconds)

        response = self._responses.pop(0) if self._responses else self.default_response
        for token in response.split(" "):
            yield f"{token} "
