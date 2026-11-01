import logging
from pathlib import Path

from viernes.stt.base import STTProvider

logger = logging.getLogger(__name__)


class FakeSTTProvider(STTProvider):
    """Fake STT Provider for unit and integration testing without downloading models."""

    def __init__(
        self,
        default_text: str = "Hola Viernes",
        responses: list[str] | dict[str, str] | None = None,
    ) -> None:
        self.default_text = default_text
        self.responses = responses
        self._call_count: int = 0
        self._history: list[Path] = []

    @property
    def call_count(self) -> int:
        return self._call_count

    @property
    def history(self) -> list[Path]:
        return list(self._history)

    def transcribe(self, audio_path: Path) -> str:
        """Simulates transcription with predefined answers."""
        if not audio_path.exists():
            raise FileNotFoundError(f"Audio file does not exist: {audio_path}")

        self._call_count += 1
        self._history.append(audio_path)

        if isinstance(self.responses, dict):
            # Key lookup by filename or exact path
            result = self.responses.get(audio_path.name, self.responses.get(str(audio_path), self.default_text))
        elif isinstance(self.responses, list) and self.responses:
            idx = (self._call_count - 1) % len(self.responses)
            result = self.responses[idx]
        else:
            result = self.default_text

        normalized = result.strip()
        logger.debug("FakeSTTProvider transcribed '%s' -> '%s'", audio_path.name, normalized)
        return normalized
