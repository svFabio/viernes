"""Timeout control and graceful degradation wrapper for brain agents."""

import asyncio
from collections.abc import AsyncIterator
import concurrent.futures
import logging

from viernes.brain.agent import Agent

logger = logging.getLogger(__name__)

DEFAULT_REASONING_TIMEOUT: float = 10.0
FALLBACK_DEGRADED_RESPONSE: str = (
    "Disculpa, tardé demasiado en pensar la respuesta. Por favor intenta de nuevo."
)


class ResilientAgent(Agent):
    """Wraps an Agent instance with strict timeout control and graceful degradation fallback."""

    def __init__(
        self,
        inner_agent: Agent,
        timeout: float = DEFAULT_REASONING_TIMEOUT,
        fallback_response: str = FALLBACK_DEGRADED_RESPONSE,
    ) -> None:
        self.inner_agent = inner_agent
        self.timeout = timeout
        self.fallback_response = fallback_response

    def ask(self, prompt: str) -> str:
        """Executes inner agent synchronously with timeout enforcement."""
        with concurrent.futures.ThreadPoolExecutor(max_workers=1) as executor:
            future = executor.submit(self.inner_agent.ask, prompt)
            try:
                return future.result(timeout=self.timeout)
            except concurrent.futures.TimeoutError:
                logger.warning(
                    "Reasoning timed out after %.2f seconds. Applying graceful degradation.",
                    self.timeout,
                )
                return self.fallback_response
            except Exception as exc:
                logger.error("Reasoning failed with exception: %s. Returning fallback.", exc)
                return self.fallback_response

    async def ask_stream(self, prompt: str) -> AsyncIterator[str]:
        """Streams tokens from inner agent with asynchronous timeout enforcement."""
        try:
            async with asyncio.timeout(self.timeout):
                async for chunk in self.inner_agent.ask_stream(prompt):
                    yield chunk
        except TimeoutError:
            logger.warning(
                "Streaming reasoning timed out after %.2f seconds. Emitting degraded response.",
                self.timeout,
            )
            yield self.fallback_response
        except Exception as exc:
            logger.error("Streaming reasoning failed with exception: %s", exc)
            yield self.fallback_response
