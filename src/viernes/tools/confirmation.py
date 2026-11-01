"""Voice confirmation formulation and impact descriptions for critical MCP actions."""

from dataclasses import dataclass
from typing import Any

from pydantic import BaseModel

DEFAULT_CONFIRMATION_TIMEOUT_SECONDS = 8.0


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


__all__ = [
    "DEFAULT_CONFIRMATION_TIMEOUT_SECONDS",
    "ConfirmationQuestionBuilder",
    "PendingAction",
    "formulate_confirmation_question",
    "formulate_impact_description",
]
