import asyncio
import time
from collections.abc import AsyncIterator

import pytest

from viernes.brain.agent import Agent, EchoAgent, MockAgent


def _collect_stream(agent: Agent, prompt: str) -> list[str]:
    """Helper to collect async stream chunks in sync pytest test functions."""

    async def _runner() -> list[str]:
        chunks: list[str] = []
        async for chunk in agent.ask_stream(prompt):
            chunks.append(chunk)
        return chunks

    return asyncio.run(_runner())


def test_agent_interface_is_abstract():
    """Agent cannot be instantiated directly and requires both ask and ask_stream."""
    with pytest.raises(TypeError):
        Agent()  # type: ignore[abstract]

    class MissingStream(Agent):
        def ask(self, prompt: str) -> str:
            return prompt

    with pytest.raises(TypeError):
        MissingStream()  # type: ignore[abstract]

    class MissingAsk(Agent):
        async def ask_stream(self, prompt: str) -> AsyncIterator[str]:
            yield prompt

    with pytest.raises(TypeError):
        MissingAsk()  # type: ignore[abstract]

    class CompleteAgent(Agent):
        def ask(self, prompt: str) -> str:
            return prompt

        async def ask_stream(self, prompt: str) -> AsyncIterator[str]:
            yield prompt

    agent = CompleteAgent()
    assert agent.ask("ping") == "ping"
    assert _collect_stream(agent, "ping") == ["ping"]


def test_echo_agent_ask_default_prefix():
    """EchoAgent prepends the default prefix in synchronous ask."""
    echo = EchoAgent()
    assert echo.ask("Hola mundo") == "Viernes received: Hola mundo"


def test_echo_agent_ask_custom_prefix():
    """EchoAgent accepts and applies a custom prefix."""
    echo = EchoAgent(prefix="Eco: ")
    assert echo.ask("probando") == "Eco: probando"


def test_echo_agent_ask_stream():
    """EchoAgent yields prefix and prompt as separate chunks."""
    echo = EchoAgent()
    chunks = _collect_stream(echo, "Hola mundo")
    assert chunks == ["Viernes received: ", "Hola mundo"]
    assert "".join(chunks) == echo.ask("Hola mundo")


def test_echo_agent_ask_stream_empty_prompt():
    """EchoAgent handles empty prompt in streaming mode gracefully."""
    echo = EchoAgent()
    chunks = _collect_stream(echo, "")
    assert chunks == ["Viernes received: "]
    assert "".join(chunks) == echo.ask("")


def test_mock_agent_default_response_and_tracking():
    """MockAgent returns default response and tracks call counts and history."""
    mock = MockAgent()
    assert mock.call_count == 0
    assert mock.history == []

    # Sync ask
    response = mock.ask("Hola")
    assert response == "Mock response"
    assert mock.call_count == 1
    assert mock.history == ["Hola"]

    # Stream ask
    chunks = _collect_stream(mock, "¿Qué hora es?")
    assert chunks == ["Mock response"]
    assert mock.call_count == 2
    assert mock.history == ["Hola", "¿Qué hora es?"]

    # Reset
    mock.reset()
    assert mock.call_count == 0
    assert mock.history == []


def test_mock_agent_dict_responses():
    """MockAgent looks up responses by prompt key or falls back to default."""
    mock = MockAgent(
        default_response="No entiendo",
        responses={
            "saludo": "Hola, ¿en qué te puedo ayudar?",
            "despedida": "Hasta luego",
        },
    )

    assert mock.ask("saludo") == "Hola, ¿en qué te puedo ayudar?"
    assert mock.ask("despedida") == "Hasta luego"
    assert mock.ask("desconocido") == "No entiendo"

    stream_chunks = _collect_stream(mock, "saludo")
    assert stream_chunks == ["Hola, ¿en qué te puedo ayudar?"]


def test_mock_agent_list_responses():
    """MockAgent cycles sequentially through a list of responses."""
    mock = MockAgent(responses=["Respuesta 1", "Respuesta 2"])

    assert mock.ask("pregunta A") == "Respuesta 1"
    assert mock.ask("pregunta B") == "Respuesta 2"
    assert mock.ask("pregunta C") == "Respuesta 1"
    assert mock.call_count == 3


def test_mock_agent_stream_chunks_list():
    """MockAgent yields predefined chunk list when configured."""
    mock = MockAgent(stream_chunks=["Hola ", "mundo", "!"])
    chunks = _collect_stream(mock, "cualquier prompt")
    assert chunks == ["Hola ", "mundo", "!"]
    assert mock.call_count == 1


def test_mock_agent_stream_chunks_dict():
    """MockAgent selects stream chunks based on prompt key."""
    mock = MockAgent(
        default_response="Default stream",
        stream_chunks={
            "temperatura": ["La temperatura ", "es de ", "22°C"],
            "hora": ["Son ", "las 4:00 PM"],
        },
    )

    assert _collect_stream(mock, "temperatura") == ["La temperatura ", "es de ", "22°C"]
    assert _collect_stream(mock, "hora") == ["Son ", "las 4:00 PM"]
    assert _collect_stream(mock, "otro") == ["Default stream"]
    assert mock.call_count == 3


def test_mock_agent_with_delay():
    """MockAgent pauses between stream chunks when delay is set."""
    mock = MockAgent(stream_chunks=["chunk1", "chunk2"], delay=0.02)
    start = time.perf_counter()
    chunks = _collect_stream(mock, "prompt")
    duration = time.perf_counter() - start

    assert chunks == ["chunk1", "chunk2"]
    assert duration >= 0.035


def test_mock_agent_exception_on_ask():
    """MockAgent raises configured exception on ask and records the call."""
    mock = MockAgent(exception=TimeoutError("Request timed out"))

    with pytest.raises(TimeoutError, match="Request timed out"):
        mock.ask("prompt_fallido")

    assert mock.call_count == 1
    assert mock.history == ["prompt_fallido"]


def test_mock_agent_exception_on_ask_stream():
    """MockAgent raises configured exception during ask_stream and records call."""
    mock = MockAgent(exception=RuntimeError("Stream connection broken"))

    with pytest.raises(RuntimeError, match="Stream connection broken"):
        _collect_stream(mock, "prompt_stream_fallido")

    assert mock.call_count == 1
    assert mock.history == ["prompt_stream_fallido"]
