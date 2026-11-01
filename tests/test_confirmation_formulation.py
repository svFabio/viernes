"""Tests for spoken question formulation and impact description generation."""

from viernes.tools.confirmation import (
    ConfirmationQuestionBuilder,
    formulate_confirmation_question,
    formulate_impact_description,
)
from viernes.tools.schemas import FileDeleteParams, TickTickDeleteParams


def test_formulate_impact_description_for_file_delete() -> None:
    params = FileDeleteParams(file_path="/home/user/downloads/archive.zip")
    impact = formulate_impact_description("system_delete_file", params)
    assert impact == "eliminar permanentemente el archivo '/home/user/downloads/archive.zip'"

    question = formulate_confirmation_question(impact)
    assert question == (
        "¿Confirmas que deseas eliminar permanentemente el archivo '/home/user/downloads/archive.zip'? "
        "Di 'Sí' para proceder o 'Cancelar'."
    )


def test_formulate_impact_description_for_ticktick_delete() -> None:
    params = TickTickDeleteParams(task_id="task_999")
    impact = formulate_impact_description("ticktick_delete_task", params)
    assert impact == "eliminar la tarea 'task_999' de TickTick"

    question = formulate_confirmation_question(impact)
    assert question == (
        "¿Confirmas que deseas eliminar la tarea 'task_999' de TickTick? "
        "Di 'Sí' para proceder o 'Cancelar'."
    )


def test_formulate_impact_description_for_system_reboot_and_poweroff() -> None:
    impact_reboot = formulate_impact_description("system_reboot")
    assert impact_reboot == "reiniciar el sistema operativo"

    impact_poweroff = formulate_impact_description("system_poweroff")
    assert impact_poweroff == "apagar el equipo por completo"


def test_confirmation_question_builder_builds_pending_action() -> None:
    params = FileDeleteParams(file_path="/tmp/important.db")
    action = ConfirmationQuestionBuilder.build_pending_action(
        tool_name="system_delete_file",
        params=params,
    )
    assert action.tool_name == "system_delete_file"
    assert "eliminar permanentemente el archivo '/tmp/important.db'" in action.impact_description
    assert action.spoken_question.startswith("¿Confirmas que deseas")
    assert action.spoken_question.endswith("Di 'Sí' para proceder o 'Cancelar'.")
