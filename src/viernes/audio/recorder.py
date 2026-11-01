import logging
import queue
import tempfile
import time
import uuid
import wave
from collections import deque
from collections.abc import Callable
from enum import Enum
from pathlib import Path
from typing import Self

import numpy as np
import sounddevice as sd

from viernes.audio.vad import VoiceActivityDetector, validate_vad_threshold

logger = logging.getLogger(__name__)


class RecorderState(str, Enum):
    IDLE = "IDLE"
    RECORDING = "RECORDING"
    PROCESSING = "PROCESSING"


class AudioRecorder:
    """Captures 16kHz mono 16-bit PCM audio with Voice Activity Detection (VAD).

    Implements automatic silence cutoff, short noise filtering, and 30s safety limits.
    """

    def __init__(
        self,
        sample_rate: int = 16000,
        channels: int = 1,
        dtype: str = "int16",
        frame_duration_ms: int = 30,
        vad_threshold: float = 0.5,
        silence_timeout_ms: int = 800,
        min_speech_duration_ms: int = 300,
        max_recording_duration_s: float = 30.0,
        device: int | str | None = None,
        temp_dir: Path | None = None,
        vad: VoiceActivityDetector | None = None,
    ) -> None:
        self.sample_rate = sample_rate
        self.channels = channels
        self.dtype = dtype
        self.frame_duration_ms = frame_duration_ms
        self.frame_size = int(self.sample_rate * (self.frame_duration_ms / 1000.0))

        # Thresholds
        self.vad_threshold = validate_vad_threshold(vad_threshold)
        self.silence_timeout_ms = self._validate_silence_timeout(silence_timeout_ms)
        self.min_speech_duration_ms = min_speech_duration_ms
        self.max_recording_duration_s = max_recording_duration_s
        self.device = device

        self.temp_dir = temp_dir or Path(tempfile.gettempdir()) / "viernes"
        self.temp_dir.mkdir(parents=True, exist_ok=True)

        self.vad = vad or VoiceActivityDetector(
            sample_rate=self.sample_rate,
            threshold=self.vad_threshold,
            frame_duration_ms=self.frame_duration_ms,
        )

        # State tracking
        self.state: RecorderState = RecorderState.IDLE
        self._pre_roll_frames: deque[np.ndarray] = deque(maxlen=5)  # ~150ms buffer
        self._recorded_frames: list[np.ndarray] = []
        self._consecutive_silence_ms: float = 0.0
        self._total_speech_ms: float = 0.0
        self._total_recorded_ms: float = 0.0

        # Hardware stream handles
        self._stream: sd.InputStream | None = None
        self._audio_queue: queue.Queue[np.ndarray] = queue.Queue()
        self._is_listening: bool = False

    @staticmethod
    def _validate_silence_timeout(timeout_ms: int) -> int:
        """Validates silence timeout between 300 and 3000 ms."""
        if not isinstance(timeout_ms, (int, float)):
            logger.warning("Invalid SILENCE_TIMEOUT_MS %s. Defaulting to 800.", timeout_ms)
            return 800
        val = int(timeout_ms)
        if val < 300 or val > 3000:
            logger.warning(
                "SILENCE_TIMEOUT_MS %s out of range [300, 3000]. Clamping to 800.",
                val,
            )
            return 800
        return val

    def reset(self) -> None:
        """Resets the internal recording buffer and state machine to IDLE."""
        self.state = RecorderState.IDLE
        self._pre_roll_frames.clear()
        self._recorded_frames.clear()
        self._consecutive_silence_ms = 0.0
        self._total_speech_ms = 0.0
        self._total_recorded_ms = 0.0
        self.vad.reset()

    def process_frame(self, frame: np.ndarray | bytes) -> Path | None:
        """Processes a single 20-30ms audio block.

        Returns:
            Path: Path to written .wav file when an utterance finishes and is valid.
            None: When still listening, recording, or when discarding short noises.
        """
        # Ensure frame is int16 numpy array
        if isinstance(frame, bytes):
            samples = np.frombuffer(frame, dtype=np.int16)
        elif isinstance(frame, np.ndarray):
            if frame.dtype == np.int16:
                samples = frame
            elif frame.dtype in (np.float32, np.float64):
                samples = (np.clip(frame, -1.0, 1.0) * 32767).astype(np.int16)
            else:
                samples = frame.astype(np.int16)
        else:
            raise TypeError(f"Unsupported frame type: {type(frame)}")

        is_voice = self.vad.is_speech(samples)

        if self.state == RecorderState.IDLE:
            if is_voice:
                # Transition to RECORDING: include pre-roll buffer to prevent cutting initial consonants
                self.state = RecorderState.RECORDING
                self._recorded_frames = list(self._pre_roll_frames) + [samples]
                self._consecutive_silence_ms = 0.0
                self._total_speech_ms = self.frame_duration_ms
                self._total_recorded_ms = len(self._recorded_frames) * self.frame_duration_ms
                logger.debug("VAD detected voice start -> RECORDING state")
            else:
                self._pre_roll_frames.append(samples)
            return None

        elif self.state == RecorderState.RECORDING:
            self._recorded_frames.append(samples)
            self._total_recorded_ms += self.frame_duration_ms

            if is_voice:
                self._total_speech_ms += self.frame_duration_ms
                self._consecutive_silence_ms = 0.0
            else:
                self._consecutive_silence_ms += self.frame_duration_ms

            # 1. Safety maximum duration check (e.g. 30s continuous speech)
            if self._total_recorded_ms >= (self.max_recording_duration_s * 1000.0):
                logger.info(
                    "Safety recording limit (%.1fs) reached. Finalizing segment.",
                    self.max_recording_duration_s,
                )
                self.state = RecorderState.PROCESSING
                output_path = self.export_wav(self._recorded_frames)
                self.reset()
                return output_path

            # 2. Silence cutoff check (> 800ms)
            if self._consecutive_silence_ms >= self.silence_timeout_ms:
                # Check minimum speech duration (< 300ms is considered noise/clicks/coughs)
                if self._total_speech_ms < self.min_speech_duration_ms:
                    logger.debug(
                        "Discarded brief noise: speech duration %.1fms < %dms",
                        self._total_speech_ms,
                        self.min_speech_duration_ms,
                    )
                    self.reset()
                    return None
                else:
                    logger.debug(
                        "Silence timeout (%dms) reached. Speech duration: %.1fms -> PROCESSING",
                        self.silence_timeout_ms,
                        self._total_speech_ms,
                    )
                    self.state = RecorderState.PROCESSING
                    output_path = self.export_wav(self._recorded_frames)
                    self.reset()
                    return output_path

            return None

        return None

    def export_wav(
        self,
        frames: list[np.ndarray],
        output_path: Path | None = None,
    ) -> Path:
        """Exports accumulated audio frames to a valid RIFF WAV file."""
        if not frames:
            raise ValueError("No audio frames to export.")

        if output_path is None:
            filename = f"utterance_{int(time.time())}_{uuid.uuid4().hex[:8]}.wav"
            output_path = self.temp_dir / filename

        concatenated = np.concatenate(frames).astype(np.int16)

        with wave.open(str(output_path), "wb") as wav_file:
            wav_file.setnchannels(self.channels)
            wav_file.setsampwidth(2)  # 16-bit PCM = 2 bytes per sample
            wav_file.setframerate(self.sample_rate)
            wav_file.writeframes(concatenated.tobytes())

        logger.info("Saved audio utterance (%d samples) to %s", len(concatenated), output_path)
        return output_path

    def _audio_callback(
        self,
        indata: np.ndarray,
        frames: int,
        time_info: dict,
        status: sd.CallbackFlags,
    ) -> None:
        """Sounddevice stream callback running in background audio thread."""
        if status:
            logger.warning("Audio input status warning: %s", status)
        self._audio_queue.put(indata.copy())

    def start_stream(self) -> None:
        """Opens and starts the live sounddevice input stream."""
        if self._stream is not None and self._stream.active:
            return

        self._audio_queue = queue.Queue()
        self._is_listening = True

        logger.info(
            "Opening audio stream (device: %s, rate: %dHz, blocksize: %d)",
            self.device,
            self.sample_rate,
            self.frame_size,
        )
        self._stream = sd.InputStream(
            samplerate=self.sample_rate,
            channels=self.channels,
            dtype=self.dtype,
            blocksize=self.frame_size,
            device=self.device,
            callback=self._audio_callback,
        )
        self._stream.start()

    def stop_stream(self) -> None:
        """Stops and closes the live audio stream, releasing all hardware resources."""
        self._is_listening = False
        if self._stream is not None:
            try:
                if self._stream.active:
                    self._stream.stop()
                self._stream.close()
            except (sd.PortAudioError, OSError) as exc:
                logger.warning("Error closing audio stream: %s", exc)
            finally:
                self._stream = None
        logger.info("Audio stream closed and resources freed.")

    def record_utterance(
        self,
        timeout_s: float | None = None,
        on_state_change: Callable[[RecorderState], None] | None = None,
    ) -> Path | None:
        """Listens from the microphone stream until a complete utterance is captured.

        Handles Ctrl+C and timeouts gracefully, guaranteeing resource release.
        """
        self.reset()
        self.start_stream()
        start_time = time.time()
        last_notified_state = self.state

        try:
            while self._is_listening:
                if timeout_s and (time.time() - start_time) > timeout_s:
                    logger.debug("Listening timed out after %.1fs", timeout_s)
                    break

                try:
                    frame = self._audio_queue.get(timeout=0.1)
                except queue.Empty:
                    continue

                output_path = self.process_frame(frame)

                if on_state_change and self.state != last_notified_state:
                    last_notified_state = self.state
                    on_state_change(self.state)

                if output_path is not None:
                    return output_path

        except KeyboardInterrupt:
            logger.info("Recording interrupted by user (Ctrl+C).")
            raise
        finally:
            self.stop_stream()

        return None

    def close(self) -> None:
        """Releases all stream handles."""
        self.stop_stream()

    def __enter__(self) -> Self:
        return self

    def __exit__(self, exc_type, exc_val, exc_tb) -> None:
        self.close()
