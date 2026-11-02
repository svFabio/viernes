import math

import numpy as np

from viernes.audio.vad import (
    BaseVAD,
    EnergyVAD,
    VADChunkResult,
    VADState,
    VoiceActivityDetector,
    validate_vad_threshold,
)


def _make_chunk(
    num_samples: int = 480, sample_rate: int = 16000, freq: float = 400.0, is_speech: bool = True
) -> np.ndarray:
    t = np.linspace(0, num_samples / sample_rate, num_samples, endpoint=False)
    if is_speech:
        # Combination of human fundamental and formant frequencies
        signal = 0.4 * np.sin(2 * np.pi * freq * t) + 0.2 * np.sin(2 * np.pi * (freq * 2.5) * t)
    else:
        signal = np.zeros(num_samples)
    return (signal * 32767).astype(np.int16)


def test_validate_vad_threshold():
    assert validate_vad_threshold(0.0) == 0.0
    assert validate_vad_threshold(0.5) == 0.5
    assert validate_vad_threshold(1.0) == 1.0
    assert validate_vad_threshold(0.8) == 0.8

    # Out of range or invalid
    assert validate_vad_threshold(-0.1) == 0.5
    assert validate_vad_threshold(1.5) == 0.5
    assert validate_vad_threshold(float("nan")) == 0.5


def test_vad_silence_detection():
    vad = EnergyVAD(sample_rate=16000, threshold=0.5, frame_duration_ms=30)
    silence = np.zeros(480, dtype=np.int16)

    prob = vad.compute_probability(silence)
    assert prob == 0.0
    assert not vad.is_speech(silence)


def test_vad_low_amplitude_ambient_noise():
    vad = EnergyVAD(sample_rate=16000, threshold=0.5, frame_duration_ms=30)
    np.random.seed(42)
    noise = np.random.normal(0, 50, 480).astype(np.int16)

    prob = vad.compute_probability(noise)
    assert prob < 0.4
    assert not vad.is_speech(noise)


def test_vad_speech_detection():
    vad = EnergyVAD(sample_rate=16000, threshold=0.5, frame_duration_ms=30)
    speech = _make_chunk(480, freq=350.0, is_speech=True)

    prob = vad.compute_probability(speech)
    assert prob >= 0.5
    assert vad.is_speech(speech)


def test_vad_high_frequency_noise_rejection():
    vad = EnergyVAD(sample_rate=16000, threshold=0.5, frame_duration_ms=30)
    t = np.linspace(0, 480 / 16000, 480, endpoint=False)
    # 7200Hz tone well outside speech band (300-3400Hz)
    high_freq_signal = (0.3 * np.sin(2 * np.pi * 7200 * t) * 32767).astype(np.int16)

    prob = vad.compute_probability(high_freq_signal)
    assert prob < 0.5
    assert not vad.is_speech(high_freq_signal)


def test_vad_state_transitions_and_silence_counter():
    vad = EnergyVAD(
        sample_rate=16000,
        threshold=0.5,
        frame_duration_ms=30,
        silence_transition_chunks=3,
        speech_transition_chunks=1,
    )
    assert vad.state == VADState.SILENCE
    assert vad.consecutive_silence_count == 0

    speech_chunk = _make_chunk(480, freq=400.0, is_speech=True)
    silence_chunk = np.zeros(480, dtype=np.int16)

    # 1. Processing speech triggers SPEECH state immediately (speech_transition_chunks=1)
    res1 = vad.process_chunk(speech_chunk)
    assert res1.is_speech is True
    assert res1.state == VADState.SPEECH
    assert res1.consecutive_silence_chunks == 0
    assert vad.consecutive_speech_count == 1

    # 2. First silent chunk: silence count becomes 1, but state remains SPEECH until threshold 3
    res2 = vad.process_chunk(silence_chunk)
    assert res2.is_speech is False
    assert res2.state == VADState.SPEECH
    assert res2.consecutive_silence_chunks == 1

    # 3. Second silent chunk: silence count is 2, still in SPEECH
    res3 = vad.process_chunk(silence_chunk)
    assert res3.state == VADState.SPEECH
    assert res3.consecutive_silence_chunks == 2

    # 4. Third silent chunk: silence count reaches threshold 3 -> transitions to SILENCE
    res4 = vad.process_chunk(silence_chunk)
    assert res4.state == VADState.SILENCE
    assert res4.consecutive_silence_chunks == 3


def test_vad_reset():
    vad = EnergyVAD(sample_rate=16000)
    speech = _make_chunk(480, is_speech=True)
    vad.process_chunk(speech)

    assert vad.state == VADState.SPEECH
    assert vad.consecutive_speech_count > 0

    vad.reset()
    assert vad.state == VADState.SILENCE
    assert vad.consecutive_speech_count == 0
    assert vad.consecutive_silence_count == 0
    assert vad.noise_floor == 0.005


def test_vad_chunk_sizes_and_formats():
    vad = VoiceActivityDetector(sample_rate=16000)

    # 20ms = 320 samples
    chunk_20ms = _make_chunk(320, is_speech=True)
    assert vad.is_speech(chunk_20ms)

    # 512 samples (~32ms)
    chunk_512 = _make_chunk(512, is_speech=True)
    assert vad.is_speech(chunk_512)

    # Raw bytes input
    bytes_chunk = chunk_512.tobytes()
    assert vad.is_speech(bytes_chunk)

    # Float32 normalized array
    float_chunk = chunk_512.astype(np.float32) / 32768.0
    assert vad.is_speech(float_chunk)


def test_modular_custom_vad_subclass():
    class MockVAD(BaseVAD):
        def compute_probability(self, frame: np.ndarray | bytes) -> float:
            return 0.9

    custom = MockVAD(threshold=0.8)
    assert custom.is_speech(np.zeros(100))
    result = custom.process_chunk(np.zeros(100))
    assert isinstance(result, VADChunkResult)
    assert result.state == VADState.SPEECH
