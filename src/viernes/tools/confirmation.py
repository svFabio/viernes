"""Voice confirmation formulation, 8s listening window, and decision evaluation for critical MCP actions."""

import asyncio
from collections.abc import Awaitable, Callable
from dataclasses import dataclass
from enum import Enum
import logging
from typing import Any

from pydantic import BaseModel

logger = logging.getLogger("viernes.tools.confirmation")

DEFAULT_CONFIRMATION_TIMEOUT_SECONDS: float = 8.0

AFFIRMATIVE_TERMS: frozenset[str] = frozenset(
    {
        "sí",
        "si",
        "confirmo",
        "adelante",
        "procede",
        "dale",
        "de acuerdo",
        "claro",
        "seguro",
    }
)

NEGATIVE_TERMS: frozenset[str] = frozenset(
    {
        "no",
        "cancela",
        "cancelar",
        "espera",
        "pará",
        "para",
        "detén",
        "detener",
        "abortar",
    }
)


class ConfirmationDecision(str, Enum):
    """Possible outcomes of a voice confirmation query."""

    APPROVED = "APPROVED"
    REJECTED = "REJECTED"
    TIMEOUT = "TIMEOUT"


@dataclass
class PendingAction:
    """Represents a destructive action waiting for explicit voice confirmation."""

    tool_name: str
    params: BaseModel | dict[str, Any]
    impact_description: str
    spoken_question: str


def formulate_impact_description(
    tool_name: str,
    params: BaseModel | dict[str, Any] | None = None,
) -> str:
    """Formulates a natural spoken Spanish description of the exact impact of an action."""
    params_dict: dict[str, Any] = {}
    if isinstance(params, BaseModel):
        params_dict = params.model_dump()
    elif isinstance(params, dict):
        params_dict = params

    if tool_name == "system_delete_file":
        path = params_dict.get("file_path", "el archivo indicado")
        return f"eliminar permanentemente el archivo '{path}'"

    if tool_name == "ticktick_delete_task":
        task_id = params_dict.get("task_id", params_dict.get("title", "la tarea indicada"))
        return f"eliminar la tarea '{task_id}' de TickTick"

    if tool_name in {"system_reboot", "reboot"}:
        return "reiniciar el sistema operativo"

    if tool_name in {"system_poweroff", "shutdown", "poweroff"}:
        return "apagar el equipo por completo"

    if tool_name == "system_kill_process":
        proc = params_dict.get("process_name", params_dict.get("pid", "el proceso indicado"))
        return f"finalizar forzosamente el proceso '{proc}'"

    return f"ejecutar la acción de riesgo '{tool_name}'"


def formulate_confirmation_question(impact_description: str) -> str:
    """Formulates a concise spoken confirmation question describing the exact impact."""
    clean_impact = impact_description.strip()
    if clean_impact.startswith("¿"):
        return clean_impact
    return f"¿Confirmas que deseas {clean_impact}? Di 'Sí' para proceder o 'Cancelar'."


class ConfirmationQuestionBuilder:
    """Builds spoken confirmation requests with clear impact descriptions."""

    @classmethod
    def build_pending_action(
        cls,
        tool_name: str,
        params: BaseModel | dict[str, Any] | None = None,
        custom_impact: str | None = None,
    ) -> PendingAction:
        """Constructs a PendingAction with spoken question and impact description."""
        impact = custom_impact or formulate_impact_description(tool_name, params)
        question = formulate_confirmation_question(impact)
        return PendingAction(
            tool_name=tool_name,
            params=params or {},
            impact_description=impact,
            spoken_question=question,
        )


class ConfirmationManager:
    """Manages explicit voice confirmation for destructive actions within an 8-second window."""

    def __init__(self, timeout_seconds: float = DEFAULT_CONFIRMATION_TIMEOUT_SECONDS) -> None:
        self.timeout_seconds = timeout_seconds
        self.pending_action: PendingAction | None = None

    def evaluate_response(self, text: str) -> ConfirmationDecision:
        """Evaluates whether spoken text contains an accepted affirmative or negative term."""
        cleaned = text.strip().lower()
        words = set(cleaned.split())

        if any(term in words or term in cleaned for term in AFFIRMATIVE_TERMS):
            return ConfirmationDecision.APPROVED
        if any(term in words or term in cleaned for term in NEGATIVE_TERMS):
            return ConfirmationDecision.REJECTED

        return ConfirmationDecision.REJECTED

    def is_affirmative(self, text: str) -> bool:
        """Returns True if the text evaluates to approved."""
        return self.evaluate_response(text) == ConfirmationDecision.APPROVED

    def is_negative(self, text: str) -> bool:
        """Returns True if the text evaluates to rejected."""
        return self.evaluate_response(text) == ConfirmationDecision.REJECTED

    async def wait_for_voice_confirmation(
        self,
        action: PendingAction,
        voice_listener: Callable[[], Awaitable[str]],
    ) -> tuple[ConfirmationDecision, str]:
        """Listens for user confirmation within the strict time window (default 8s)."""
        self.pending_action = action
        try:
            async with asyncio.timeout(self.timeout_seconds):
                user_spoken = await voice_listener()

            decision = self.evaluate_response(user_spoken)
            if decision == ConfirmationDecision.APPROVED:
                logger.info("Voice action '%s' confirmed by user.", action.tool_name)
                return decision, "Confirmación recibida. Procediendo con la acción."
            else:
                logger.info(
                    "Voice action '%s' cancelled by user response: '%s'",
                    action.tool_name,
                    user_spoken,
                )
                return ConfirmationDecision.REJECTED, "Acción cancelada."

        except TimeoutError:
            logger.warning(
                "Confirmation window (%.1fs) expired for '%s'. Discarding action.",
                self.timeout_seconds,
                action.tool_name,
            )
            return (
                ConfirmationDecision.TIMEOUT,
                "Tiempo de espera agotado. Acción cancelada por seguridad.",
            )
        finally:
            # Guarantee no cumulative lingering state
            self.pending_action = None


__all__ = [
    "AFFIRMATIVE_TERMS",
    "DEFAULT_CONFIRMATION_TIMEOUT_SECONDS",
    "NEGATIVE_TERMS",
    "ConfirmationDecision",
    "ConfirmationManager",
    "ConfirmationQuestionBuilder",
    "PendingAction",
    "formulate_confirmation_question",
    "formulate_impact_description",
]
