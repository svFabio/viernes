import logging
import os
from pathlib import Path
from unittest.mock import patch

from viernes.config import Settings, load_settings, mask_secret


def test_mask_secret():
    assert mask_secret("") == "<not set>"
    assert mask_secret("short") == "***"
    assert mask_secret("sk_live_123456789") == "sk_***6789"


def test_settings_default_values():
    with patch.dict(os.environ, {}, clear=True):
        config = Settings()
        assert config.elevenlabs_api_key == ""
        assert config.elevenlabs_voice_id == "cgSgspJ2msm6clMCkdW9"
        assert config.piper_bin == "piper"
        assert config.piper_model_path == ""
        assert config.temp_dir == Path("/tmp/viernes")
        assert not config.is_elevenlabs_configured
        assert config.masked_elevenlabs_api_key == "<not set>"
        assert "<not set>" in repr(config)


def test_settings_masked_representation():
    config = Settings(elevenlabs_api_key="sk_production_secret_key_9999")
    assert config.is_elevenlabs_configured is True
    assert config.masked_elevenlabs_api_key == "sk_***9999"
    repr_str = repr(config)
    assert "sk_***9999" in repr_str
    assert "secret_key" not in repr_str


def test_load_settings_warns_when_api_key_missing(caplog):
    with (
        patch.dict(os.environ, {"ELEVENLABS_API_KEY": ""}, clear=True),
        caplog.at_level(logging.WARNING),
    ):
        loaded = load_settings()
        assert not loaded.is_elevenlabs_configured
        assert any("ELEVENLABS_API_KEY is not configured" in rec.message for rec in caplog.records)


def test_gitignore_contains_env():
    repo_root = Path(__file__).resolve().parent.parent.parent
    gitignore_file = repo_root / ".gitignore"
    assert gitignore_file.exists()
    lines = gitignore_file.read_text().splitlines()
    assert ".env" in lines
