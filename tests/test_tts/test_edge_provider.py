from pathlib import Path
from unittest.mock import AsyncMock, patch

import pytest

from viernes.tts.edge_provider import DEFAULT_EDGE_VOICE, EdgeTTSProvider
from viernes.tts.resilient import ResilientTTS


def test_edge_tts_provider_init(tmp_path: Path) -> None:
    provider = EdgeTTSProvider(voice="es-CO-SalomeNeural", temp_dir=tmp_path)
    assert provider.voice == "es-CO-SalomeNeural"
    assert provider.temp_dir == tmp_path


def test_edge_tts_provider_empty_text_raises(tmp_path: Path) -> None:
    provider = EdgeTTSProvider(temp_dir=tmp_path)
    with pytest.raises(ValueError, match="Text cannot be empty"):
        provider.synthesize("   ")


def test_edge_tts_provider_synthesize_mocked(tmp_path: Path) -> None:
    provider = EdgeTTSProvider(temp_dir=tmp_path)
    out_file = tmp_path / "mocked_salome.mp3"

    async def fake_save(dest: str) -> None:
        Path(dest).write_bytes(b"mocked_mp3_data")

    with patch("edge_tts.Communicate.save", side_effect=fake_save):
        result = provider.synthesize("Hola Señor", out_file)

    assert result == out_file
    assert result.exists()
    assert result.read_bytes() == b"mocked_mp3_data"


def test_resilient_tts_fails_over_to_edge_provider(tmp_path: Path) -> None:
    class FailingElevenLabs:
        def synthesize(self, text: str, output_path: Path | None = None) -> Path:
            raise RuntimeError("402 payment_required quota exceeded")

    edge_fallback = EdgeTTSProvider(voice="es-CO-SalomeNeural", temp_dir=tmp_path)
    out_file = tmp_path / "fallback_output.mp3"

    async def fake_save(dest: str) -> None:
        Path(dest).write_bytes(b"salome_audio")

    with patch("edge_tts.Communicate.save", side_effect=fake_save):
        resilient = ResilientTTS(
            primary=FailingElevenLabs(),  # type: ignore[arg-type]
            fallback=edge_fallback,
        )
        result = resilient.synthesize("Texto de prueba", out_file)

    assert resilient.quota_exhausted is True
    assert result == out_file
    assert result.read_bytes() == b"salome_audio"
