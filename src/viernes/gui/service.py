import logging
import os
import subprocess
from pathlib import Path
from typing import Any

import dotenv

from viernes.audio.devices import list_audio_devices

logger = logging.getLogger(__name__)

PRESET_VOICES: dict[str, str] = {
    "Jessica (Oficial)": "cgSgspJ2msm6clMCkdW9",
    "Lily (Cálida)": "pFZP5JQG7iQjIQuC4Bku",
    "Bella (Expresiva)": "EXAVITQu4vr4xnSDxMaL",
    "Laura (Conversacional)": "FGY2WhTYpPnrIDTdsKH5",
    "Alice (Directa)": "Xb7hH8MSUJfdDyGDuBmr",
    "Sarah (Suave)": "EXAVITQu4vr4xnSDxMaL",
}

DEFAULT_TEST_PHRASE = "A la orden, Señor. Viernes en línea y lista para operar."


class ConfigService:
    """Manages application configuration, hardware device enumeration, and background service controls."""

    def __init__(self, env_path: Path | None = None) -> None:
        self.env_path = env_path or self.resolve_env_path()

    @staticmethod
    def resolve_env_path() -> Path:
        """Finds .env file from project root or working directory."""
        candidates = [
            Path.cwd() / ".env",
            Path(__file__).resolve().parent.parent.parent.parent / ".env",
            Path.home() / ".config" / "viernes" / ".env",
        ]
        for candidate in candidates:
            if candidate.is_file():
                return candidate
        # Default to current directory .env if none found
        return Path.cwd() / ".env"

    def read_config(self) -> dict[str, str]:
        """Reads configuration key-values from the target .env file."""
        if not self.env_path.exists():
            return {}
        return dotenv.dotenv_values(str(self.env_path))  # type: ignore[return-value]

    def save_config(self, updates: dict[str, str]) -> None:
        """Updates or appends configuration keys in the .env file cleanly."""
        if not self.env_path.exists():
            self.env_path.parent.mkdir(parents=True, exist_ok=True)
            self.env_path.touch()

        for key, value in updates.items():
            dotenv.set_key(str(self.env_path), key, str(value).strip(), quote_mode="never")

    def get_audio_input_devices(self) -> list[tuple[str, str]]:
        """Returns input devices formatted as (display_name, raw_id)."""
        devices: list[tuple[str, str]] = [("Por defecto del sistema", "")]
        for dev in list_audio_devices():
            if dev.get("max_input_channels", 0) > 0:
                idx = dev.get("index", 0)
                name = dev.get("name", f"Microphone {idx}")
                display = f"[{idx}] {name}"
                devices.append((display, str(idx)))
        return devices

    def get_audio_output_devices(self) -> list[tuple[str, str]]:
        """Returns output devices formatted as (display_name, raw_id)."""
        devices: list[tuple[str, str]] = [("Por defecto del sistema", "")]
        for dev in list_audio_devices():
            if dev.get("max_output_channels", 0) > 0:
                idx = dev.get("index", 0)
                name = dev.get("name", f"Speaker {idx}")
                display = f"[{idx}] {name}"
                devices.append((display, str(idx)))
        return devices

    def test_voice(
        self,
        api_key: str,
        voice_id: str,
        phrase: str = DEFAULT_TEST_PHRASE,
        temp_dir: Path | None = None,
    ) -> Path:
        """Generates a live voice audition file and plays it."""
        from viernes.audio.player import AudioPlayer
        from viernes.tts.elevenlabs_provider import ElevenLabsProvider
        from viernes.tts.key_pool import KeyPoolElevenLabsProvider

        keys = [k.strip() for k in api_key.replace("\n", ",").split(",") if k.strip()]
        if not keys:
            raise ValueError("La API key de ElevenLabs no puede estar vacía.")

        vid = voice_id.strip() or PRESET_VOICES["Jessica (Oficial)"]
        work_dir = temp_dir or Path(os.getenv("VIERNES_TEMP_DIR", "/tmp/viernes"))
        work_dir.mkdir(parents=True, exist_ok=True)
        out_path = work_dir / "audition_test.mp3"

        provider = (
            KeyPoolElevenLabsProvider(api_keys=keys, voice_id=vid, temp_dir=work_dir)
            if len(keys) > 1
            else ElevenLabsProvider(api_key=keys[0], voice_id=vid, temp_dir=work_dir)
        )
        synthesized = provider.synthesize(phrase, out_path)

        player = AudioPlayer()
        player.play(synthesized)
        return synthesized

    def get_service_status(self, service_name: str = "viernes.service") -> tuple[bool, str]:
        """Checks systemd user service status."""
        try:
            res = subprocess.run(
                ["systemctl", "--user", "is-active", service_name],
                capture_output=True,
                check=False,
                text=True,
                timeout=3,
            )
            status = res.stdout.strip()
            is_active = status == "active"
            return is_active, status or "inactive"
        except (subprocess.SubprocessError, OSError) as exc:
            logger.debug("Failed checking systemd service: %s", exc)
            return False, "no disponible"

    def control_service(
        self, action: str, service_name: str = "viernes.service"
    ) -> tuple[bool, str]:
        """Controls systemd user service (start, stop, restart)."""
        if action not in ("start", "stop", "restart", "reload"):
            return False, f"Acción inválida: {action}"
        try:
            res = subprocess.run(
                ["systemctl", "--user", action, service_name],
                capture_output=True,
                check=False,
                text=True,
                timeout=5,
            )
            if res.returncode == 0:
                return True, f"Servicio {action} completado exitosamente"
            return False, res.stderr.strip() or f"Error al ejecutar {action}"
        except (subprocess.SubprocessError, OSError) as exc:
            return False, str(exc)
