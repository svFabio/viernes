from abc import ABC, abstractmethod
from collections.abc import AsyncIterator


class Agent(ABC):
    """Abstract interface for the reasoning engine."""

    @abstractmethod
    def ask(self, prompt: str) -> str:
        """Processes user input and returns a textual response."""

    @abstractmethod
    def ask_stream(self, prompt: str) -> AsyncIterator[str]:
        """Processes user input and yields streamed text chunks."""


class EchoAgent(Agent):
    """Basic fallback agent for testing pipelines without cloud dependencies."""

    def ask(self, prompt: str) -> str:
        return f"Viernes received: {prompt}"

    async def ask_stream(self, prompt: str) -> AsyncIterator[str]:
        yield self.ask(prompt)
