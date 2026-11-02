import logging
import signal
import subprocess
import time
from collections.abc import Callable
from enum import Enum
from typing import Any

from viernes.pipeline.playback_queue import SequentialPlaybackQueue
from viernes.pipeline.streaming_pipeline import StreamingPipeline

logger = logging.getLogger("viernes.pipeline.barge_in")

INTERRUPTION_LATENCY_TARGET_MS = 150.0


class SystemState(str, Enum):
    IDLE = "IDLE"
    SPEAKING = "SPEAKING"
    RECORDING = "RECORDING"
    THINKING = "THINKING"


class AcousticEchoFilter:
    """Filters speaker output reflection to prevent the assistant from interrupting itself."""

    def __init__(
        self, echo_margin_ratio: float = 1.3, min_voice_energy_threshold: float = 0.05
    ) -> None:
        self.echo_margin = echo_margin_ratio
        self.min_voice_threshold = min_voice_energy_threshold

    def is_user_interruption(
        self,
        mic_energy: float,
        speaker_energy: float = 0.0,
        vad_active: bool = True,
    ) -> bool:
        """Determines if microphone energy represents true user voice rather than acoustic speaker echo."""
        if not vad_active:
            return False

        if mic_energy < self.min_voice_threshold:
            return False

        # If speaker is playing, microphone energy must significantly exceed echo reference
        if speaker_energy > 0 and mic_energy <= (speaker_energy * self.echo_margin):
            logger.debug(
                "Acoustic echo filtered: mic=%.3f <= speaker=%.3f * margin",
                mic_energy,
                speaker_energy,
            )
            return False

        return True


class BargeInController:
    """Manages real-time audio playback interruption and state transition upon user voice detection."""

    def __init__(
        self,
        playback_queue: SequentialPlaybackQueue | None = None,
        streaming_pipeline: StreamingPipeline | None = None,
        active_process_supplier: Callable[[], subprocess.Popen[Any] | None] | None = None,
        on_state_transition: Callable[[SystemState], None] | None = None,
        echo_filter: AcousticEchoFilter | None = None,
    ) -> None:
        self.playback_queue = playback_queue
        self.streaming_pipeline = streaming_pipeline
        self.active_process_supplier = active_process_supplier
        self.on_state_transition = on_state_transition
        self.echo_filter = echo_filter or AcousticEchoFilter()
        self.mic_stream_active_during_playback: bool = True
        self.current_state: SystemState = SystemState.IDLE

    def stop_active_player(self) -> float:
        """Sends SIGKILL/SIGTERM to the active audio player process and measures stop latency in ms."""
        start = time.perf_counter()
        if self.active_process_supplier:
            proc = self.active_process_supplier()
            if proc and proc.poll() is None:
                try:
                    logger.info("Sending SIGKILL to audio player process (PID %d)", proc.pid)
                    proc.send_signal(signal.SIGKILL)
                    proc.wait(timeout=0.1)
                except (subprocess.TimeoutExpired, ProcessLookupError, OSError) as exc:
                    logger.warning("Error stopping player process: %s", exc)

        stop_duration_ms = (time.perf_counter() - start) * 1000
        return stop_duration_ms

    def handle_voice_detected(
        self,
        mic_energy: float,
        speaker_energy: float = 0.0,
        vad_active: bool = True,
    ) -> bool:
        """Called when microphone captures sound during audio playback."""
        if not self.echo_filter.is_user_interruption(mic_energy, speaker_energy, vad_active):
            return False

        start_time = time.perf_counter()
        logger.info("Barge-in triggered by detected user voice.")

        # 1. Immediate termination of audio player sub-process
        player_latency = self.stop_active_player()

        # 2. Flush pending audio synthesis & playback queue
        if self.playback_queue:
            self.playback_queue.clear()

        # 3. Cancel active streaming pipeline coroutines / LLM token generation
        if self.streaming_pipeline:
            self.streaming_pipeline.cancel()

        # 4. Immediate transition to RECORDING state
        self.current_state = SystemState.RECORDING
        if self.on_state_transition:
            self.on_state_transition(SystemState.RECORDING)

        total_latency_ms = (time.perf_counter() - start_time) * 1000
        logger.info(
            "Barge-in processed in %.2fms (player kill: %.2fms, target < %.1fms)",
            total_latency_ms,
            player_latency,
            INTERRUPTION_LATENCY_TARGET_MS,
        )
        return True

    def handle_user_speech_detected(self) -> None:
        """Explicit trigger indicating user started speaking, aborting current playback."""
        self.handle_voice_detected(mic_energy=1.0, speaker_energy=0.0, vad_active=True)
