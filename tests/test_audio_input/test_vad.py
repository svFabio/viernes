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
