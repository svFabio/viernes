import pytest

from viernes.brain.agent import EchoAgent
from viernes.brain.antigravity_client import (
    FALLBACK_DEGRADED_RESPONSE,
    AntigravityAgent,
)
from viernes.brain.memory import ConversationMemory
from viernes.brain.mock import MockAgent
from viernes.brain.prompt import VIERNES_SYSTEM_PROMPT


def test_echo_agent_ask_and_stream() -> None:
    agent = EchoAgent()
    assert agent.ask("Hola") == "Viernes received: Hola"


@pytest.mark.anyio
async def test_echo_agent_ask_stream() -> None:
    agent = EchoAgent()
    chunks = [chunk async for chunk in agent.ask_stream("Hola")]
    assert "".join(chunks) == "Viernes received: Hola"


def test_mock_agent_canned_responses_and_history() -> None:
    agent = MockAgent(responses=["Primera respuesta", "Segunda respuesta"])
    res1 = agent.ask("Pregunta 1")
    res2 = agent.ask("Pregunta 2")
    res3 = agent.ask("Pregunta 3")

    assert res1 == "Primera respuesta"
    assert res2 == "Segunda respuesta"
    assert res3 == "Respuesta de prueba de Viernes."
    assert agent.received_prompts == ["Pregunta 1", "Pregunta 2", "Pregunta 3"]


@pytest.mark.anyio
async def test_mock_agent_streaming() -> None:
    agent = MockAgent(default_response="Hola mundo")
    tokens = [tok async for tok in agent.ask_stream("Test")]
    assert "".join(tokens).strip() == "Hola mundo"


def test_conversation_memory_windowing_and_formatting() -> None:
    memory = ConversationMemory(max_turns=2)
    memory.add_turn("Turno 1", "Respuesta 1")
    memory.add_turn("Turno 2", "Respuesta 2")
    memory.add_turn("Turno 3", "Respuesta 3")

    messages = memory.get_messages()
    # 2 turns * 2 = 4 messages max
    assert len(messages) == 4
    assert messages[0].content == "Turno 2"
    assert messages[1].content == "Respuesta 2"
    assert messages[2].content == "Turno 3"
    assert messages[3].content == "Respuesta 3"

    context_prompt = memory.format_context_prompt("Turno 4")
    assert "Turno 2" in context_prompt
    assert "Turno 3" in context_prompt
    assert "Turno 4" in context_prompt


def test_antigravity_agent_with_custom_invoker() -> None:
    def fake_invoker(prompt: str, timeout: float) -> str:
        return f"Procesado: {prompt}"

    agent = AntigravityAgent(custom_invoker=fake_invoker)
    res = agent.ask("¿Qué hora es?")

    assert res == "Procesado: ¿Qué hora es?"
    assert len(agent.memory.get_messages()) == 2
    assert agent.system_prompt == VIERNES_SYSTEM_PROMPT


def test_antigravity_agent_timeout_graceful_degradation() -> None:
    def slow_invoker(prompt: str, timeout: float) -> str:
        raise TimeoutError("Simulated timeout")

    agent = AntigravityAgent(custom_invoker=slow_invoker, timeout=1.0)
    res = agent.ask("Consulta pesada")

    assert res == FALLBACK_DEGRADED_RESPONSE


@pytest.mark.anyio
async def test_antigravity_agent_stream_timeout() -> None:
    async def slow_streamer(prompt: str, timeout: float):
        import asyncio

        await asyncio.sleep(0.5)
        yield "chunk"

    agent = AntigravityAgent(custom_streamer=slow_streamer, timeout=0.1)
    chunks = [chunk async for chunk in agent.ask_stream("Consulta lenta")]

    assert FALLBACK_DEGRADED_RESPONSE in "".join(chunks)
