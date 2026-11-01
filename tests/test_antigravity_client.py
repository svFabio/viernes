import pytest

from viernes.brain.agent import Agent
from viernes.brain.client import AntigravityClient


def test_antigravity_client_implements_agent_interface():
    client = AntigravityClient()
    assert isinstance(client, Agent)


def test_antigravity_client_ask_with_custom_invoker():
    def dummy_invoker(prompt: str, timeout: float) -> str:
        return f"Echoed from invoker: {prompt} (timeout={timeout})"

    client = AntigravityClient(custom_invoker=dummy_invoker, timeout=5.0)
    result = client.ask("Hola Antigravity")
    assert result == "Echoed from invoker: Hola Antigravity (timeout=5.0)"


@pytest.mark.anyio
async def test_antigravity_client_ask_stream_with_custom_streamer():
    async def dummy_streamer(prompt: str, timeout: float):
        for part in ["Parte 1 ", "Parte 2 ", prompt]:
            yield part

    client = AntigravityClient(custom_streamer=dummy_streamer)
    chunks = [chunk async for chunk in client.ask_stream("test")]
    assert "".join(chunks) == "Parte 1 Parte 2 test"


def test_antigravity_client_fallback_when_cli_not_in_path():
    # Provide nonexistent binary
    client = AntigravityClient(cli_bin="nonexistent_agy_binary_xyz")
    res = client.ask("Consulta")
    assert "simulated response for: Consulta" in res
