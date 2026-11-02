import datetime
import json
import logging
import os
import re
import tempfile
from pathlib import Path
from typing import Any

logger = logging.getLogger(__name__)

# Matches intent to reset, rotate, or start a new conversation/memory
_VERBS = (
    r"(abre|abras|abrí|abrir|inicia|inicies|iniciá|iniciar|"
    r"comienza|comiences|comenzá|comenzar|empieza|empezá|empezar|"
    r"reinicia|reinicies|reiniciá|reiniciar|olvida|olvides|olvidá|olvidar|"
    r"borra|borres|borrá|borrar|limpia|limpies|limpiá|limpiar)"
)
_TARGETS = r"(conversaci[oó]n|charla|memoria|contexto|sesi[oó]n)"
RESET_INTENT_PATTERN = re.compile(
    rf"\b{_VERBS}\b.*\b{_TARGETS}\b|\b(nueva|otra)\s+{_TARGETS}\b",
    re.IGNORECASE,
)


def is_reset_conversation_intent(prompt: str) -> bool:
    """Detects whether a user prompt requests starting a new conversation or clearing memory."""
    cleaned = prompt.strip()
    if not cleaned:
        return False
    return bool(RESET_INTENT_PATTERN.search(cleaned))


def get_default_session_file() -> Path:
    """Returns standard path to the Viernes session pointer file."""
    config_dir = Path.home() / ".config" / "viernes"
    override = os.getenv("VIERNES_SESSION_FILE")
    if override:
        return Path(override)
    return config_dir / "session.json"


class ConversationSessionManager:
    """Manages dynamic active conversation pointer and rotation history for Antigravity."""

    def __init__(self, state_file: Path | None = None) -> None:
        self.state_file = state_file or get_default_session_file()

    def _read_data(self) -> dict[str, Any]:
        if not self.state_file.exists():
            return {}
        try:
            with open(self.state_file, encoding="utf-8") as f:
                data = json.load(f)
                return data if isinstance(data, dict) else {}
        except (json.JSONDecodeError, OSError) as exc:
            logger.warning("Could not read session pointer from %s: %s", self.state_file, exc)
            return {}

    def _write_data(self, data: dict[str, Any]) -> None:
        self.state_file.parent.mkdir(parents=True, exist_ok=True)
        # Atomic write via temporary file
        with tempfile.NamedTemporaryFile(
            mode="w",
            dir=self.state_file.parent,
            encoding="utf-8",
            delete=False,
        ) as temp_file:
            temp_path = Path(temp_file.name)
            try:
                json.dump(data, temp_file, indent=2, ensure_ascii=False)
                temp_file.flush()
                os.fsync(temp_file.fileno())
            except Exception:
                if temp_path.exists():
                    temp_path.unlink()
                raise

        temp_path.replace(self.state_file)

    def get_current_conversation_id(self) -> str | None:
        """Retrieves active conversation ID if stored, else None."""
        data = self._read_data()
        val = data.get("active_conversation_id")
        return str(val).strip() if val else None

    def set_current_conversation_id(self, conversation_id: str) -> None:
        """Atomically stores active conversation ID."""
        cleaned_id = conversation_id.strip()
        if not cleaned_id:
            return

        data = self._read_data()
        current = data.get("active_conversation_id")
        if current == cleaned_id:
            return

        data["active_conversation_id"] = cleaned_id
        data["updated_at"] = datetime.datetime.now(datetime.UTC).isoformat()
        if "history" not in data or not isinstance(data["history"], list):
            data["history"] = []

        self._write_data(data)
        logger.info("Persisted active conversation ID: %s in %s", cleaned_id, self.state_file)

    def rotate_conversation(self) -> str | None:
        """Archives current conversation to history list and clears active pointer."""
        data = self._read_data()
        current = data.get("active_conversation_id")
        if not current:
            return None

        history = data.setdefault("history", [])
        if not isinstance(history, list):
            history = []
            data["history"] = history

        history.append(
            {
                "id": current,
                "archived_at": datetime.datetime.now(datetime.UTC).isoformat(),
            }
        )
        data["active_conversation_id"] = None
        data["updated_at"] = datetime.datetime.now(datetime.UTC).isoformat()

        self._write_data(data)
        logger.info("Rotated conversation. Archived ID: %s", current)
        return str(current)

    def get_history(self) -> list[dict[str, str]]:
        """Returns chronological list of archived conversation records."""
        data = self._read_data()
        raw_history = data.get("history", [])
        if not isinstance(raw_history, list):
            return []
        return [item for item in raw_history if isinstance(item, dict)]
