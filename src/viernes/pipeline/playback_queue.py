import asyncio
import logging
from collections.abc import Callable
from pathlib import Path
from typing import Any

logger = logging.getLogger("viernes.pipeline.queue")


class SequentialPlaybackQueue:
    """Manages sequential audio playback queue preventing overlap and allowing clean cancellation."""

    def __init__(self, play_executor: Callable[[Path], Any]) -> None:
        self.play_executor = play_executor
        self._queue: asyncio.Queue[Path | None] = asyncio.Queue()
        self._worker_task: asyncio.Task[None] | None = None
        self._is_playing: bool = False
        self._current_audio: Path | None = None

    @property
    def is_playing(self) -> bool:
        return self._is_playing

    @property
    def current_audio(self) -> Path | None:
        return self._current_audio

    def start(self) -> None:
        if self._worker_task is None or self._worker_task.done():
            self._worker_task = asyncio.create_task(self._playback_worker())

    async def enqueue(self, audio_path: Path) -> None:
        """Pushes an audio chunk file to the playback queue."""
        self.start()
        await self._queue.put(audio_path)

    async def join(self) -> None:
        """Waits until all queued items have been played."""
        await self._queue.join()

    def clear(self) -> list[Path]:
        """Drains and discards all pending audio chunks in the queue without playing them."""
        discarded: list[Path] = []
        while not self._queue.empty():
            try:
                item = self._queue.get_nowait()
                if item is not None:
                    discarded.append(item)
                self._queue.task_done()
            except asyncio.QueueEmpty:
                break
        logger.info("Cleared %d pending audio items from playback queue.", len(discarded))
        return discarded

    async def stop(self) -> None:
        """Stops the playback worker."""
        self.clear()
        if self._worker_task:
            self._worker_task.cancel()
            try:
                await self._worker_task
            except asyncio.CancelledError:
                pass
            self._worker_task = None
        self._is_playing = False

    async def _playback_worker(self) -> None:
        while True:
            audio_path = await self._queue.get()
            if audio_path is None:
                self._queue.task_done()
                break

            self._is_playing = True
            self._current_audio = audio_path
            try:
                logger.debug("Starting playback of %s", audio_path)
                res = self.play_executor(audio_path)
                if asyncio.iscoroutine(res):
                    await res
            except asyncio.CancelledError:
                logger.info("Playback task was cancelled during %s", audio_path)
                raise
            except Exception as exc:  # noqa: BLE001
                logger.error("Error playing audio chunk %s: %s", audio_path, exc)
            finally:
                self._is_playing = False
                self._current_audio = None
                self._queue.task_done()
