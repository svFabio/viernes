import logging
import queue
import tempfile
import time
import uuid
import wave
from enum import Enum
from pathlib import Path
from typing import Self

import numpy as np
import sounddevice as sd

logger = logging.getLogger(__name__)


class RecorderState(str, Enum):
    IDLE = "IDLE"
    RECORDING = "RECORDING"
    PROCESSING = "PROCESSING"


class AudioRecorder:
    """Captures real-time 16kHz mono 16-bit PCM audio stream using sounddevice.

    Provides stream lifecycle management, frame queuing, resource cleanup,
    and RIFF WAV export.
    """

    def __init__(
        self,
        sample_rate: int = 16000,
        channels: int = 1,
        dtype: str = "int16",
        frame_duration_ms: int = 30,
        device: int | str | None = None,
        temp_dir: Path | None = None,
    ) -> None:
        self.sample_rate = sample_rate
        self.channels = channels
        self.dtype = dtype
        self.frame_duration_ms = frame_duration_ms
        self.frame_size = int(self.sample_rate * (self.frame_duration_ms / 1000.0))
        self.device = device

        self.temp_dir = temp_dir or Path(tempfile.gettempdir()) / "viernes"
        self.temp_dir.mkdir(parents=True, exist_ok=True)

        self.state: RecorderState = RecorderState.IDLE
        self._stream: sd.InputStream | None = None
        self._audio_queue: queue.Queue[np.ndarray] = queue.Queue()
        self._is_listening: bool = False

    @property
    def is_listening(self) -> bool:
        """Indicates whether audio stream capture is currently active."""
        return self._is_listening

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
        """Opens and starts the live sounddevice PCM input stream."""
        if self._stream is not None and self._stream.active:
            return

        self._audio_queue = queue.Queue()
        self._is_listening = True

        logger.info(
            "Opening audio stream (device: %s, rate: %dHz, channels: %d, blocksize: %d)",
            self.device,
            self.sample_rate,
            self.channels,
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
        """Stops and closes the live audio stream, releasing hardware resources."""
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

    def read_frame(self, timeout: float | None = None) -> np.ndarray | None:
        """Reads a captured PCM frame from the internal buffer.

        Returns:
            np.ndarray of shape (frame_size, channels) with dtype=int16, or None if timed out.
        """
        try:
            return self._audio_queue.get(timeout=timeout)
        except queue.Empty:
            return None

    def record_frames(self, num_frames: int, timeout_per_frame: float = 1.0) -> list[np.ndarray]:
        """Records a fixed number of PCM frames from the active stream."""
        frames: list[np.ndarray] = []
        for _ in range(num_frames):
            frame = self.read_frame(timeout=timeout_per_frame)
            if frame is None:
                break
            frames.append(frame)
        return frames

    def process_frame(self, frame: np.ndarray) -> np.ndarray:
        """Normalizes and validates a captured PCM frame to int16."""
        if frame.dtype != np.int16:
            if frame.dtype in (np.float32, np.float64):
                return (np.clip(frame, -1.0, 1.0) * 32767).astype(np.int16)
            return frame.astype(np.int16)
        return frame

    def export_wav(
        self,
        frames: list[np.ndarray],
        output_path: Path | None = None,
    ) -> Path:
        """Exports accumulated PCM audio frames to a valid RIFF WAV file."""
        if not frames:
            raise ValueError("No audio frames to export.")

        if output_path is None:
            filename = f"capture_{int(time.time())}_{uuid.uuid4().hex[:8]}.wav"
            output_path = self.temp_dir / filename

        concatenated = np.concatenate(frames).astype(np.int16)

        with wave.open(str(output_path), "wb") as wav_file:
            wav_file.setnchannels(self.channels)
            wav_file.setsampwidth(2)  # 16-bit PCM = 2 bytes per sample
            wav_file.setframerate(self.sample_rate)
            wav_file.writeframes(concatenated.tobytes())

        logger.info(
            "Exported audio (%d samples, %.2fs) to %s",
            len(concatenated),
            len(concatenated) / self.sample_rate,
            output_path,
        )
        return output_path

    def close(self) -> None:
        """Closes stream and releases resources."""
        self.stop_stream()

    def __enter__(self) -> Self:
        return self

    def __exit__(self, exc_type: object, exc_val: object, exc_tb: object) -> None:
        self.close()
