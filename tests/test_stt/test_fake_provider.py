from pathlib import Path

import pytest

from viernes.stt.fake_provider import FakeSTTProvider


def test_fake_provider_non_existent_file(tmp_path: Path):
    provider = FakeSTTProvider()
    non_existent = tmp_path / "missing.wav"

    with pytest.raises(FileNotFoundError, match="does not exist"):
        provider.transcribe(non_existent)


def test_fake_provider_default_response(tmp_path: Path):
    audio_file = tmp_path / "test.wav"
    audio_file.write_bytes(b"dummy audio content")

    provider = FakeSTTProvider(default_text="  Viernes activa la luz  ")
    result = provider.transcribe(audio_file)

    assert result == "Viernes activa la luz"
    assert provider.call_count == 1
    assert provider.history == [audio_file]


def test_fake_provider_list_responses(tmp_path: Path):
    audio_file1 = tmp_path / "cmd1.wav"
    audio_file1.write_bytes(b"dummy1")
    audio_file2 = tmp_path / "cmd2.wav"
    audio_file2.write_bytes(b"dummy2")

    provider = FakeSTTProvider(responses=["primer comando", "segundo comando"])

    assert provider.transcribe(audio_file1) == "primer comando"
    assert provider.transcribe(audio_file2) == "segundo comando"
    assert provider.transcribe(audio_file1) == "primer comando"
    assert provider.call_count == 3


def test_fake_provider_dict_responses(tmp_path: Path):
    audio_a = tmp_path / "saludo.wav"
    audio_a.write_bytes(b"saludo")
    audio_b = tmp_path / "hora.wav"
    audio_b.write_bytes(b"hora")

    provider = FakeSTTProvider(
        responses={
            "saludo.wav": "Hola Viernes",
            "hora.wav": "¿Qué hora es?",
        }
    )

    assert provider.transcribe(audio_a) == "Hola Viernes"
    assert provider.transcribe(audio_b) == "¿Qué hora es?"
