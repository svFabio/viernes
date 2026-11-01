import asyncio
import logging
from collections.abc import Awaitable, Callable
from dataclasses import dataclass
from enum import Enum

from pydantic import BaseModel

logger = logging.getLogger(__name__)

DEFAULT_CONFIRMATION_TIMEOUT_SECONDS = 8.0

AFFIRMATIVE_TERMS = {
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

NEGATIVE_TERMS = {
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


class ConfirmationDecision(str, Enum):
    APPROVED = "APPROVED"
    REJECTED = "REJECTED"
    TIMEOUT = "TIMEOUT"


@dataclass
class PendingAction:
    tool_name: str
    params: BaseModel
    impact_description: str
    spoken_question: str


class ConfirmationManager:
    """Manages explicit voice confirmation for destructive actions with 8s window."""

    def __init__(self, timeout_seconds: float = DEFAULT_CONFIRMATION_TIMEOUT_SECONDS) -> None:
        self.timeout_seconds = timeout_seconds
        self.pending_action: PendingAction | None = None

    def formulate_question(self, impact_description: str) -> str:
        """Formulates a natural spoken question detailing the exact impact."""
        return f"¿Confirmas que deseas {impact_description}? Di 'Sí' para proceder o 'Cancelar'."

    def prepare_request(
        self, tool_name: str, params: BaseModel, impact_description: str
    ) -> PendingAction:
        question = self.formulate_question(impact_description)
        action = PendingAction(
            tool_name=tool_name,
            params=params,
            impact_description=impact_description,
            spoken_question=question,
        )
        self.pending_action = action
        return action

    def evaluate_response(self, text: str) -> ConfirmationDecision:
        cleaned = text.strip().lower()
        words = set(cleaned.split())

        if any(term in words or term in cleaned for term in AFFIRMATIVE_TERMS):
            return ConfirmationDecision.APPROVED
        if any(term in words or term in cleaned for term in NEGATIVE_TERMS):
            return ConfirmationDecision.REJECTED

        return ConfirmationDecision.REJECTED

    async def wait_for_voice_confirmation(
        self,
        action: PendingAction,
        voice_listener: Callable[[], Awaitable[str]],
    ) -> tuple[ConfirmationDecision, str]:
        """Listens for user confirmation within the strict time window."""
        try:
            async with asyncio.timeout(self.timeout_seconds):
                user_spoken = await voice_listener()

            decision = self.evaluate_response(user_spoken)
            if decision == ConfirmationDecision.APPROVED:
                logger.info("Voice action '%s' confirmed by user.", action.tool_name)
                return decision, "Confirmación recibida. Procediendo con la acción."
            else:
                logger.info("Voice action '%s' cancelled by user response: '%s'", action.tool_name, user_spoken)
                return ConfirmationDecision.REJECTED, "Acción cancelada."

        except TimeoutError:
            logger.warning(
                "Confirmation window (%.1fs) expired for '%s'. Discarding action.",
                self.timeout_seconds,
                action.tool_name,
            )
            return ConfirmationDecision.TIMEOUT, "Tiempo de espera agotado. Acción cancelada por seguridad."
        finally:
            # Guarantee no cumulative lingering state
            self.pending_action = None
