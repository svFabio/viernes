import math

import numpy as np

from viernes.audio.vad import VoiceActivityDetector, validate_vad_threshold


def test_validate_vad_threshold_valid():
    assert validate_vad_threshold(0.0) == 0.0
    assert validate_vad_threshold(0.5) == 0.5
    assert validate_vad_threshold(1.0) == 1.0
    assert validate_vad_threshold(0.75) == 0.75


def test_validate_vad_threshold_out_of_range():
    assert validate_vad_threshold(-0.1) == 0.5
    assert validate_vad_threshold(1.2) == 0.5
    assert validate_vad_threshold(float("nan")) == 0.5


def test_vad_silence_detection():
    detector = VoiceActivityDetector(sample_rate=16000, threshold=0.5, frame_duration_ms=30)
    # 30ms = 480 samples
    silence = np.zeros(480, dtype=np.float32)
    assert detector.compute_probability(silence) == 0.0
    assert not detector.is_speech(silence)


def test_vad_low_ambient_noise():
    detector = VoiceActivityDetector(sample_rate=16000, threshold=0.5, frame_duration_ms=30)
    # Low amplitude random noise
    np.random.seed(42)
    noise = np.random.normal(0, 0.001, 480).astype(np.float32)
    prob = detector.compute_probability(noise)
    assert prob < 0.3
    assert not detector.is_speech(noise)


def test_vad_simulated_speech_sine_waves():
    detector = VoiceActivityDetector(sample_rate=16000, threshold=0.5, frame_duration_ms=30)
    t = np.linspace(0, 0.03, 480, endpoint=False)
    # Fundamental frequency ~300Hz and formant ~1000Hz (human voice range)
    speech = (0.3 * np.sin(2 * np.pi * 300 * t) + 0.2 * np.sin(2 * np.pi * 1000 * t)).astype(np.float32)

    prob = detector.compute_probability(speech)
    assert prob >= 0.5
    assert detector.is_speech(speech)


def test_vad_high_frequency_noise_rejection():
    detector = VoiceActivityDetector(sample_rate=16000, threshold=0.5, frame_duration_ms=30)
    t = np.linspace(0, 0.03, 480, endpoint=False)
    # High frequency tone outside voice band (e.g. 7000Hz)
    high_freq = (0.2 * np.sin(2 * np.pi * 7000 * t)).astype(np.float32)

    prob = detector.compute_probability(high_freq)
    assert prob < 0.5
    assert not detector.is_speech(high_freq)


def test_vad_int16_bytes_and_numpy():
    detector = VoiceActivityDetector(sample_rate=16000, threshold=0.5, frame_duration_ms=20)
    # 20ms = 320 samples
    t = np.linspace(0, 0.02, 320, endpoint=False)
    speech_float = 0.4 * np.sin(2 * np.pi * 500 * t)
    speech_int16 = (speech_float * 32767).astype(np.int16)
    speech_bytes = speech_int16.tobytes()

    prob_np = detector.compute_probability(speech_int16)
    prob_bytes = detector.compute_probability(speech_bytes)

    assert math.isclose(prob_np, prob_bytes, abs_tol=1e-3)
    assert detector.is_speech(speech_bytes)


def test_vad_dc_offset_rejection():
    detector = VoiceActivityDetector(sample_rate=16000, threshold=0.5)
    # Hardware DC bias (+0.3 constant voltage)
    dc_bias = np.full(480, 0.3, dtype=np.float32)
    assert detector.compute_probability(dc_bias) == 0.0
    assert not detector.is_speech(dc_bias)


def test_vad_nan_and_inf_sanitization():
    detector = VoiceActivityDetector(sample_rate=16000, threshold=0.5)
    nan_frame = np.zeros(480, dtype=np.float32)
    nan_frame[5] = np.nan
    nan_frame[20] = np.inf
    nan_frame[30] = -np.inf

    assert detector.compute_probability(nan_frame) == 0.0
    assert not detector.is_speech(nan_frame)


def test_vad_adversarial_waveform_fuzzing():
    detector = VoiceActivityDetector(sample_rate=16000, threshold=0.5)
    rng = np.random.default_rng(seed=777)

    # Test 200 diverse adversarial waveforms
    for _ in range(200):
        # Mix Dirac impulses, chirps, clipping, and random noise
        wave = rng.uniform(-1.0, 1.0, 480).astype(np.float32)
        # Randomly inject occasional NaN or Inf
        if rng.random() < 0.1:
            wave[rng.integers(0, 480)] = np.nan
        if rng.random() < 0.1:
            wave[rng.integers(0, 480)] = np.inf

        prob = detector.compute_probability(wave)
        assert 0.0 <= prob <= 1.0
        assert isinstance(detector.is_speech(wave), (bool, np.bool_))

