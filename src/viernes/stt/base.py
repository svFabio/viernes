from abc import ABC, abstractmethod
from pathlib import Path


class STTProvider(ABC):
    """Abstract interface for Speech-to-Text (STT) providers."""

    @abstractmethod
    def transcribe(self, audio_path: Path) -> str:
        """Transcribes the given audio file into normalized text.

        Args:
            audio_path: Path to the input audio file (e.g. WAV).

        Returns:
            str: Normalized transcribed text, or empty string if unintelligible or silent.

        Raises:
            FileNotFoundError: If the specified audio file does not exist on disk.
        """
