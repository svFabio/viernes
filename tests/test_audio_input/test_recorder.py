import wave
from pathlib import Path
from unittest.mock import MagicMock, patch

import numpy as np
import pytest

from viernes.audio.recorder import AudioRecorder, RecorderState, resample_audio


@pytest.fixture
def temp_audio_dir(tmp_path: Path) -> Path:
    audio_dir = tmp_path / "audio_out"
    audio_dir.mkdir(parents=True, exist_ok=True)
    return audio_dir


def _make_frame(duration_ms: int = 30, freq: float = 400.0, is_speech: bool = True) -> np.ndarray:
    """Generates synthetic 16kHz audio frame."""
    num_samples = int(16000 * (duration_ms / 1000.0))
    t = np.linspace(0, duration_ms / 1000.0, num_samples, endpoint=False)
    if is_speech:
        signal = 0.4 * np.sin(2 * np.pi * freq * t)
    else:
        signal = np.zeros(num_samples)
    return (signal * 32767).astype(np.int16)


def test_recorder_initial_state(temp_audio_dir: Path):
    recorder = AudioRecorder(temp_dir=temp_audio_dir)
    assert recorder.state == RecorderState.IDLE
    assert recorder.sample_rate == 16000
    assert recorder.channels == 1
    assert recorder.silence_timeout_ms == 800


def test_recorder_silence_timeout_validation(temp_audio_dir: Path):
    rec_low = AudioRecorder(silence_timeout_ms=100, temp_dir=temp_audio_dir)
    assert rec_low.silence_timeout_ms == 800  # Clamped to default

    rec_high = AudioRecorder(silence_timeout_ms=5000, temp_dir=temp_audio_dir)
    assert rec_high.silence_timeout_ms == 800  # Clamped to default

    rec_valid = AudioRecorder(silence_timeout_ms=1200, temp_dir=temp_audio_dir)
    assert rec_valid.silence_timeout_ms == 1200


def test_recorder_speech_capture_and_silence_cutoff(temp_audio_dir: Path):
    recorder = AudioRecorder(
        temp_dir=temp_audio_dir,
        frame_duration_ms=30,
        silence_timeout_ms=800,
        min_speech_duration_ms=300,
    )

    silence_frame = _make_frame(30, is_speech=False)
    speech_frame = _make_frame(30, is_speech=True)

    # 1. Feed initial silence -> stays IDLE
    for _ in range(5):
        assert recorder.process_frame(silence_frame) is None
        assert recorder.state == RecorderState.IDLE

    # 2. Feed 500ms of speech (approx 17 frames of 30ms) -> transitions to RECORDING
    for _ in range(17):
        assert recorder.process_frame(speech_frame) is None
        assert recorder.state == RecorderState.RECORDING

    # 3. Feed silence until 800ms threshold reached (800ms / 30ms = ~27 frames)
    wav_path: Path | None = None
    for _ in range(27):
        res = recorder.process_frame(silence_frame)
        if res is not None:
            wav_path = res
            break

    assert wav_path is not None
    assert wav_path.exists()
    assert recorder.state == RecorderState.IDLE  # Automatically resets after processing

    # Verify RIFF WAV validity
    with wave.open(str(wav_path), "rb") as wf:
        assert wf.getnchannels() == 1
        assert wf.getsampwidth() == 2
        assert wf.getframerate() == 16000
        n_frames = wf.getnframes()
        assert n_frames > 0


def test_recorder_brief_noise_filtering(temp_audio_dir: Path):
    recorder = AudioRecorder(
        temp_dir=temp_audio_dir,
        frame_duration_ms=30,
        silence_timeout_ms=800,
        min_speech_duration_ms=300,
    )

    silence_frame = _make_frame(30, is_speech=False)
    speech_frame = _make_frame(30, is_speech=True)

    # Brief noise: only 3 frames (90ms < 300ms)
    recorder.process_frame(speech_frame)
    recorder.process_frame(speech_frame)
    recorder.process_frame(speech_frame)
    assert recorder.state == RecorderState.RECORDING

    # Now feed silence > 800ms (27 frames)
    wav_path = None
    for _ in range(28):
        res = recorder.process_frame(silence_frame)
        if res is not None:
            wav_path = res

    # Short noise should be discarded!
    assert wav_path is None
    assert recorder.state == RecorderState.IDLE


def test_recorder_max_duration_safety_cutoff(temp_audio_dir: Path):
    # Set a small max duration for test: 1.0 second
    recorder = AudioRecorder(
        temp_dir=temp_audio_dir,
        frame_duration_ms=30,
        max_recording_duration_s=1.0,
        silence_timeout_ms=800,
    )

    speech_frame = _make_frame(30, is_speech=True)

    wav_path: Path | None = None
    # Feed speech continuously for > 1.0s (34 frames * 30ms = 1020ms)
    for _ in range(35):
        res = recorder.process_frame(speech_frame)
        if res is not None:
            wav_path = res
            break

    assert wav_path is not None
    assert wav_path.exists()
    with wave.open(str(wav_path), "rb") as wf:
        duration_s = wf.getnframes() / wf.getframerate()
        assert duration_s >= 0.9


def test_recorder_stream_cleanup_and_context_manager(temp_audio_dir: Path):
    with AudioRecorder(temp_dir=temp_audio_dir) as recorder:
        assert recorder._stream is None
        # Mock sounddevice stream
        mock_stream = MagicMock()
        mock_stream.active = True
        recorder._stream = mock_stream

    # Exiting context manager must call close() and stop_stream()
    assert recorder._stream is None
    mock_stream.stop.assert_called_once()
    mock_stream.close.assert_called_once()


def test_recorder_keyboard_interrupt_handling(temp_audio_dir: Path):
    recorder = AudioRecorder(temp_dir=temp_audio_dir)

    def fake_start():
        recorder._is_listening = True

    with patch.object(recorder, "start_stream", side_effect=fake_start) as mock_start, \
         patch.object(recorder, "stop_stream") as mock_stop, \
         patch.object(recorder._audio_queue, "get", side_effect=KeyboardInterrupt):
        with pytest.raises(KeyboardInterrupt):
            recorder.record_utterance()

        mock_start.assert_called_once()
        mock_stop.assert_called_once()


def test_resample_audio_linear():
    t_48k = np.linspace(0, 0.03, 1440, endpoint=False)  # 30ms at 48kHz
    signal_48k = (0.4 * np.sin(2 * np.pi * 400 * t_48k) * 32767).astype(np.int16)

    resampled_16k = resample_audio(signal_48k, orig_sr=48000, target_sr=16000)
    assert len(resampled_16k) == 480  # 30ms at 16kHz
    assert resampled_16k.dtype == np.int16

    # Test identical rate returns original
    assert len(resample_audio(signal_48k, 48000, 48000)) == 1440


def test_recorder_resamples_48khz_input(temp_audio_dir: Path):
    recorder = AudioRecorder(
        sample_rate=16000,
        input_sample_rate=48000,
        temp_dir=temp_audio_dir,
    )
    # Feed 30ms frame at 48kHz (1440 samples)
    t_48k = np.linspace(0, 0.03, 1440, endpoint=False)
    speech_48k = (0.4 * np.sin(2 * np.pi * 400 * t_48k) * 32767).astype(np.int16)

    recorder.process_frame(speech_48k)
    assert recorder.state == RecorderState.RECORDING
    # The recorded frame stored in internal buffer must be resampled to 16kHz (480 samples)
    assert len(recorder._recorded_frames[-1]) == 480
