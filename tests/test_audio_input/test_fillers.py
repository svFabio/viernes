import asyncio
from collections.abc import AsyncIterator
from pathlib import Path

import pytest

from viernes.audio.fillers import FillerAudioCache
from viernes.brain.agent import Agent
from viernes.brain.mock import MockAgent
from viernes.pipeline.playback_queue import SequentialPlaybackQueue
from viernes.pipeline.streaming_pipeline import StreamingPipeline


def test_filler_cache_get_filler_path(tmp_path: Path) -> None:
    cache = FillerAudioCache(cache_dir=tmp_path)
    path = cache.get_filler_path_for_phrase("Hola Señor")
    assert path.parent == tmp_path
    assert path.name.startswith("filler_")
    assert path.suffix == ".mp3"


def test_filler_cache_prewarm_and_get_random(tmp_path: Path) -> None:
    cache = FillerAudioCache(cache_dir=tmp_path, phrases=["Frase 1", "Frase 2"])

    def dummy_synth(text: str, out: Path) -> Path:
        out.write_bytes(b"dummy_audio_bytes")
        return out

    cache.prewarm(dummy_synth)
    assert len(cache._cached_files) == 2

    chosen = cache.get_random_filler()
    assert chosen is not None
    assert chosen.exists()
    assert chosen.name.startswith("filler_")


def test_filler_cache_tool_fillers(tmp_path: Path) -> None:
    cache = FillerAudioCache(
        cache_dir=tmp_path,
        tool_phrases={
            "files": ["Revisando archivos, Señor."],
            "terminal": ["Ejecutando en terminal, Señor."],
        },
    )

    def dummy_synth(text: str, out: Path) -> Path:
        out.write_bytes(b"dummy_tool_audio")
        return out

    cache.prewarm(dummy_synth)

    file_filler = cache.get_tool_filler("read_file")
    assert file_filler is not None
    assert file_filler.exists()

    term_filler = cache.get_tool_filler("run_command")
    assert term_filler is not None
    assert term_filler.exists()

    unknown = cache.get_tool_filler("non_existent_tool")
    assert unknown is None


@pytest.mark.anyio
async def test_streaming_pipeline_enqueues_filler_first(tmp_path: Path) -> None:
    cache = FillerAudioCache(cache_dir=tmp_path, phrases=["Deme un segundo, Señor."])
    filler_file = tmp_path / "filler_test.mp3"
    filler_file.write_bytes(b"filler_bytes")
    cache._cached_files = [filler_file]

    played_items: list[Path] = []
    playback_queue = SequentialPlaybackQueue(lambda p: played_items.append(p))

    agent = MockAgent(default_response="Esta es la respuesta real.")

    pipeline = StreamingPipeline(
        agent=agent,
        tts_synthesizer=lambda text, out: out.write_bytes(b"out") or out,
        playback_queue=playback_queue,
        temp_dir=tmp_path,
        filler_cache=cache,
    )

    metrics = await pipeline.execute("Hola Viernes")
    await playback_queue.join()
    await playback_queue.stop()

    assert len(played_items) >= 2
    # First played item MUST be the filler audio
    assert played_items[0] == filler_file
    assert metrics.first_audio_latency_seconds < 0.5


class ToolTriggeringMockAgent(Agent):
    def __init__(self) -> None:
        self.on_tool_activity = None

    def ask(self, prompt: str) -> str:
        return "Respuesta final con herramienta."

    async def ask_stream(self, prompt: str) -> AsyncIterator[str]:
        if self.on_tool_activity:
            self.on_tool_activity("read_file")
        await asyncio.sleep(0.05)
        yield "Respuesta "
        yield "después "
        yield "del archivo."


@pytest.mark.anyio
async def test_streaming_pipeline_enqueues_tool_filler(tmp_path: Path) -> None:
    cache = FillerAudioCache(
        cache_dir=tmp_path,
        phrases=["Instant filler."],
        tool_phrases={"files": ["Permítame leer los archivos, Señor."]},
    )
    def dummy_synth(text: str, out: Path) -> Path:
        out.write_bytes(b"audio")
        return out
    cache.prewarm(dummy_synth)

    played_items: list[Path] = []
    playback_queue = SequentialPlaybackQueue(lambda p: played_items.append(p))

    agent = ToolTriggeringMockAgent()

    pipeline = StreamingPipeline(
        agent=agent,
        tts_synthesizer=dummy_synth,
        playback_queue=playback_queue,
        temp_dir=tmp_path,
        filler_cache=cache,
    )

    metrics = await pipeline.execute("Lee un archivo")
    await playback_queue.join()
    await playback_queue.stop()

    assert metrics.sentences_count >= 1

    # Should contain: generic filler, tool filler, synthesized sentence(s)
    assert len(played_items) >= 2
    tool_filler_path = cache.get_tool_filler("read_file")
    assert tool_filler_path in played_items
