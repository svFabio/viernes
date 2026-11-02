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


def test_filler_cache_patience_fillers(tmp_path: Path) -> None:
    cache = FillerAudioCache(
        cache_dir=tmp_path,
        patience_phrases=["Señor, demoraré un momento en responder."],
    )

    def dummy_synth(text: str, out: Path) -> Path:
        out.write_bytes(b"dummy_patience_audio")
        return out

    cache.prewarm(dummy_synth)
    patience = cache.get_patience_filler()
    assert patience is not None
    assert patience.exists()


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


def test_load_and_save_custom_fillers(tmp_path: Path) -> None:
    config_file = tmp_path / "fillers.json"
    from viernes.audio.fillers import load_custom_fillers, save_custom_fillers

    # When file does not exist, returns defaults
    generic, tools, patience = load_custom_fillers(config_path=config_file)
    assert len(generic) > 0
    assert "files" in tools
    assert len(patience) > 0

    # Save custom phrases
    custom_generic = ["Frase personalizada uno", "Frase personalizada dos"]
    custom_tools = {"files": ["Mirando archivos"]}
    custom_patience = ["Espere un momento por favor"]

    save_custom_fillers(
        generic=custom_generic,
        tool_phrases=custom_tools,
        patience=custom_patience,
        config_path=config_file,
    )

    # Load again and verify
    loaded_gen, loaded_tools, loaded_pat = load_custom_fillers(config_path=config_file)
    assert loaded_gen == custom_generic
    assert loaded_tools == custom_tools
    assert loaded_pat == custom_patience


def test_filler_cache_loads_from_config(tmp_path: Path) -> None:
    config_file = tmp_path / "fillers.json"
    from viernes.audio.fillers import save_custom_fillers

    save_custom_fillers(
        generic=["Mi relleno único"],
        tool_phrases={"terminal": ["Terminal personalizada"]},
        patience=["Paciencia personalizada"],
        config_path=config_file,
    )

    cache = FillerAudioCache(cache_dir=tmp_path / "audio", config_path=config_file)
    assert cache.phrases == ["Mi relleno único"]
    assert cache.tool_phrases == {"terminal": ["Terminal personalizada"]}
    assert cache.patience_phrases == ["Paciencia personalizada"]
