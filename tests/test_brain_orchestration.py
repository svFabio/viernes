import pytest

from viernes.brain.agent import MockAgent


def test_mock_agent_orchestration_history():
    agent = MockAgent(
        responses=["Respuesta 1", "Respuesta 2"],
    )
    r1 = agent.ask("Turno 1")
    r2 = agent.ask("Turno 2")

    assert r1 == "Respuesta 1"
    assert r2 == "Respuesta 2"
    assert agent.call_count == 2
    assert agent.history == ["Turno 1", "Turno 2"]

    agent.reset()
    assert agent.call_count == 0
    assert agent.history == []


@pytest.mark.anyio
async def test_mock_agent_streaming_orchestration():
    agent = MockAgent(stream_chunks={"test": ["token1 ", "token2 ", "token3"]})
    chunks = [chunk async for chunk in agent.ask_stream("test")]
    assert chunks == ["token1 ", "token2 ", "token3"]
    assert agent.call_count == 1
