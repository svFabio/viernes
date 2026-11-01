from pathlib import Path
from elevenlabs.client import ElevenLabs
from viernes.tts.base import TTSProvider


class ElevenLabsProvider(TTSProvider):
    """ElevenLabs cloud TTS provider."""

    def __init__(self, api_key: str, voice_id: str = "21m00Tcm4TlvDq8ikWAM"):
        if not api_key:
            raise ValueError("ELEVENLABS_API_KEY is required for ElevenLabsProvider.")
        self.client = ElevenLabs(api_key=api_key)
        self.voice_id = voice_id

    def synthesize(self, text: str, output_path: Path) -> Path:
        output_path.parent.mkdir(parents=True, exist_ok=True)
        audio_stream = self.client.text_to_speech.convert(
            voice_id=self.voice_id,
            output_format="mp3_44100_128",
            text=text,
            model_id="eleven_multilingual_v2",
        )
        with open(output_path, "wb") as f:
            for chunk in audio_stream:
                f.write(chunk)
        return output_path
