import os
from pathlib import Path
from unittest.mock import MagicMock, patch

import pytest

from viernes.gui.app import ViernesControlCenter
from viernes.gui.service import PRESET_VOICES, ConfigService
from viernes.gui.theme import Colors, Radius, TypographyManager


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
    """Verifies that the control center loads stored configuration and behaves properly."""
    try:
        app = ViernesControlCenter(config_service=dummy_service)
        app.withdraw()  # keep hidden during tests

        # Verify initial values were loaded into controls
        assert app.api_key_entry.get() == "sk_test_key"
        assert app.voice_id_entry.get() == "cgSgspJ2msm6clMCkdW9"
        assert app.voice_combo.get() == "Jessica (Oficial)"
        assert abs(app.vad_slider.get() - 0.015) < 1e-4
        assert int(app.silence_slider.get()) == 900

        # Verify typography and design tokens are bound
        assert app.header_card.cget("corner_radius") == Radius.HEADER
        assert app.status_pill.cget("corner_radius") == Radius.PILL

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
        if "no display name" in str(exc).lower():
            pytest.skip("Skipping GUI test because DISPLAY is not set.")
        raise


def test_service_badge_status_transitions(dummy_service: ConfigService) -> None:
    """Verifies live service badge pill styling transitions between active and inactive."""
    try:
        app = ViernesControlCenter(config_service=dummy_service)
        app.withdraw()

        # Inactive simulation
        dummy_service.get_service_status = MagicMock(return_value=(False, "inactive"))  # type: ignore[method-assign]
        app._update_service_status_badge()
        assert "Inactivo" in app.status_badge.cget("text")
        assert app.status_badge.cget("text_color") == Colors.TEXT_MUTED

        # Active simulation
        dummy_service.get_service_status = MagicMock(return_value=(True, "active"))  # type: ignore[method-assign]
        app._update_service_status_badge()
        assert "Activo" in app.status_badge.cget("text")
        assert app.status_badge.cget("text_color") == Colors.SUCCESS_TEXT

        app.destroy()
    except Exception as exc:
        if "no display name" in str(exc).lower():
            pytest.skip("Skipping GUI test because DISPLAY is not set.")
        raise


def test_fillers_tab_reset_and_save(dummy_service: ConfigService, tmp_path: Path) -> None:
    """Verifies resetting and persisting filler phrases within the GUI."""
    try:
        app = ViernesControlCenter(config_service=dummy_service)
        app.withdraw()

        # Trigger reset to defaults
        app._on_reset_fillers_clicked()
        assert len(app.fillers_generic_txt.get("1.0", "end").strip()) > 0
        assert "restauradas" in app.fillers_status_label.cget("text")

        # Edit and save
        app.fillers_generic_txt.delete("1.0", "end")
        app.fillers_generic_txt.insert("1.0", "Entendido, Señor.\nEnseguida.")
        with patch("viernes.audio.fillers.save_custom_fillers") as mock_save:
            app._on_save_fillers_clicked()
            assert mock_save.called

        app.destroy()
    except Exception as exc:
        if "no display name" in str(exc).lower():
            pytest.skip("Skipping GUI test because DISPLAY is not set.")
        raise
