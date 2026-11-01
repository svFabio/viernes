import logging
import os
from pathlib import Path

from dotenv import load_dotenv
from pydantic import BaseModel, Field

load_dotenv()
logger = logging.getLogger(__name__)


def mask_secret(value: str) -> str:
    """Masks secret keys for safe logging and string representation."""
    if not value:
        return "<not set>"
    if len(value) <= 7:
        return "***"
    return f"{value[:3]}***{value[-4:]}"


class Settings(BaseModel):
    """Strongly typed application configuration."""

    elevenlabs_api_key: str = Field(default_factory=lambda: os.getenv("ELEVENLABS_API_KEY", ""))
    elevenlabs_voice_id: str = Field(
        default_factory=lambda: os.getenv("ELEVENLABS_VOICE_ID", "21m00Tcm4TlvDq8ikWAM")
    )
    piper_bin: str = Field(default_factory=lambda: os.getenv("PIPER_BIN", "piper"))
    piper_model_path: str = Field(default_factory=lambda: os.getenv("PIPER_MODEL_PATH", ""))
    temp_dir: Path = Field(
        default_factory=lambda: Path(os.getenv("VIERNES_TEMP_DIR", "/tmp/viernes"))
    )

    model_config = {"frozen": True, "arbitrary_types_allowed": True}

    @property
    def is_elevenlabs_configured(self) -> bool:
        return bool(self.elevenlabs_api_key.strip())

    @property
    def masked_elevenlabs_api_key(self) -> str:
        return mask_secret(self.elevenlabs_api_key)

    def __repr__(self) -> str:
        return (
            f"Settings("
            f"elevenlabs_api_key='{self.masked_elevenlabs_api_key}', "
            f"elevenlabs_voice_id='{self.elevenlabs_voice_id}', "
            f"piper_bin='{self.piper_bin}', "
            f"piper_model_path='{self.piper_model_path}', "
            f"temp_dir={self.temp_dir})"
        )


def load_settings() -> Settings:
    """Instantiates Settings and validates runtime prerequisites."""
    app_settings = Settings()
    if not app_settings.is_elevenlabs_configured:
        logger.warning(
            "ELEVENLABS_API_KEY is not configured. Viernes will operate using local Piper fallback as primary TTS."
        )
    try:
        app_settings.temp_dir.mkdir(parents=True, exist_ok=True, mode=0o700)
        os.chmod(app_settings.temp_dir, 0o700)
    except Exception as exc:
        logger.warning("Could not pre-create temp_dir %s: %s", app_settings.temp_dir, exc)
    return app_settings


settings = load_settings()
