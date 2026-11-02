import asyncio
from collections.abc import AsyncIterator
from pathlib import Path

import pytest

from viernes.brain.agent import Agent
from viernes.pipeline.playback_queue import SequentialPlaybackQueue
from viernes.pipeline.streaming_pipeline import StreamingPipeline
from viernes.pipeline.tokenizer import SentenceTokenizer


class MockTokenStreamAgent(Agent):
    def __init__(self, token_chunks: list[str]) -> None:
        self.chunks = token_chunks

    def ask(self, prompt: str) -> str:
        return "".join(self.chunks)

    async def ask_stream(self, prompt: str) -> AsyncIterator[str]:
        for chunk in self.chunks:
            await asyncio.sleep(0.01)
            yield chunk


def test_sentence_tokenizer_basic_and_abbreviations() -> None:
    tokenizer = SentenceTokenizer(min_words_per_sentence=1)

    tokens = [
        "Hola.",
        " Hoy",
        " es",
        " un",
        " buen",
        " día!",
        " El",
        " Dr.",
        " Smith",
        " vendrá",
        " pronto.",
    ]
    sentences: list[str] = []
    for tok in tokens:
        sentences.extend(tokenizer.process_token(tok))
    sentences.extend(tokenizer.flush())

    assert len(sentences) == 3
    assert sentences[0] == "Hola."
    assert sentences[1] == "Hoy es un buen día!"
    # Ensure Dr. did not cause an early sentence break
    assert sentences[2] == "El Dr. Smith vendrá pronto."


@pytest.mark.anyio
async def test_sequential_playback_queue_order(tmp_path: Path) -> None:
    played_items: list[Path] = []
    active_plays = 0
    max_concurrent = 0

    async def mock_play(path: Path) -> None:
        nonlocal active_plays, max_concurrent
        active_plays += 1
        max_concurrent = max(max_concurrent, active_plays)
        await asyncio.sleep(0.05)
        played_items.append(path)
        active_plays -= 1

    queue = SequentialPlaybackQueue(mock_play)
    p1 = tmp_path / "1.mp3"
    p2 = tmp_path / "2.mp3"
    p3 = tmp_path / "3.mp3"

    await queue.enqueue(p1)
    await queue.enqueue(p2)
    await queue.enqueue(p3)

    await queue.join()
    await queue.stop()

    assert played_items == [p1, p2, p3]
    assert max_concurrent == 1  # Guaranteed zero overlap


@pytest.mark.anyio
async def test_streaming_pipeline_execution(tmp_path: Path) -> None:
    tokens = [
        "Primera ",
        "oración ",
        "completa. ",
        "Segunda ",
        "oración ",
        "aquí.",
    ]
    agent = MockTokenStreamAgent(tokens)

    synthesized_sentences: list[str] = []

    def mock_synthesizer(text: str, out_path: Path) -> Path:
        synthesized_sentences.append(text)
        out_path.write_bytes(b"dummy_mp3_data")
        return out_path

    played_audio: list[Path] = []

    async def mock_player(audio_path: Path) -> None:
        await asyncio.sleep(0.02)
        played_audio.append(audio_path)

    queue = SequentialPlaybackQueue(mock_player)
    pipeline = StreamingPipeline(
        agent=agent,
        tts_synthesizer=mock_synthesizer,
        playback_queue=queue,
        temp_dir=tmp_path,
    )

    metrics = await pipeline.execute("Dame un informe")
    await queue.join()
    await queue.stop()

    assert metrics.sentences_count == 2
    assert len(synthesized_sentences) == 2
    assert "Primera oración completa." in synthesized_sentences[0]
    assert "Segunda oración aquí." in synthesized_sentences[1]
    assert len(played_audio) == 2
    assert metrics.first_audio_latency_seconds > 0
    assert metrics.first_audio_latency_seconds < 1.2
