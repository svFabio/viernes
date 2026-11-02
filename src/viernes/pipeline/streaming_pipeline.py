import asyncio
import logging
import time
from collections.abc import AsyncIterator, Callable
from dataclasses import dataclass
from pathlib import Path

from viernes.audio.fillers import FillerAudioCache
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
        filler_cache: FillerAudioCache | None = None,
    ) -> None:
        self.agent = agent
        self.tts_synthesizer = tts_synthesizer
        self.playback_queue = playback_queue
        self.temp_dir = temp_dir
        self.temp_dir.mkdir(parents=True, exist_ok=True)
        self.filler_cache = filler_cache
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
        first_token_received = False
        tool_filler_played = False

        logger.info("Starting streaming pipeline for prompt: '%s'", prompt)

        # Enqueue instant filler audio if available to eliminate perceived latency
        if self.filler_cache is not None:
            filler = self.filler_cache.get_random_filler()
            if filler and filler.is_file():
                logger.info("Instantly playing conversational filler to mask latency: %s", filler.name)
                first_audio_latency = time.perf_counter() - start_time
                await self.playback_queue.enqueue(filler)

        # Hook into agent tool activity to trigger tool-specific fillers (e.g. terminal / files)
        def handle_tool(tool_name: str) -> None:
            nonlocal tool_filler_played
            if not first_token_received and not tool_filler_played and self.filler_cache is not None:
                tool_filler = self.filler_cache.get_tool_filler(tool_name)
                if tool_filler and tool_filler.is_file():
                    logger.info(
                        "Enqueuing tool-aware filler audio for '%s': %s",
                        tool_name,
                        tool_filler.name,
                    )
                    tool_filler_played = True
                    asyncio.create_task(self.playback_queue.enqueue(tool_filler))

        self.agent.on_tool_activity = handle_tool

        # Patience reminder if processing takes longer (> 6.5s without tokens)
        patience_filler_played = False

        async def patience_watchdog() -> None:
            nonlocal patience_filler_played
            try:
                await asyncio.sleep(6.5)
                if (
                    not first_token_received
                    and not patience_filler_played
                    and not self._is_cancelled
                    and self.filler_cache is not None
                ):
                    patience = self.filler_cache.get_patience_filler()
                    if patience and patience.is_file():
                        logger.info("Enqueuing patience reminder filler: %s", patience.name)
                        patience_filler_played = True
                        await self.playback_queue.enqueue(patience)
            except asyncio.CancelledError:
                pass

        watchdog_task = asyncio.create_task(patience_watchdog())

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

        async def monitored_token_stream() -> AsyncIterator[str]:
            nonlocal first_token_received
            async for token in self.agent.ask_stream(prompt):
                first_token_received = True
                if not watchdog_task.done():
                    watchdog_task.cancel()
                yield token

        try:
            async for sentence in tokenizer.tokenize_stream(monitored_token_stream()):
                if self._is_cancelled:
                    break
                logger.debug("Dispatched sentence %d to TTS: '%s'", sentence_index + 1, sentence)
                await sentence_queue.put(sentence)

        finally:
            if not watchdog_task.done():
                watchdog_task.cancel()
            self.agent.on_tool_activity = None
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
