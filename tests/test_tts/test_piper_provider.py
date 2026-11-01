import subprocess
from unittest.mock import MagicMock, patch

import pytest

from viernes.tts.piper_provider import PiperProvider


def test_piper_empty_text_raises_value_error():
    provider = PiperProvider()
    with pytest.raises(ValueError, match="Text cannot be empty"):
        provider.synthesize("")
    with pytest.raises(ValueError, match="Text cannot be empty"):
        provider.synthesize("   \n\t  ")


def test_piper_binary_not_found(tmp_path):
    provider = PiperProvider(piper_bin="non_existent_piper_binary", temp_dir=tmp_path)
    with pytest.raises(FileNotFoundError, match="not found in PATH"):
        provider.synthesize("Hola mundo", output_path=tmp_path / "out.wav")


def test_piper_synthesize_success(tmp_path):
    mock_proc = MagicMock()
    mock_proc.communicate.return_value = ("", "")
    mock_proc.returncode = 0

    with (
        patch("shutil.which", return_value="/usr/bin/piper"),
        patch("subprocess.Popen", return_value=mock_proc) as mock_popen,
    ):
        provider = PiperProvider(
            model_path="/models/es_ES.onnx",
            piper_bin="piper",
            temp_dir=tmp_path,
        )
        out_file = tmp_path / "custom.wav"
        result = provider.synthesize("Probando síntesis con Piper", output_path=out_file)

        assert result == out_file.resolve()
        mock_popen.assert_called_once_with(
            ["/usr/bin/piper", "--model", "/models/es_ES.onnx", "--output_file", str(out_file)],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        mock_proc.communicate.assert_called_once_with(input="Probando síntesis con Piper")


def test_piper_synthesize_without_model(tmp_path):
    mock_proc = MagicMock()
    mock_proc.communicate.return_value = ("", "")
    mock_proc.returncode = 0

    with (
        patch("shutil.which", return_value="/usr/bin/piper"),
        patch("subprocess.Popen", return_value=mock_proc) as mock_popen,
    ):
        provider = PiperProvider(model_path="", temp_dir=tmp_path)
        out_file = tmp_path / "no_model.wav"
        result = provider.synthesize("Texto sin modelo", output_path=out_file)

        assert result == out_file.resolve()
        mock_popen.assert_called_once_with(
            ["/usr/bin/piper", "--output_file", str(out_file)],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )


def test_piper_synthesize_default_temp_dir(tmp_path):
    mock_proc = MagicMock()
    mock_proc.communicate.return_value = ("", "")
    mock_proc.returncode = 0

    with (
        patch("shutil.which", return_value="/usr/bin/piper"),
        patch("subprocess.Popen", return_value=mock_proc),
    ):
        provider = PiperProvider(temp_dir=tmp_path)
        result = provider.synthesize("Mensaje temporal")

        assert result.parent == tmp_path
        assert result.suffix == ".wav"


def test_piper_execution_failure(tmp_path):
    mock_proc = MagicMock()
    mock_proc.communicate.return_value = ("", "Model file corrupted")
    mock_proc.returncode = 1

    with (
        patch("shutil.which", return_value="/usr/bin/piper"),
        patch("subprocess.Popen", return_value=mock_proc),
    ):
        provider = PiperProvider(temp_dir=tmp_path)
        with pytest.raises(RuntimeError, match="Piper execution failed: Model file corrupted"):
            provider.synthesize("Fallo esperado", output_path=tmp_path / "fail.wav")


def test_piper_repr_and_env(monkeypatch, tmp_path):
    monkeypatch.setenv("PIPER_MODEL_PATH", "/env/model.onnx")
    monkeypatch.setenv("PIPER_BIN", "custom_piper")
    provider = PiperProvider(temp_dir=tmp_path)

    assert provider.model_path == "/env/model.onnx"
    assert provider.piper_bin == "custom_piper"
    assert "custom_piper" in repr(provider)
    assert "/env/model.onnx" in repr(provider)
