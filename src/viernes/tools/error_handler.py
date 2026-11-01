import logging
import traceback
from collections.abc import Callable
from enum import Enum
from typing import Any

logger = logging.getLogger("viernes.tools.errors")


class AssistantState(str, Enum):
    IDLE = "IDLE"
    LISTENING = "LISTENING"
    THINKING = "THINKING"
    SPEAKING = "SPEAKING"
    CONFIRMING = "CONFIRMING"


def format_spoken_error(exc: BaseException, tool_name: str | None = None) -> str:
    """Translates technical exceptions into concise, natural spoken messages without technical jargon."""
    err_str = str(exc).lower()

    if isinstance(exc, (TimeoutError, ConnectionError)) or "timeout" in err_str or "connection" in err_str:
        return "No pude completar la solicitud porque el servicio no responde. Intenta de nuevo más tarde."
    if isinstance(exc, PermissionError) or "permission" in err_str or "allowlist" in err_str:
        return "No tengo autorización de seguridad para realizar esa acción."
    if isinstance(exc, FileNotFoundError) or "not found" in err_str or "inexistente" in err_str:
        return "No pude encontrar el archivo o recurso solicitado."
    if isinstance(exc, (ValueError, TypeError)) or "validation" in err_str:
        return "Los parámetros indicados para la acción no son correctos."

    if tool_name:
        return f"Ocurrió un error al ejecutar la herramienta {tool_name}."
    return "Ocurrió un inconveniente al procesar tu solicitud."


class ToolExceptionMiddleware:
    """Global exception capture middleware for tools ensuring logging, spoken notice and clean IDLE return."""

    def __init__(
        self,
        tts_speaker: Callable[[str], None] | None = None,
        on_state_change: Callable[[AssistantState], None] | None = None,
    ) -> None:
        self.tts_speaker = tts_speaker
        self.on_state_change = on_state_change
        self.current_state = AssistantState.IDLE

    def set_state(self, new_state: AssistantState) -> None:
        self.current_state = new_state
        if self.on_state_change:
            self.on_state_change(new_state)

    def execute_safely(
        self,
        tool_func: Callable[[], Any],
        tool_name: str | None = None,
    ) -> tuple[bool, Any, str | None]:
        """Executes a tool call catching all exceptions, logging diagnostic trace, and transitioning to IDLE."""
        try:
            result = tool_func()
            return True, result, None
        except Exception as exc:  # noqa: BLE001
            # 1. Log detailed technical stack trace for journalctl diagnostics
            logger.error(
                "Execution failure in tool '%s': %s\nStack trace:\n%s",
                tool_name or "unknown",
                exc,
                traceback.format_exc(),
            )

            # 2. Translate technical error to natural spoken message
            spoken_message = format_spoken_error(exc, tool_name)

            # 3. Synthesize and speak error message if speaker provided
            if self.tts_speaker:
                try:
                    self.set_state(AssistantState.SPEAKING)
                    self.tts_speaker(spoken_message)
                except Exception as tts_err:  # noqa: BLE001
                    logger.error("Failed to speak error message: %s", tts_err)

            # 4. Return cleanly to IDLE state
            self.set_state(AssistantState.IDLE)
            return False, None, spoken_message
