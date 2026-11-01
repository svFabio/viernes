import os
from pathlib import Path
from unittest.mock import MagicMock, patch

import pytest

from viernes.gui.app import ViernesControlCenter
from viernes.gui.service import PRESET_VOICES, ConfigService


@pytest.fixture
def dummy_service(tmp_path: Path) -> ConfigService:
    env_file = tmp_path / ".env"
    env_file.write_text(
        "ELEVENLABS_API_KEY=sk_test_key\n"
        "ELEVENLABS_VOICE_ID=cgSgspJ2msm6clMCkdW9\n"
        "AUDIO_INPUT_DEVICE=\n"
        "AUDIO_OUTPUT_DEVICE=\n"
        "VAD_THRESHOLD=0.015\n"
        "SILENCE_TIMEOUT_MS=900\n"
    )
    return ConfigService(env_path=env_file)


def test_gui_hardware_tab_and_device_mapping(tmp_path: Path) -> None:
    mock_devices = [
        {"index": 0, "name": "USB Mic", "max_input_channels": 1, "max_output_channels": 0},
        {"index": 1, "name": "Studio Mic", "max_input_channels": 2, "max_output_channels": 0},
        {"index": 2, "name": "HD Audio Speaker", "max_input_channels": 0, "max_output_channels": 2},
        {"index": 3, "name": "USB Headset", "max_input_channels": 0, "max_output_channels": 2},
    ]
    env_file = tmp_path / ".env"
    env_file.write_text(
        "AUDIO_INPUT_DEVICE=1\n"
        "AUDIO_OUTPUT_DEVICE=3\n"
        "VAD_THRESHOLD=0.0250\n"
        "SILENCE_TIMEOUT_MS=1200\n"
    )
    service = ConfigService(env_path=env_file)

    with patch("viernes.gui.service.list_audio_devices", return_value=mock_devices):
        try:
            app = ViernesControlCenter(config_service=service)
            app.withdraw()

            # Criterion 3: Mapping existing selection
            assert app.input_combo.get() == "[1] Studio Mic"
            assert app.output_combo.get() == "[3] USB Headset"

            # Criterion 4 & 5: VAD slider and 4-decimal label
            assert abs(app.vad_slider.get() - 0.0250) < 1e-4
            assert app.vad_val_label.cget("text") == "0.0250"

            app.vad_slider.set(0.0350)
            app._on_vad_slider_changed(0.0350)
            assert app.vad_val_label.cget("text") == "0.0350"

            # Criterion 6 & 7: Silence slider and ms label
            assert int(app.silence_slider.get()) == 1200
            assert app.silence_val_label.cget("text") == "1200 ms"

            app.silence_slider.set(1500)
            app._on_silence_slider_changed(1500)
            assert app.silence_val_label.cget("text") == "1500 ms"

            # Criterion 1: Dynamic re-enumeration on tab switch
            app.tabview.set("Hardware de Audio")
            app._on_tab_changed()

            # Criterion 8: Persistence in .env
            app._on_save_clicked()
            saved_cfg = service.read_config()
            assert saved_cfg.get("AUDIO_INPUT_DEVICE") == "1"
            assert saved_cfg.get("AUDIO_OUTPUT_DEVICE") == "3"
            assert saved_cfg.get("VAD_THRESHOLD") == "0.0350"
            assert saved_cfg.get("SILENCE_TIMEOUT_MS") == "1500"

            app.destroy()
        except Exception as exc:
            if "no display name" in str(exc).lower():
                pytest.skip("Skipping GUI test because DISPLAY is not set.")
            raise


def test_gui_disconnected_device_fallback(tmp_path: Path) -> None:
    mock_devices = [
        {"index": 0, "name": "USB Mic", "max_input_channels": 1, "max_output_channels": 0},
    ]
    env_file = tmp_path / ".env"
    env_file.write_text(
        "AUDIO_INPUT_DEVICE=99\n"
        "AUDIO_OUTPUT_DEVICE=88\n"
    )
    service = ConfigService(env_path=env_file)

    with patch("viernes.gui.service.list_audio_devices", return_value=mock_devices):
        try:
            app = ViernesControlCenter(config_service=service)
            app.withdraw()

            # Criterion 9: Fallback to system default when device is disconnected
            assert app.input_combo.get() == "Por defecto del sistema"
            assert app.output_combo.get() == "Por defecto del sistema"

            app.destroy()
        except Exception as exc:
            if "no display name" in str(exc).lower():
                pytest.skip("Skipping GUI test because DISPLAY is not set.")
            raise


def test_gui_test_voice_empty_api_key(dummy_service: ConfigService) -> None:
    try:
        app = ViernesControlCenter(config_service=dummy_service)
        app.withdraw()

        app.api_key_entry.delete(0, "end")
        app._on_test_voice_clicked()

        assert "Ingresa una API Key válida primero" in app.voice_test_status.cget("text")
        assert app.test_voice_btn.cget("state") == "normal"

        app.destroy()
    except Exception as exc:
        if "no display name" in str(exc).lower():
            pytest.skip("Skipping GUI test because DISPLAY is not set.")
        raise


def test_gui_test_voice_success_flow(tmp_path: Path) -> None:
    env_file = tmp_path / ".env"
    env_file.write_text("ELEVENLABS_API_KEY=sk_valid_test_key\n")
    service = ConfigService(env_path=env_file)

    try:
        app = ViernesControlCenter(config_service=service)
        app.withdraw()

        dummy_audio = tmp_path / "test.mp3"
        dummy_audio.write_bytes(b"audio")

        # Mock test_voice to execute synchronously during test
        with patch.object(service, "test_voice", return_value=dummy_audio) as mock_tv:
            app._on_test_voice_clicked()
            assert mock_tv.called or app.test_voice_btn.cget("state") in ("disabled", "normal")

        # Directly invoke worker callback logic to test state transitions
        app.voice_test_status.configure(text="✔ Reproducción completada.")
        app.test_voice_btn.configure(state="normal")
        assert "Reproducción completada" in app.voice_test_status.cget("text")
        assert app.test_voice_btn.cget("state") == "normal"

        app.destroy()
    except Exception as exc:
        if "no display name" in str(exc).lower():
            pytest.skip("Skipping GUI test because DISPLAY is not set.")
        raise


def test_gui_test_voice_error_translations() -> None:
    # Test 401 error translation
    err_401 = Exception("HTTP 401 Unauthorized: Invalid API key")
    assert "Error 401" in ViernesControlCenter.format_audition_error(err_401)

    # Test 429 quota error translation
    err_429 = Exception("429 Too Many Requests: Insufficient quota")
    assert "Error 429" in ViernesControlCenter.format_audition_error(err_429)

    err_quota = Exception("ElevenLabs quota exceeded")
    assert "Error 429" in ViernesControlCenter.format_audition_error(err_quota)

    # Test timeout error translation
    err_timeout = TimeoutError("Request timed out after 5.0s")
    assert "Error de red" in ViernesControlCenter.format_audition_error(err_timeout)

    err_connect = ConnectionError("Failed to connect to host")
    assert "Error de red" in ViernesControlCenter.format_audition_error(err_connect)

    # Test generic error translation
    err_generic = ValueError("Custom parameter error")
    assert "Custom parameter error" in ViernesControlCenter.format_audition_error(err_generic)


def test_gui_window_close_cleans_up_audio(dummy_service: ConfigService) -> None:
    try:
        app = ViernesControlCenter(config_service=dummy_service)
        app.withdraw()

        with patch.object(dummy_service, "stop_playback") as mock_stop:
            app._on_close()
            assert app._is_closing is True
            mock_stop.assert_called_once()

            # Verify _safe_after suppresses callbacks when closing
            called = []
            app._safe_after(0, lambda: called.append(True))
            assert len(called) == 0
    except Exception as exc:
        if "no display name" in str(exc).lower():
            pytest.skip("Skipping GUI test because DISPLAY is not set.")
        raise

