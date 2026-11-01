import logging
import os
from pathlib import Path

from dotenv import load_dotenv
from pydantic import BaseModel, Field, model_validator

load_dotenv()
logger = logging.getLogger(__name__)


def mask_secret(value: str) -> str:
    """Masks secret keys for safe logging and string representation."""
    if not value:
        return "<not set>"
    if len(value) <= 7:
        return "***"
    return f"{value[:3]}***{value[-4:]}"


def parse_elevenlabs_keys() -> list[str]:
    """Extracts all configured ElevenLabs API keys from environment."""
    raw_pool = os.getenv("ELEVENLABS_API_KEYS", "")
    keys = [k.strip() for k in raw_pool.split(",") if k.strip()]
    single = os.getenv("ELEVENLABS_API_KEY", "").strip()
    if single and single not in keys:
        keys.insert(0, single)
    return keys


class Settings(BaseModel):
    """Strongly typed application configuration."""

    elevenlabs_api_key: str = Field(default_factory=lambda: os.getenv("ELEVENLABS_API_KEY", ""))
    elevenlabs_api_keys: list[str] = Field(default_factory=parse_elevenlabs_keys)
    elevenlabs_voice_id: str = Field(
        default_factory=lambda: os.getenv("ELEVENLABS_VOICE_ID", "cgSgspJ2msm6clMCkdW9")
    )
    piper_bin: str = Field(default_factory=lambda: os.getenv("PIPER_BIN", "piper"))
    piper_model_path: str = Field(default_factory=lambda: os.getenv("PIPER_MODEL_PATH", ""))
    temp_dir: Path = Field(
        default_factory=lambda: Path(os.getenv("VIERNES_TEMP_DIR", "/tmp/viernes"))
    )

    model_config = {"frozen": True, "arbitrary_types_allowed": True}

    @model_validator(mode="after")
    def _validate_keys(self) -> "Settings":
        if self.elevenlabs_api_key and self.elevenlabs_api_key not in self.elevenlabs_api_keys:
            object.__setattr__(
                self, "elevenlabs_api_keys", [self.elevenlabs_api_key] + self.elevenlabs_api_keys
            )
        return self

    @property
    def is_elevenlabs_configured(self) -> bool:
        return bool(self.elevenlabs_api_keys or self.elevenlabs_api_key.strip())

    @property
    def masked_elevenlabs_api_key(self) -> str:
        return mask_secret(self.elevenlabs_api_key)

    @property
    def masked_elevenlabs_api_keys(self) -> list[str]:
        return [mask_secret(k) for k in self.elevenlabs_api_keys]

    def __repr__(self) -> str:
        keys_preview = ", ".join(self.masked_elevenlabs_api_keys) or "<none>"
        return (
            f"Settings("
            f"elevenlabs_api_key='{self.masked_elevenlabs_api_key}', "
            f"elevenlabs_api_keys=[{keys_preview}], "
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
        app_settings.temp_dir.mkdir(parents=True, exist_ok=True)
    except Exception as exc:
        logger.warning("Could not pre-create temp_dir %s: %s", app_settings.temp_dir, exc)
    return app_settings


settings = load_settings()
