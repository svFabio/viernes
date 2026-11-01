from abc import ABC, abstractmethod
from pathlib import Path


class TTSProvider(ABC):
    """Abstract interface for text-to-speech providers."""

    @abstractmethod
    def synthesize(self, text: str, output_path: Path) -> Path:
        """Synthesizes text into an audio file and returns the written path."""
        pass
