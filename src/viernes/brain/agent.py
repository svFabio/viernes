import asyncio
from abc import ABC, abstractmethod
from collections.abc import AsyncIterator, Callable


class Agent(ABC):
    """Abstract interface for the reasoning engine."""

    on_tool_activity: Callable[[str], None] | None = None

    def rotate_conversation(self) -> str | None:
        """Rotates the conversation session if supported by the engine."""
        return None

    @abstractmethod
    def ask(self, prompt: str) -> str:
        """Processes user input synchronously and returns a textual response."""

    @abstractmethod
    def ask_stream(self, prompt: str) -> AsyncIterator[str]:
        """Processes user input and yields textual chunks asynchronously."""


class EchoAgent(Agent):
    """Basic fallback agent for testing pipelines without cloud dependencies."""

    def __init__(self, prefix: str = "Viernes received: ") -> None:
        self.prefix = prefix

    def ask(self, prompt: str) -> str:
        """Processes user input synchronously and returns a textual response."""
        return f"{self.prefix}{prompt}"

    async def ask_stream(self, prompt: str) -> AsyncIterator[str]:
        """Processes user input and yields textual chunks asynchronously."""
        yield self.prefix
        if prompt:
            yield prompt


class MockAgent(Agent):
    """Mock agent for testing with programmable responses, call history, and streaming."""

    def __init__(
        self,
        default_response: str = "Mock response",
        responses: list[str] | dict[str, str] | None = None,
        stream_chunks: list[str] | dict[str, list[str]] | None = None,
        delay: float = 0.0,
        exception: Exception | None = None,
    ) -> None:
        self.default_response = default_response
        self.responses = responses
        self.stream_chunks = stream_chunks
        self.delay = delay
        self.exception = exception
        self._call_count: int = 0
        self._history: list[str] = []

    @property
    def call_count(self) -> int:
        """Returns the total number of calls to ask or ask_stream."""
        return self._call_count

    @property
    def history(self) -> list[str]:
        """Returns a copy of the list of prompts received."""
        return list(self._history)

    def reset(self) -> None:
        """Resets call counter and recorded prompt history."""
        self._call_count = 0
        self._history.clear()

    def _resolve_response(self, prompt: str) -> str:
        if isinstance(self.responses, dict):
            return self.responses.get(prompt, self.default_response)
        if isinstance(self.responses, list) and self.responses:
            idx = (self._call_count - 1) % len(self.responses)
            return self.responses[idx]
        return self.default_response

    def ask(self, prompt: str) -> str:
        """Processes user input synchronously with mock behavior."""
        self._call_count += 1
        self._history.append(prompt)
        if self.exception is not None:
            raise self.exception
        return self._resolve_response(prompt)

    async def ask_stream(self, prompt: str) -> AsyncIterator[str]:
        """Processes user input and yields simulated chunks asynchronously."""
        self._call_count += 1
        self._history.append(prompt)
        if self.exception is not None:
            raise self.exception

        chunks: list[str]
        if isinstance(self.stream_chunks, dict):
            chunks = self.stream_chunks.get(prompt, [self._resolve_response(prompt)])
        elif isinstance(self.stream_chunks, list):
            chunks = self.stream_chunks
        else:
            chunks = [self._resolve_response(prompt)]

        for chunk in chunks:
            if self.delay > 0:
                await asyncio.sleep(self.delay)
            yield chunk


__all__ = ["Agent", "EchoAgent", "MockAgent"]
