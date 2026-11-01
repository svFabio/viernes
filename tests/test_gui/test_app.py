import os
import tkinter as tk
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


def test_gui_initialization_and_load(dummy_service: ConfigService) -> None:
    # Set DISPLAY to avoid crash if running headless or use Tkinter mock
    try:
        app = ViernesControlCenter(config_service=dummy_service)
        app.withdraw()  # keep hidden during tests

        # Verify initial values were loaded into controls
        assert app.api_key_entry.get() == "sk_test_key"
        assert app.voice_id_entry.get() == "cgSgspJ2msm6clMCkdW9"
        assert app.voice_combo.get() == "Jessica (Oficial)"
        assert abs(app.vad_slider.get() - 0.015) < 1e-4
        assert int(app.silence_slider.get()) == 900

        # Test toggle API key visibility
        assert app.api_key_entry.cget("show") == "*"
        app._toggle_api_key_visibility()
        assert app.api_key_entry.cget("show") == ""
        app._toggle_api_key_visibility()
        assert app.api_key_entry.cget("show") == "*"

        # Test voice preset change
        app._on_voice_preset_selected("Lily (Cálida)")
        assert app.voice_id_entry.get() == PRESET_VOICES["Lily (Cálida)"]

        # Test save button
        app.api_key_entry.delete(0, "end")
        app.api_key_entry.insert(0, "sk_new_secret")
        app._on_save_clicked()

        saved_cfg = dummy_service.read_config()
        assert saved_cfg.get("ELEVENLABS_API_KEY") == "sk_new_secret"

        app.destroy()
    except Exception as exc:
        # If no X11/Wayland display server is available in CI
        if "no display name" in str(exc).lower():
            pytest.skip("Skipping GUI test because DISPLAY is not set.")
        raise


def test_gui_safe_after_suppresses_tcl_error(dummy_service: ConfigService) -> None:
    try:
        app = ViernesControlCenter(config_service=dummy_service)
        app.withdraw()

        called = []
        # Normal safe_after
        app.safe_after(0, lambda: called.append(True))
        app.update()
        assert called == [True]

        # Closing state suppresses after()
        app._is_closing = True
        res = app.safe_after(0, lambda: called.append(False))
        assert res is None

        # TclError suppression
        app._is_closing = False
        with patch.object(app, "after", side_effect=tk.TclError("invalid command name")):
            assert app.safe_after(0, lambda: None) is None

        app.destroy()
    except Exception as exc:
        if "no display name" in str(exc).lower():
            pytest.skip("Skipping GUI test because DISPLAY is not set.")
        raise


def test_gui_close_window_stops_player_and_destroys(dummy_service: ConfigService) -> None:
    try:
        app = ViernesControlCenter(config_service=dummy_service)
        app.withdraw()

        with (
            patch.object(dummy_service, "stop_test_voice") as mock_stop_voice,
            patch.object(app, "destroy") as mock_destroy,
        ):
            app._on_close_window()
            assert app._is_closing is True
            mock_stop_voice.assert_called_once()
            mock_destroy.assert_called_once()

    except Exception as exc:
        if "no display name" in str(exc).lower():
            pytest.skip("Skipping GUI test because DISPLAY is not set.")
        raise


def test_gui_test_voice_error_interception(dummy_service: ConfigService) -> None:
    import time

    try:
        app = ViernesControlCenter(config_service=dummy_service)
        app.withdraw()

        # 1. 401 Credential error interception
        with patch.object(
            dummy_service,
            "test_voice",
            side_effect=Exception("status_code: 401 unauthorized"),
        ):
            app._on_test_voice_clicked()
            for _ in range(50):
                app.update()
                time.sleep(0.02)
                if "Credenciales inválidas (401)" in app.voice_test_status.cget("text"):
                    break

            status_text = app.voice_test_status.cget("text")
            assert "Credenciales inválidas (401)" in status_text

        # 2. 402/429 Quota exhausted interception
        with patch.object(
            dummy_service,
            "test_voice",
            side_effect=Exception("429 rate limit quota_exceeded"),
        ):
            app._on_test_voice_clicked()
            for _ in range(50):
                app.update()
                time.sleep(0.02)
                if "Cuota agotada o límite alcanzado (402/429)" in app.voice_test_status.cget("text"):
                    break

            status_text = app.voice_test_status.cget("text")
            assert "Cuota agotada o límite alcanzado (402/429)" in status_text

        app.destroy()
    except Exception as exc:
        if "no display name" in str(exc).lower():
            pytest.skip("Skipping GUI test because DISPLAY is not set.")
        raise
