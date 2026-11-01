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
