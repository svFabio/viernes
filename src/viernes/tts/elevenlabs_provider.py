import os
import uuid
from pathlib import Path

import httpx
from elevenlabs.client import ElevenLabs

from viernes.tts.base import TTSProvider


class ElevenLabsProvider(TTSProvider):
    """ElevenLabs cloud TTS provider."""

    DEFAULT_VOICE_ID = "21m00Tcm4TlvDq8ikWAM"
    DEFAULT_MODEL_ID = "eleven_multilingual_v2"
    DEFAULT_OUTPUT_FORMAT = "mp3_44100_128"
    DEFAULT_TIMEOUT_SECONDS = 5.0

    def __init__(
        self,
        api_key: str,
        voice_id: str = DEFAULT_VOICE_ID,
        model_id: str = DEFAULT_MODEL_ID,
        output_format: str = DEFAULT_OUTPUT_FORMAT,
        timeout: float = DEFAULT_TIMEOUT_SECONDS,
        temp_dir: Path | str | None = None,
        client: ElevenLabs | None = None,
    ):
        if not api_key:
            raise ValueError("ELEVENLABS_API_KEY is required for ElevenLabsProvider.")
        self.api_key = api_key
        self.voice_id = voice_id or self.DEFAULT_VOICE_ID
        self.model_id = model_id
        self.output_format = output_format
        self.timeout = timeout
        self.temp_dir = Path(temp_dir or os.getenv("VIERNES_TEMP_DIR", "/tmp/viernes"))
        self.client = client or ElevenLabs(api_key=api_key, timeout=timeout)

    def synthesize(self, text: str, output_path: Path | None = None) -> Path:
        """Synthesize text into MP3 audio via streaming chunks."""
        if not text or not text.strip():
            raise ValueError("Text cannot be empty or whitespace only.")

        if output_path is None:
            self.temp_dir.mkdir(parents=True, exist_ok=True)
            output_path = self.temp_dir / f"elevenlabs_{uuid.uuid4().hex[:8]}.mp3"
        else:
            output_path = Path(output_path)
            output_path.parent.mkdir(parents=True, exist_ok=True)

        try:
            audio_stream = self.client.text_to_speech.convert(
                voice_id=self.voice_id,
                output_format=self.output_format,
                text=text.strip(),
                model_id=self.model_id,
            )
            with open(output_path, "wb") as f:
                if isinstance(audio_stream, (bytes, bytearray)):
                    f.write(audio_stream)
                else:
                    f.writelines(audio_stream)
        except (httpx.TimeoutException, TimeoutError) as exc:
            raise TimeoutError(f"ElevenLabs request timed out after {self.timeout}s: {exc}") from exc

        return output_path.resolve()

    def __repr__(self) -> str:
        masked_key = f"{self.api_key[:3]}***{self.api_key[-4:]}" if len(self.api_key) > 7 else "***"
        return f"ElevenLabsProvider(voice_id='{self.voice_id}', api_key='{masked_key}')"
