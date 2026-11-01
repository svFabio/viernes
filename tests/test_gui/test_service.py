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
    with patch("shutil.which", return_value="/usr/bin/systemctl"), patch("subprocess.run") as mock_run:
        mock_run.return_value = MagicMock(returncode=0, stdout="active\n", stderr="")
        is_active, status = service.get_service_status("viernes.service")
        assert is_active is True
        assert status == "active"

        success, msg = service.control_service("restart")
        assert success is True
        assert "completado exitosamente" in msg

        # Try-restart and reload-or-restart
        success_try, _ = service.control_service("try-restart")
        assert success_try is True
        success_reload, _ = service.control_service("reload-or-restart")
        assert success_reload is True

    # Invalid action
    success, msg = service.control_service("invalid_action")
    assert success is False
    assert "Acción inválida" in msg


def test_validate_api_key_scenarios() -> None:
    # Empty key is allowed
    is_valid, clean, warning = ConfigService.validate_api_key("   ")
    assert is_valid is True
    assert clean == ""
    assert warning == ""

    # Short key produces alert
    is_valid, clean, warning = ConfigService.validate_api_key("sk_short")
    assert is_valid is True
    assert clean == "sk_short"
    assert "inferior a 20 caracteres" in warning

    # Full valid key
    valid_key = "sk_808ad725799d106b95dd40636aeaeb901a9"
    is_valid, clean, warning = ConfigService.validate_api_key(f"  {valid_key}  ")
    assert is_valid is True
    assert clean == valid_key
    assert warning == ""

    # Incompatible characters sanitized
    is_valid, clean, warning = ConfigService.validate_api_key("sk_key@#$%12345678901234567890")
    assert is_valid is True
    assert "@" not in clean
    assert "#" not in clean
    assert "sanearon caracteres" in warning


def test_validate_voice_id_scenarios() -> None:
    # Empty defaults to Jessica
    is_valid, clean, warning = ConfigService.validate_voice_id("")
    assert is_valid is True
    assert clean == PRESET_VOICES["Jessica (Oficial)"]
    assert warning == ""

    # Custom voice ID sanitized
    is_valid, clean, warning = ConfigService.validate_voice_id("  custom_voice_id_123  ")
    assert is_valid is True
    assert clean == "custom_voice_id_123"

    # Incompatible characters sanitized
    is_valid, clean, warning = ConfigService.validate_voice_id("custom!voice?id")
    assert is_valid is True
    assert clean == "customvoiceid"
    assert "sanearon caracteres" in warning


def test_atomic_save_config_error_recovery(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    env_file = tmp_path / ".env"
    env_file.write_text("ORIGINAL=kept\n# comment\n")

    service = ConfigService(env_path=env_file)

    # Simulate permission error during replace
    def failing_replace(src: str, dst: str) -> None:
        raise PermissionError("Access denied")

    monkeypatch.setattr("os.replace", failing_replace)

    with pytest.raises(PermissionError, match="Access denied"):
        service.save_config({"ORIGINAL": "modified"})

    # Original file remains completely intact
    assert "ORIGINAL=kept" in env_file.read_text()
    assert "# comment" in env_file.read_text()


def test_service_status_variations() -> None:
    service = ConfigService()

    # Missing systemctl
    with patch("shutil.which", return_value=None):
        is_active, status = service.get_service_status()
        assert is_active is False
        assert status == "no disponible"

        success, msg = service.control_service("start")
        assert success is False
        assert "no disponible" in msg

    # Inactive and failed statuses
    with patch("shutil.which", return_value="/bin/systemctl"), patch("subprocess.run") as mock_run:
        mock_run.return_value = MagicMock(returncode=3, stdout="failed\n", stderr="")
        is_active, status = service.get_service_status()
        assert is_active is False
        assert status == "failed"

    # Timeout handling
    import subprocess
    with patch("shutil.which", return_value="/bin/systemctl"), patch("subprocess.run", side_effect=subprocess.TimeoutExpired(cmd="systemctl", timeout=5)):
        is_active, status = service.get_service_status()
        assert is_active is False
        assert status == "timeout"

        success, msg = service.control_service("restart")
        assert success is False
        assert "Tiempo de espera agotado" in msg

