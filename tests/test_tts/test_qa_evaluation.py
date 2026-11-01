import subprocess
from unittest.mock import MagicMock, patch

import pytest
from pydantic import ValidationError

from viernes.audio.player import AudioPlayer
from viernes.config import Settings
from viernes.tts.base import TTSProvider
from viernes.tts.elevenlabs_provider import ElevenLabsProvider
from viernes.tts.piper_provider import PiperProvider
from viernes.tts.resilient import ResilientTTS


def test_qa_unicode_and_emojis_in_piper(tmp_path):
    mock_proc = MagicMock()
    mock_proc.communicate.return_value = ("", "")
    mock_proc.returncode = 0

    with (
        patch("shutil.which", return_value="/usr/bin/piper"),
        patch("subprocess.Popen", return_value=mock_proc),
    ):
        provider = PiperProvider(temp_dir=tmp_path)
        unicode_text = "¡Hola, mundo! 🚀 ¿Cómo estás hoy? ñandú y café"
        out_file = tmp_path / "unicode.wav"
        result = provider.synthesize(unicode_text, output_path=out_file)

        assert result == out_file.resolve()
        mock_proc.communicate.assert_called_once_with(input=unicode_text)


def test_qa_multi_paragraph_text_in_elevenlabs(tmp_path):
    mock_client = MagicMock()
    mock_client.text_to_speech.convert.return_value = [b"chunk-data"]

    provider = ElevenLabsProvider(
        api_key="sk_valid_key_12345",
        temp_dir=tmp_path,
        client=mock_client,
    )
    long_text = "Primer párrafo.\n\nSegundo párrafo con más información.\n\nTercer párrafo final."
    result = provider.synthesize(long_text)
    assert result.exists()
    assert result.read_bytes() == b"chunk-data"


def test_qa_circuit_breaker_no_flapping():
    primary = MagicMock(spec=TTSProvider)
    fallback = MagicMock(spec=TTSProvider)

    primary.synthesize.side_effect = RuntimeError("429 Too Many Requests: quota_exceeded")
    fallback.synthesize.return_value = "/tmp/fallback.wav"

    on_fallback = MagicMock()
    resilient = ResilientTTS(primary=primary, fallback=fallback, on_fallback_activated=on_fallback)

    for i in range(10):
        res = resilient.synthesize(f"Message {i}")
        assert res == "/tmp/fallback.wav"

    assert primary.synthesize.call_count == 1
    assert fallback.synthesize.call_count == 10
    assert on_fallback.call_count == 1
    assert resilient.quota_exhausted is True


def test_qa_settings_frozen_immutability():
    config = Settings()
    with pytest.raises(ValidationError):
        config.elevenlabs_api_key = "new_key"  # type: ignore


def test_qa_settings_whitespace_only_key_not_configured():
    config = Settings(elevenlabs_api_key="   \t\n  ")
    assert config.is_elevenlabs_configured is False


def test_qa_audio_player_stop_all_idempotent():
    with patch("shutil.which", return_value="/usr/bin/mpv"):
        player = AudioPlayer()
        player.stop_all()
        player.stop_all()
        assert len(player._active_processes) == 0


def test_qa_audio_player_process_failure_in_blocking(tmp_path):
    audio_file = tmp_path / "test.mp3"
    audio_file.write_bytes(b"data")

    mock_proc = MagicMock(spec=subprocess.Popen)
    mock_proc.poll.return_value = 1  # Exited with error
    mock_proc.wait.return_value = 1

    with (
        patch("shutil.which", return_value="/usr/bin/mpv"),
        patch("subprocess.Popen", return_value=mock_proc),
    ):
        player = AudioPlayer()
        proc = player.play(audio_file, blocking=True)
        assert proc.poll() == 1
