import logging
from pathlib import Path
from typing import Any

from viernes.stt.base import STTProvider

logger = logging.getLogger(__name__)

SUPPORTED_MODEL_SIZES = ("tiny", "base", "small", "medium", "large", "large-v2", "large-v3")


class WhisperSTTProvider(STTProvider):
    """Local Speech-to-Text provider powered by faster-whisper."""

    def __init__(
        self,
        model_size: str = "base",
        device: str = "cpu",
        compute_type: str = "int8",
        language: str | None = "es",
        beam_size: int = 5,
        download_root: Path | str | None = None,
        model: Any | None = None,
    ) -> None:
        self.model_size = model_size if model_size in SUPPORTED_MODEL_SIZES else "base"
        if model_size not in SUPPORTED_MODEL_SIZES:
            logger.warning(
                "Whisper model_size '%s' not recognized. Defaulting to 'base'.",
                model_size,
            )

        self.device = device
        self.compute_type = compute_type
        self.language = language
        self.beam_size = beam_size
        self.download_root = str(download_root) if download_root else None

        # Allow model injection for automated unit tests
        self._model = model

    @property
    def model(self) -> Any:
        """Lazily initializes and loads the faster-whisper model."""
        if self._model is None:
            try:
                from faster_whisper import WhisperModel
            except ImportError as err:
                raise ImportError(
                    "faster-whisper is not installed. Install it with: uv add faster-whisper"
                ) from err

            logger.info(
                "Loading local faster-whisper model '%s' (device: %s, compute_type: %s)...",
                self.model_size,
                self.device,
                self.compute_type,
            )
            self._model = WhisperModel(
                self.model_size,
                device=self.device,
                compute_type=self.compute_type,
                download_root=self.download_root,
            )
        return self._model

    def transcribe(self, audio_path: Path) -> str:
        """Transcribes a local audio file using the faster-whisper model.

        Args:
            audio_path: Path to the .wav or other audio file.

        Returns:
            str: Normalized transcribed text, or empty string if unintelligible or silent.

        Raises:
            FileNotFoundError: If the specified file does not exist.
        """
        if not audio_path.exists():
            raise FileNotFoundError(f"Audio file does not exist: {audio_path}")

        if audio_path.stat().st_size == 0:
            logger.debug("Audio file %s is empty (0 bytes). Returning empty transcription.", audio_path)
            return ""

        logger.debug("Transcribing audio file: %s (language=%s)", audio_path, self.language)

        try:
            segments, info = self.model.transcribe(
                str(audio_path),
                language=self.language,
                beam_size=self.beam_size,
                vad_filter=False,  # Audio has already been segmented by VAD
            )

            # Collect and concatenate text across all segments
            segment_texts = [segment.text.strip() for segment in segments if segment.text.strip()]
            full_text = " ".join(segment_texts).strip()

            logger.info(
                "Transcription completed for %s: '%s' (language detected: %s)",
                audio_path.name,
                full_text,
                getattr(info, "language", self.language),
            )
            return full_text

        except (RuntimeError, ValueError, OSError) as exc:
            logger.warning("Error during whisper transcription of %s: %s", audio_path, exc)
            return ""
