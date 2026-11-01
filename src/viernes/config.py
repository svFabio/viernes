import os
from dataclasses import dataclass
from pathlib import Path
from dotenv import load_dotenv

load_dotenv()


@dataclass(frozen=True)
class Settings:
    elevenlabs_api_key: str = os.getenv("ELEVENLABS_API_KEY", "")
    elevenlabs_voice_id: str = os.getenv("ELEVENLABS_VOICE_ID", "21m00Tcm4TlvDq8ikWAM")  # Rachel
    piper_bin: str = os.getenv("PIPER_BIN", "piper")
    piper_model_path: str = os.getenv("PIPER_MODEL_PATH", "")
    temp_dir: Path = Path(os.getenv("VIERNES_TEMP_DIR", "/tmp/viernes"))


settings = Settings()
