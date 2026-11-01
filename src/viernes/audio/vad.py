import logging
import math
from abc import ABC, abstractmethod
from dataclasses import dataclass
from enum import Enum

import numpy as np

logger = logging.getLogger(__name__)


class VADState(str, Enum):
    """Voice Activity Detection state machine states."""

    SILENCE = "SILENCE"
    SPEECH = "SPEECH"


@dataclass(frozen=True)
class VADChunkResult:
    """Result of evaluating an audio chunk through VAD."""

    is_speech: bool
    probability: float
    state: VADState
    consecutive_silence_chunks: int


def validate_vad_threshold(threshold: float) -> float:
    """Validates that vad_threshold is within [0.0, 1.0].

    If out of range, logs a warning and returns default 0.5.
    """
    if not isinstance(threshold, (int, float)) or math.isnan(threshold):
        logger.warning("Invalid VAD_THRESHOLD '%s'. Defaulting to 0.5.", threshold)
        return 0.5
    if threshold < 0.0 or threshold > 1.0:
        logger.warning(
            "VAD_THRESHOLD %s out of range [0.0, 1.0]. Defaulting to 0.5.",
            threshold,
        )
        return 0.5
    return float(threshold)


class BaseVAD(ABC):
    """Abstract Base Class for modular Voice Activity Detectors."""

    def __init__(
        self,
        sample_rate: int = 16000,
        threshold: float = 0.5,
        frame_duration_ms: int = 30,
        silence_transition_chunks: int = 3,
        speech_transition_chunks: int = 1,
    ) -> None:
        self.sample_rate = sample_rate
        self.threshold = validate_vad_threshold(threshold)
        self.frame_duration_ms = frame_duration_ms
        self.expected_frame_size = int(self.sample_rate * (self.frame_duration_ms / 1000.0))
        self.silence_transition_chunks = max(1, silence_transition_chunks)
        self.speech_transition_chunks = max(1, speech_transition_chunks)

        self.state: VADState = VADState.SILENCE
        self.consecutive_silence_count: int = 0
        self.consecutive_speech_count: int = 0

    def _to_float32(self, frame: np.ndarray | bytes) -> np.ndarray:
        """Converts input frame (bytes or int16/float32 array) to normalized float32."""
        if isinstance(frame, bytes):
            int16_arr = np.frombuffer(frame, dtype=np.int16)
            return int16_arr.astype(np.float32) / 32768.0
        elif isinstance(frame, np.ndarray):
            if frame.dtype == np.int16:
                return frame.astype(np.float32) / 32768.0
            elif frame.dtype in (np.float32, np.float64):
                return frame.astype(np.float32)
            else:
                return frame.astype(np.float32)
        else:
            raise TypeError(f"Unsupported frame type: {type(frame)}")

    @abstractmethod
    def compute_probability(self, frame: np.ndarray | bytes) -> float:
        """Computes speech probability for the given audio frame."""
        ...

    def is_speech(self, frame: np.ndarray | bytes) -> bool:
        """Evaluates whether the frame exceeds the speech probability threshold."""
        return self.compute_probability(frame) >= self.threshold

    def process_chunk(self, frame: np.ndarray | bytes) -> VADChunkResult:
        """Processes an audio chunk, updating silence counters and state machine."""
        prob = self.compute_probability(frame)
        active = prob >= self.threshold

        if active:
            self.consecutive_speech_count += 1
            self.consecutive_silence_count = 0
            if self.consecutive_speech_count >= self.speech_transition_chunks:
                self.state = VADState.SPEECH
        else:
            self.consecutive_silence_count += 1
            self.consecutive_speech_count = 0
            if self.consecutive_silence_count >= self.silence_transition_chunks:
                self.state = VADState.SILENCE

        return VADChunkResult(
            is_speech=active,
            probability=prob,
            state=self.state,
            consecutive_silence_chunks=self.consecutive_silence_count,
        )

    def reset(self) -> None:
        """Resets the state machine and silence/speech counters."""
        self.state = VADState.SILENCE
        self.consecutive_silence_count = 0
        self.consecutive_speech_count = 0


class EnergyVAD(BaseVAD):
    """Energy & spectral voice band Voice Activity Detector.

    Evaluates 20-30ms audio blocks (e.g. 320 to 512 samples at 16kHz) by combining:
    1. RMS energy relative to adaptive noise floor.
    2. Spectral energy concentration in human speech band (300Hz - 3400Hz).
    3. Zero-crossing rate (ZCR) filtering.
    """

    def __init__(
        self,
        sample_rate: int = 16000,
        threshold: float = 0.5,
        frame_duration_ms: int = 30,
        silence_transition_chunks: int = 3,
        speech_transition_chunks: int = 1,
    ) -> None:
        super().__init__(
            sample_rate=sample_rate,
            threshold=threshold,
            frame_duration_ms=frame_duration_ms,
            silence_transition_chunks=silence_transition_chunks,
            speech_transition_chunks=speech_transition_chunks,
        )
        self.noise_floor: float = 0.005
        self.alpha_noise: float = 0.95

    def reset(self) -> None:
        """Resets the adaptive noise floor and state counters."""
        super().reset()
        self.noise_floor = 0.005

    def compute_probability(self, frame: np.ndarray | bytes) -> float:
        """Evaluates an audio block and returns speech probability in [0.0, 1.0]."""
        samples = self._to_float32(frame)
        if len(samples) == 0:
            return 0.0

        # 1. Compute RMS energy
        rms = float(np.sqrt(np.mean(samples**2) + 1e-12))

        # Absolute silence floor check
        if rms < 0.002:
            return 0.0

        # 2. Spectral analysis via FFT
        fft_vals = np.abs(np.fft.rfft(samples))
        freqs = np.fft.rfftfreq(len(samples), d=1.0 / self.sample_rate)

        total_energy = float(np.sum(fft_vals**2) + 1e-12)
        voice_band_mask = (freqs >= 300) & (freqs <= 3400)
        voice_band_energy = float(np.sum(fft_vals[voice_band_mask] ** 2))
        voice_band_ratio = voice_band_energy / total_energy

        # 3. Zero-Crossing Rate (ZCR)
        zero_crossings = np.sum(np.abs(np.diff(np.signbit(samples))))
        zcr = float(zero_crossings / len(samples))

        # 4. Energy relative to noise floor (SNR proxy)
        snr = rms / (self.noise_floor + 1e-6)

        # Update noise floor adaptively when energy is low
        if snr < 1.5:
            self.noise_floor = self.alpha_noise * self.noise_floor + (1.0 - self.alpha_noise) * rms

        # 5. Probability estimation model
        energy_score = 1.0 / (1.0 + np.exp(-1.8 * (snr - 2.5)))
        spectral_score = 1.0 / (1.0 + np.exp(-10.0 * (voice_band_ratio - 0.45)))

        # ZCR penalty if purely high frequency noise
        zcr_penalty = 0.5 if (zcr > 0.45 and voice_band_ratio < 0.3) else 1.0

        raw_prob = float(0.6 * energy_score + 0.4 * spectral_score) * zcr_penalty
        return max(0.0, min(1.0, raw_prob))


# Default alias
VoiceActivityDetector = EnergyVAD
