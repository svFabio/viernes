import wave
from dataclasses import dataclass
from pathlib import Path
from unittest.mock import MagicMock

import pytest

from viernes.stt.whisper_provider import WhisperSTTProvider


@dataclass
class DummySegment:
    text: str


@dataclass
class DummyTranscriptionInfo:
    language: str = "es"
    language_probability: float = 0.98


@pytest.fixture
def sample_wav(tmp_path: Path) -> Path:
    wav_path = tmp_path / "sample.wav"
    with wave.open(str(wav_path), "wb") as wf:
        wf.setnchannels(1)
        wf.setsampwidth(2)
        wf.setframerate(16000)
        wf.writeframes(b"\x00\x00" * 1600)  # 0.1s of silence
    return wav_path


def test_whisper_model_size_validation():
    p1 = WhisperSTTProvider(model_size="tiny")
    assert p1.model_size == "tiny"

    p2 = WhisperSTTProvider(model_size="small")
    assert p2.model_size == "small"

    p3 = WhisperSTTProvider(model_size="unknown-model")
    assert p3.model_size == "base"  # Defaults to base


def test_whisper_non_existent_file(tmp_path: Path):
    provider = WhisperSTTProvider()
    missing_file = tmp_path / "does_not_exist.wav"

    with pytest.raises(FileNotFoundError, match="does not exist"):
        provider.transcribe(missing_file)


def test_whisper_empty_file_returns_empty_string(tmp_path: Path):
    empty_file = tmp_path / "empty.wav"
    empty_file.touch()

    mock_model = MagicMock()
    provider = WhisperSTTProvider(model=mock_model)

    result = provider.transcribe(empty_file)
    assert result == ""
    mock_model.transcribe.assert_not_called()


def test_whisper_successful_transcription(sample_wav: Path):
    mock_model = MagicMock()
    segments = [
        DummySegment(text="  Hola"),
        DummySegment(text="Viernes, ¿cómo estás?  "),
    ]
    info = DummyTranscriptionInfo(language="es")
    mock_model.transcribe.return_value = (segments, info)

    provider = WhisperSTTProvider(language="es", model=mock_model)
    result = provider.transcribe(sample_wav)

    assert result == "Hola Viernes, ¿cómo estás?"
    mock_model.transcribe.assert_called_once_with(
        str(sample_wav),
        language="es",
        beam_size=5,
        vad_filter=False,
    )


def test_whisper_silent_or_unintelligible_audio(sample_wav: Path):
    mock_model = MagicMock()
    segments = []  # No speech detected
    info = DummyTranscriptionInfo(language="es")
    mock_model.transcribe.return_value = (segments, info)

    provider = WhisperSTTProvider(model=mock_model)
    result = provider.transcribe(sample_wav)

    assert result == ""


def test_whisper_multilingual_mode(sample_wav: Path):
    mock_model = MagicMock()
    segments = [DummySegment(text="Hello Friday")]
    info = DummyTranscriptionInfo(language="en")
    mock_model.transcribe.return_value = (segments, info)

    provider = WhisperSTTProvider(language=None, model=mock_model)
    result = provider.transcribe(sample_wav)

    assert result == "Hello Friday"
    mock_model.transcribe.assert_called_once_with(
        str(sample_wav),
        language=None,
        beam_size=5,
        vad_filter=False,
    )


def test_whisper_exception_handled_cleanly(sample_wav: Path):
    mock_model = MagicMock()
    mock_model.transcribe.side_effect = RuntimeError("Model execution failed")

    provider = WhisperSTTProvider(model=mock_model)
    result = provider.transcribe(sample_wav)

    assert result == ""
