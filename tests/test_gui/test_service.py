import tempfile
from pathlib import Path
from unittest.mock import MagicMock, patch

import pytest

from viernes.gui.service import (
    DEFAULT_TEST_PHRASE,
    PRESET_VOICES,
    ConfigService,
)


def test_resolve_env_path_defaults(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.chdir(tmp_path)
    env_file = tmp_path / ".env"
    env_file.write_text("FOO=bar\n")
    service = ConfigService()
    assert service.env_path == env_file


def test_read_and_save_config(tmp_path: Path) -> None:
    env_file = tmp_path / ".env"
    env_file.write_text("ELEVENLABS_API_KEY=initial_key\n# comment\n")

    service = ConfigService(env_path=env_file)
    cfg = service.read_config()
    assert cfg.get("ELEVENLABS_API_KEY") == "initial_key"

    service.save_config({"ELEVENLABS_API_KEY": "updated_key", "ELEVENLABS_VOICE_ID": "voice_123"})
    cfg_updated = service.read_config()
    assert cfg_updated.get("ELEVENLABS_API_KEY") == "updated_key"
    assert cfg_updated.get("ELEVENLABS_VOICE_ID") == "voice_123"

    # Preserves comments
    content = env_file.read_text()
    assert "# comment" in content


def test_audio_device_listing() -> None:
    mock_devices = [
        {"index": 0, "name": "Built-in Mic", "max_input_channels": 2, "max_output_channels": 0},
        {"index": 1, "name": "Headphones", "max_input_channels": 0, "max_output_channels": 2},
    ]
    with patch("viernes.gui.service.list_audio_devices", return_value=mock_devices):
        service = ConfigService()
        inputs = service.get_audio_input_devices()
        outputs = service.get_audio_output_devices()

        assert any("[0] Built-in Mic" in disp for disp, raw in inputs)
        assert any("[1] Headphones" in disp for disp, raw in outputs)


def test_test_voice_empty_key_raises() -> None:
    service = ConfigService()
    with pytest.raises(ValueError, match="La API key de ElevenLabs no puede estar vacía"):
        service.test_voice(api_key="", voice_id="any")


def test_test_voice_success(tmp_path: Path) -> None:
    service = ConfigService()
    dummy_out = tmp_path / "audition_test.mp3"
    dummy_out.write_bytes(b"dummy")

    with (
        patch("viernes.tts.elevenlabs_provider.ElevenLabsProvider.synthesize", return_value=dummy_out) as mock_synth,
        patch("viernes.audio.player.AudioPlayer.play") as mock_play,
    ):
        res = service.test_voice(
            api_key="sk_test",
            voice_id=PRESET_VOICES["Jessica (Oficial)"],
            temp_dir=tmp_path,
        )
        assert res == dummy_out
        mock_synth.assert_called_once()
        mock_play.assert_called_once_with(dummy_out)


def test_service_status_and_control() -> None:
    service = ConfigService()
    with patch("subprocess.run") as mock_run:
        mock_run.return_value = MagicMock(returncode=0, stdout="active\n", stderr="")
        is_active, status = service.get_service_status("viernes.service")
        assert is_active is True
        assert status == "active"

        success, msg = service.control_service("restart")
        assert success is True
        assert "completado exitosamente" in msg

    # Invalid action
    success, msg = service.control_service("invalid_action")
    assert success is False
    assert "Acción inválida" in msg


def test_format_audition_error() -> None:
    # 401 Credential error
    err_401 = Exception("status_code: 401, text: invalid api key")
    msg_401 = ConfigService.format_audition_error(err_401)
    assert "Credenciales inválidas (401)" in msg_401

    class CustomHttpError(Exception):
        def __init__(self, code: int):
            self.status_code = code

    assert "Credenciales inválidas (401)" in ConfigService.format_audition_error(CustomHttpError(401))

    # 402/429 Quota exhausted
    err_402 = Exception("Payment Required: quota_exceeded")
    msg_402 = ConfigService.format_audition_error(err_402)
    assert "Cuota agotada o límite alcanzado (402/429)" in msg_402
    assert "Cuota agotada o límite alcanzado (402/429)" in ConfigService.format_audition_error(CustomHttpError(429))

    # Timeout error
    err_timeout = TimeoutError("Connection timed out after 5.0s")
    assert "Tiempo de espera agotado" in ConfigService.format_audition_error(err_timeout)

    # Network connection error
    err_network = ConnectionResetError("Connection refused by peer")
    assert "Error de conexión de red" in ConfigService.format_audition_error(err_network)


def test_stop_test_voice() -> None:
    service = ConfigService()
    mock_player = MagicMock()
    service._current_player = mock_player

    service.stop_test_voice()

    mock_player.stop_all.assert_called_once()
    assert service._current_player is None
