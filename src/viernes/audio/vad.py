import logging
import math

import numpy as np

logger = logging.getLogger(__name__)


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


class VoiceActivityDetector:
    """Voice Activity Detector (VAD) processing 20-30ms audio blocks at 16kHz.

    Calculates speech probability using adaptive energy, spectral concentration
    in voice frequencies (300Hz - 3400Hz), and zero-crossing characteristics.
    """

    def __init__(
        self,
        sample_rate: int = 16000,
        threshold: float = 0.5,
        frame_duration_ms: int = 30,
    ) -> None:
        self.sample_rate = sample_rate
        self.threshold = validate_vad_threshold(threshold)
        self.frame_duration_ms = frame_duration_ms
        self.expected_frame_size = int(self.sample_rate * (self.frame_duration_ms / 1000.0))

        # Dynamic noise floor estimation (in RMS)
        self.noise_floor: float = 0.005
        self.alpha_noise: float = 0.95  # smoothing factor for noise update

    def reset(self) -> None:
        """Resets the adaptive noise floor estimate."""
        self.noise_floor = 0.005

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

    def compute_probability(self, frame: np.ndarray | bytes) -> float:
        """Evaluates a 20-30ms audio block and returns voice probability [0.0, 1.0]."""
        samples = self._to_float32(frame)
        if len(samples) == 0:
            return 0.0

        # 1. Compute RMS energy
        rms = float(np.sqrt(np.mean(samples**2) + 1e-12))

        # 2. Spectral analysis via FFT
        # Human speech fundamental & formants are primarily concentrated between 300Hz and 3400Hz
        fft_vals = np.abs(np.fft.rfft(samples))
        freqs = np.fft.rfftfreq(len(samples), d=1.0 / self.sample_rate)

        total_energy = float(np.sum(fft_vals**2) + 1e-12)
        voice_band_mask = (freqs >= 300) & (freqs <= 3400)
        voice_band_energy = float(np.sum(fft_vals[voice_band_mask] ** 2))
        voice_band_ratio = voice_band_energy / total_energy

        # 3. Zero-Crossing Rate (ZCR)
        # Voiced speech typically has low to moderate ZCR compared to white noise
        zero_crossings = np.sum(np.abs(np.diff(np.signbit(samples))))
        zcr = float(zero_crossings / len(samples))

        # 4. Energy relative to noise floor (Signal-to-Noise Ratio proxy)
        snr = rms / (self.noise_floor + 1e-6)

        # Update noise floor adaptively when energy is low
        if snr < 1.5:
            self.noise_floor = self.alpha_noise * self.noise_floor + (1.0 - self.alpha_noise) * rms

        # 5. Probability estimation model
        # Sigmoid based on SNR and voice band concentration
        # Strong speech indicator: SNR > 3.0 and voice_band_ratio > 0.4
        energy_score = 1.0 / (1.0 + np.exp(-1.8 * (snr - 2.5)))
        spectral_score = 1.0 / (1.0 + np.exp(-10.0 * (voice_band_ratio - 0.45)))

        # ZCR penalty if purely high frequency noise (e.g. zcr > 0.45 with low voice band)
        zcr_penalty = 0.5 if (zcr > 0.45 and voice_band_ratio < 0.3) else 1.0

        # Absolute silence floor check
        if rms < 0.002:
            return 0.0

        raw_prob = float(0.6 * energy_score + 0.4 * spectral_score) * zcr_penalty
        prob = max(0.0, min(1.0, raw_prob))
        return prob

    def is_speech(self, frame: np.ndarray | bytes) -> bool:
        """Determines whether the given audio block contains active voice speech."""
        return self.compute_probability(frame) >= self.threshold
