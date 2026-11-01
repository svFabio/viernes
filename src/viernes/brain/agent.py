from abc import ABC, abstractmethod


class Agent(ABC):
    """Abstract interface for the reasoning engine."""

    @abstractmethod
    def ask(self, prompt: str) -> str:
        """Processes user input and returns a textual response."""
        pass


class EchoAgent(Agent):
    """Basic fallback agent for testing pipelines without cloud dependencies."""

    def ask(self, prompt: str) -> str:
        return f"Viernes received: {prompt}"
