import json
import pytest
from pathlib import Path

from viernes.brain.antigravity_client import AntigravityAgent
from viernes.brain.session_manager import (
    ConversationSessionManager,
    is_reset_conversation_intent,
)


def test_session_manager_initial_state(tmp_path: Path):
    state_file = tmp_path / "session.json"
    manager = ConversationSessionManager(state_file=state_file)

    assert manager.get_current_conversation_id() is None
    assert manager.get_history() == []


def test_session_manager_set_and_get(tmp_path: Path):
    state_file = tmp_path / "session.json"
    manager = ConversationSessionManager(state_file=state_file)

    manager.set_current_conversation_id("conv-12345")
    assert manager.get_current_conversation_id() == "conv-12345"

    with open(state_file, encoding="utf-8") as f:
        data = json.load(f)
    assert data["active_conversation_id"] == "conv-12345"
    assert "updated_at" in data


def test_session_manager_rotation(tmp_path: Path):
    state_file = tmp_path / "session.json"
    manager = ConversationSessionManager(state_file=state_file)

    manager.set_current_conversation_id("conv-first")
    archived = manager.rotate_conversation()

    assert archived == "conv-first"
    assert manager.get_current_conversation_id() is None
    history = manager.get_history()
    assert len(history) == 1
    assert history[0]["id"] == "conv-first"
    assert "archived_at" in history[0]

    # Rotating when already None returns None
    assert manager.rotate_conversation() is None


@pytest.mark.parametrize(
    "prompt,expected",
    [
        ("abre otra conversación", True),
        ("viernes abre otra conversación", True),
        ("viernes, quiero que me abras otra conversación", True),
        ("inicia una nueva conversación", True),
        ("nueva conversación", True),
        ("otra conversación", True),
        ("reinicia tu memoria", True),
        ("reiniciá la conversación", True),
        ("olvida la conversación anterior", True),
        ("borra el contexto", True),
        ("limpia la sesión", True),
        ("hola viernes, cómo estás?", False),
        ("qué es una conversación?", False),
        ("reproduce música", False),
        ("cuál es el estado del sistema?", False),
    ],
)
def test_reset_conversation_intent_detection(prompt: str, expected: bool):
    assert is_reset_conversation_intent(prompt) is expected


def test_antigravity_agent_handles_reset_intent_synchronously(tmp_path: Path):
    state_file = tmp_path / "session.json"
    session_mgr = ConversationSessionManager(state_file=state_file)
    session_mgr.set_current_conversation_id("old-uuid")

    agent = AntigravityAgent(
        session_manager=session_mgr,
        custom_invoker=lambda p, t: "Debería no ser invocado",
    )

    response = agent.ask("viernes, por favor abre otra conversación")
    assert "Listo, Señor" in response
    assert session_mgr.get_current_conversation_id() is None
    assert len(session_mgr.get_history()) == 1


@pytest.mark.anyio
async def test_antigravity_agent_handles_reset_intent_streaming(tmp_path: Path):
    state_file = tmp_path / "session.json"
    session_mgr = ConversationSessionManager(state_file=state_file)
    session_mgr.set_current_conversation_id("old-uuid")

    agent = AntigravityAgent(
        session_manager=session_mgr,
    )

    chunks = []
    async for chunk in agent.ask_stream("inicia una nueva conversación"):
        chunks.append(chunk)

    full_resp = "".join(chunks)
    assert "Listo, Señor" in full_resp
    assert session_mgr.get_current_conversation_id() is None
    assert len(session_mgr.get_history()) == 1
