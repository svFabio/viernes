from unittest.mock import MagicMock

import httpx
import pytest

from viernes.tts.elevenlabs_provider import ElevenLabsProvider


def test_init_requires_api_key():
    with pytest.raises(ValueError, match="ELEVENLABS_API_KEY is required"):
        ElevenLabsProvider(api_key="")


def test_init_defaults():
    mock_client = MagicMock()
    provider = ElevenLabsProvider(api_key="sk_dummy_12345678", client=mock_client)
    assert provider.voice_id == "21m00Tcm4TlvDq8ikWAM"
    assert provider.model_id == "eleven_multilingual_v2"
    assert provider.output_format == "mp3_44100_128"
    assert provider.timeout == 5.0
    assert "sk_***5678" in repr(provider)
    assert "1234" not in repr(provider)


def test_init_custom_params():
    mock_client = MagicMock()
    provider = ElevenLabsProvider(
        api_key="sk_dummy_12345678",
        voice_id="custom_voice",
        model_id="custom_model",
        output_format="pcm_16000",
        timeout=10.0,
        client=mock_client,
    )
    assert provider.voice_id == "custom_voice"
    assert provider.model_id == "custom_model"
    assert provider.output_format == "pcm_16000"
    assert provider.timeout == 10.0


def test_synthesize_empty_text_raises():
    mock_client = MagicMock()
    provider = ElevenLabsProvider(api_key="sk_dummy_12345678", client=mock_client)

    with pytest.raises(ValueError, match="Text cannot be empty"):
        provider.synthesize("")

    with pytest.raises(ValueError, match="Text cannot be empty"):
        provider.synthesize("   \n\t  ")

    mock_client.text_to_speech.convert.assert_not_called()


def test_synthesize_success_with_output_path(tmp_path):
    mock_client = MagicMock()
    mock_stream = [b"chunk-1", b"-chunk-2", b"-chunk-3"]
    mock_client.text_to_speech.convert.return_value = iter(mock_stream)

    provider = ElevenLabsProvider(
        api_key="sk_dummy_12345678",
        voice_id="custom_voice_id",
        client=mock_client,
    )

    target_file = tmp_path / "output.mp3"
    result = provider.synthesize("Hello world", output_path=target_file)

    assert result == target_file.resolve()
    assert target_file.exists()
    assert target_file.read_bytes() == b"chunk-1-chunk-2-chunk-3"

    mock_client.text_to_speech.convert.assert_called_once_with(
        voice_id="custom_voice_id",
        output_format="mp3_44100_128",
        text="Hello world",
        model_id="eleven_multilingual_v2",
    )


def test_synthesize_bytes_stream(tmp_path):
    mock_client = MagicMock()
    mock_client.text_to_speech.convert.return_value = b"raw-bytes-audio"

    provider = ElevenLabsProvider(
        api_key="sk_dummy_12345678",
        client=mock_client,
    )

    target_file = tmp_path / "raw.mp3"
    received_chunks = []
    result = provider.synthesize(
        "Test raw",
        output_path=target_file,
        on_chunk=lambda c: received_chunks.append(c),
    )
    assert result == target_file.resolve()
    assert target_file.read_bytes() == b"raw-bytes-audio"
    assert received_chunks == [b"raw-bytes-audio"]


def test_synthesize_default_temp_dir(tmp_path):
    mock_client = MagicMock()
    mock_client.text_to_speech.convert.return_value = [b"audio-data"]

    provider = ElevenLabsProvider(
        api_key="sk_dummy_12345678",
        temp_dir=tmp_path,
        client=mock_client,
    )

    result = provider.synthesize("Testing temp generation")
    assert result.exists()
    assert result.parent == tmp_path
    assert result.suffix == ".mp3"
    assert result.read_bytes() == b"audio-data"


def test_synthesize_timeout_raises_timeout_error(tmp_path):
    mock_client = MagicMock()
    mock_client.text_to_speech.convert.side_effect = httpx.ReadTimeout("Connection timed out")

    provider = ElevenLabsProvider(
        api_key="sk_dummy_12345678",
        client=mock_client,
    )

    with pytest.raises(TimeoutError, match="ElevenLabs request timed out"):
        provider.synthesize("Timeout test", output_path=tmp_path / "timeout.mp3")
