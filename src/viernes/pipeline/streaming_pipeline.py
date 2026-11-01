import asyncio
import logging
import time
from collections.abc import AsyncIterator, Callable
from dataclasses import dataclass
from pathlib import Path

from viernes.brain.agent import Agent
from viernes.pipeline.playback_queue import SequentialPlaybackQueue
from viernes.pipeline.tokenizer import SentenceTokenizer

logger = logging.getLogger("viernes.pipeline.streaming")

TARGET_LATENCY_THRESHOLD_SECONDS = 1.2


@dataclass
class PipelineMetrics:
    sentences_count: int
    first_audio_latency_seconds: float
    total_pipeline_duration_seconds: float


class StreamingPipeline:
    """Coordinates sentence-based streaming: token stream -> tokenizer -> TTS worker -> playback queue."""

    def __init__(
        self,
        agent: Agent,
        tts_synthesizer: Callable[[str, Path], Path],
        playback_queue: SequentialPlaybackQueue,
        temp_dir: Path,
    ) -> None:
        self.agent = agent
        self.tts_synthesizer = tts_synthesizer
        self.playback_queue = playback_queue
        self.temp_dir = temp_dir
        self.temp_dir.mkdir(parents=True, exist_ok=True)
        self._is_cancelled: bool = False

    def cancel(self) -> None:
        """Flags the current pipeline execution for immediate interruption."""
        self._is_cancelled = True
        self.playback_queue.clear()

    async def execute(
        self,
        prompt: str,
        speech_end_timestamp: float | None = None,
    ) -> PipelineMetrics:
        self._is_cancelled = False
        start_time = speech_end_timestamp if speech_end_timestamp is not None else time.perf_counter()
        first_audio_latency: float | None = None
        sentence_index = 0
        tokenizer = SentenceTokenizer()

        logger.info("Starting streaming pipeline for prompt: '%s'", prompt)

        # Background synthesis worker that receives sentences from a queue
        sentence_queue: asyncio.Queue[str | None] = asyncio.Queue()

        async def synthesis_worker() -> None:
            nonlocal first_audio_latency, sentence_index
            while not self._is_cancelled:
                sentence = await sentence_queue.get()
                if sentence is None:
                    sentence_queue.task_done()
                    break

                sentence_index += 1
                chunk_path = self.temp_dir / f"stream_chunk_{int(time.time() * 1000)}_{sentence_index}.mp3"

                try:
                    # Run synthesis in executor if blocking
                    loop = asyncio.get_running_loop()
                    out_path = await loop.run_in_executor(
                        None, self.tts_synthesizer, sentence, chunk_path
                    )

                    if self._is_cancelled:
                        sentence_queue.task_done()
                        break

                    # Record initial latency on the first synthesized sentence audio
                    if first_audio_latency is None:
                        first_audio_latency = time.perf_counter() - start_time
                        logger.info(
                            "Initial voice latency: %.3f seconds (target < %.1fs)",
                            first_audio_latency,
                            TARGET_LATENCY_THRESHOLD_SECONDS,
                        )

                    await self.playback_queue.enqueue(out_path)
                except Exception as exc:
                    logger.error("TTS synthesis error for sentence '%s': %s", sentence, exc)
                finally:
                    sentence_queue.task_done()

        synthesis_task = asyncio.create_task(synthesis_worker())

        # Stream tokens from brain agent
        token_stream: AsyncIterator[str] = self.agent.ask_stream(prompt)
        try:
            async for sentence in tokenizer.tokenize_stream(token_stream):
                if self._is_cancelled:
                    break
                logger.debug("Dispatched sentence %d to TTS: '%s'", sentence_index + 1, sentence)
                await sentence_queue.put(sentence)

        finally:
            await sentence_queue.put(None)
            await synthesis_task

        total_duration = time.perf_counter() - start_time
        metrics = PipelineMetrics(
            sentences_count=sentence_index,
            first_audio_latency_seconds=first_audio_latency or total_duration,
            total_pipeline_duration_seconds=total_duration,
        )
        logger.info(
            "Streaming pipeline finished: %d sentences, first audio %.3fs, total %.3fs",
            metrics.sentences_count,
            metrics.first_audio_latency_seconds,
            metrics.total_pipeline_duration_seconds,
        )
        return metrics
