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


def test_gui_voice_selection_and_custom_id(dummy_service: ConfigService) -> None:
    try:
        app = ViernesControlCenter(config_service=dummy_service)
        app.withdraw()

        # Select custom from dropdown
        app._on_voice_preset_selected("Personalizada (Custom ID)")
        assert app.voice_combo.get() == "Personalizada (Custom ID)"

        # Typing custom ID sets dropdown to Personalizada
        app.voice_id_entry.delete(0, "end")
        app.voice_id_entry.insert(0, "my_custom_unique_voice_id")
        app._on_voice_id_entry_changed()
        assert app.voice_combo.get() == "Personalizada (Custom ID)"

        # Typing official Jessica ID sets dropdown to Jessica (Oficial)
        app.voice_id_entry.delete(0, "end")
        app.voice_id_entry.insert(0, PRESET_VOICES["Jessica (Oficial)"])
        app._on_voice_id_entry_changed()
        assert app.voice_combo.get() == "Jessica (Oficial)"

        app.destroy()
    except Exception as exc:
        if "no display name" in str(exc).lower():
            pytest.skip("Skipping GUI test because DISPLAY is not set.")
        raise


def test_gui_service_status_badge_semantic_colors(dummy_service: ConfigService) -> None:
    try:
        app = ViernesControlCenter(config_service=dummy_service)
        app.withdraw()

        # Active
        app._apply_service_status(True, "active")
        assert "Activo" in app.status_badge.cget("text")
        assert app.status_badge.cget("text_color") == "#4ade80"

        # Failed
        app._apply_service_status(False, "failed")
        assert "Fallido" in app.status_badge.cget("text")
        assert app.status_badge.cget("text_color") == "#f87171"

        # No disponible
        app._apply_service_status(False, "no disponible")
        assert "No Disponible" in app.status_badge.cget("text")

        # Inactive
        app._apply_service_status(False, "inactive")
        assert "Inactivo" in app.status_badge.cget("text")

        app.destroy()
    except Exception as exc:
        if "no display name" in str(exc).lower():
            pytest.skip("Skipping GUI test because DISPLAY is not set.")
        raise


def test_gui_save_triggers_service_restart_when_active(dummy_service: ConfigService) -> None:
    try:
        app = ViernesControlCenter(config_service=dummy_service)
        app.withdraw()

        with patch.object(dummy_service, "get_service_status", return_value=(True, "active")), \
             patch.object(dummy_service, "control_service", return_value=(True, "reloaded")) as mock_ctrl, \
             patch("threading.Thread") as mock_thread:

            # Execute thread inline for testing
            mock_thread.side_effect = lambda target, daemon: MagicMock(start=lambda: target())

            app._on_save_clicked()
            mock_ctrl.assert_called_with("try-restart")

        app.destroy()
    except Exception as exc:
        if "no display name" in str(exc).lower():
            pytest.skip("Skipping GUI test because DISPLAY is not set.")
        raise


def test_gui_service_action_buttons_disabled_during_execution(dummy_service: ConfigService) -> None:
    try:
        app = ViernesControlCenter(config_service=dummy_service)
        app.withdraw()

        # Trigger service action
        with patch.object(dummy_service, "control_service", return_value=(True, "ok")), \
             patch("threading.Thread") as mock_thread:

            # Keep thread suspended to test disabled state
            mock_thread.side_effect = lambda target, daemon: MagicMock(start=lambda: None)
            app._trigger_service_action("restart")

            assert app.btn_start.cget("state") == "disabled"
            assert app.btn_restart.cget("state") == "disabled"
            assert app.btn_stop.cget("state") == "disabled"
            assert "Ejecutando restart..." in app.service_feedback_label.cget("text")

        app.destroy()
    except Exception as exc:
        if "no display name" in str(exc).lower():
            pytest.skip("Skipping GUI test because DISPLAY is not set.")
        raise


def test_gui_safe_after_suppression(dummy_service: ConfigService) -> None:
    try:
        app = ViernesControlCenter(config_service=dummy_service)
        app.withdraw()

        called = False

        def callback():
            nonlocal called
            called = True

        app.destroy()
        # safe_after should silently return without raising when destroyed
        app.safe_after(0, callback)
        assert called is False
    except Exception as exc:
        if "no display name" in str(exc).lower():
            pytest.skip("Skipping GUI test because DISPLAY is not set.")
        raise
