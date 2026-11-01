import wave
from pathlib import Path
from unittest.mock import MagicMock, patch

import numpy as np
import pytest
import sounddevice as sd

from viernes.audio.recorder import AudioRecorder, RecorderState


@pytest.fixture
def temp_audio_dir(tmp_path: Path) -> Path:
    audio_dir = tmp_path / "audio_out"
    audio_dir.mkdir(parents=True, exist_ok=True)
    return audio_dir


def _make_frame(duration_ms: int = 30, freq: float = 440.0) -> np.ndarray:
    """Generates synthetic 16kHz mono int16 audio frame."""
    num_samples = int(16000 * (duration_ms / 1000.0))
    t = np.linspace(0, duration_ms / 1000.0, num_samples, endpoint=False)
    signal = 0.5 * np.sin(2 * np.pi * freq * t)
    return (signal * 32767).astype(np.int16)


def test_recorder_initial_configuration(temp_audio_dir: Path) -> None:
    recorder = AudioRecorder(temp_dir=temp_audio_dir)
    assert recorder.sample_rate == 16000
    assert recorder.channels == 1
    assert recorder.dtype == "int16"
    assert recorder.frame_duration_ms == 30
    assert recorder.frame_size == 480  # 16000 * 0.03
    assert recorder.state == RecorderState.IDLE
    assert not recorder.is_listening

    rec_20ms = AudioRecorder(frame_duration_ms=20, temp_dir=temp_audio_dir)
    assert rec_20ms.frame_size == 320  # 16000 * 0.02


def test_recorder_start_and_stop_stream(temp_audio_dir: Path) -> None:
    recorder = AudioRecorder(temp_dir=temp_audio_dir)

    with patch("sounddevice.InputStream") as mock_input_stream_cls:
        mock_stream = MagicMock()
        mock_stream.active = True
        mock_input_stream_cls.return_value = mock_stream

        recorder.start_stream()

        assert recorder.is_listening
        mock_input_stream_cls.assert_called_once_with(
            samplerate=16000,
            channels=1,
            dtype="int16",
            blocksize=480,
            device=None,
            callback=recorder._audio_callback,
        )
        mock_stream.start.assert_called_once()

        # Calling start again while active should be a no-op
        recorder.start_stream()
        assert mock_input_stream_cls.call_count == 1

        recorder.stop_stream()
        assert not recorder.is_listening
        mock_stream.stop.assert_called_once()
        mock_stream.close.assert_called_once()
        assert recorder._stream is None


def test_recorder_context_manager_cleanup(temp_audio_dir: Path) -> None:
    mock_stream = MagicMock()
    mock_stream.active = True

    with (
        patch("sounddevice.InputStream", return_value=mock_stream),
        AudioRecorder(temp_dir=temp_audio_dir) as recorder,
    ):
        recorder.start_stream()
        assert recorder.is_listening

    # Outside context manager, stream must be closed
    assert not recorder.is_listening
    mock_stream.stop.assert_called_once()
    mock_stream.close.assert_called_once()


def test_recorder_context_manager_cleanup_on_exception(temp_audio_dir: Path) -> None:
    mock_stream = MagicMock()
    mock_stream.active = True

    with (
        patch("sounddevice.InputStream", return_value=mock_stream),
        pytest.raises(RuntimeError, match="Simulated error"),
        AudioRecorder(temp_dir=temp_audio_dir) as recorder,
    ):
        recorder.start_stream()
        raise RuntimeError("Simulated error")

    assert not recorder.is_listening
    mock_stream.stop.assert_called_once()
    mock_stream.close.assert_called_once()


def test_audio_callback_and_read_frame(temp_audio_dir: Path) -> None:
    recorder = AudioRecorder(temp_dir=temp_audio_dir)
    frame_a = _make_frame(30, freq=300.0)
    frame_b = _make_frame(30, freq=600.0)

    # Simulate sounddevice callback
    recorder._audio_callback(frame_a, len(frame_a), {}, sd.CallbackFlags())
    recorder._audio_callback(frame_b, len(frame_b), {}, sd.CallbackFlags())

    read_a = recorder.read_frame(timeout=0.1)
    read_b = recorder.read_frame(timeout=0.1)
    read_empty = recorder.read_frame(timeout=0.01)

    assert read_a is not None
    assert np.array_equal(read_a, frame_a)
    assert read_b is not None
    assert np.array_equal(read_b, frame_b)
    assert read_empty is None


def test_record_frames(temp_audio_dir: Path) -> None:
    recorder = AudioRecorder(temp_dir=temp_audio_dir)
    test_frames = [_make_frame(30, freq=float(i * 100)) for i in range(1, 4)]

    for f in test_frames:
        recorder._audio_callback(f, len(f), {}, sd.CallbackFlags())

    recorded = recorder.record_frames(num_frames=3, timeout_per_frame=0.1)
    assert len(recorded) == 3
    for orig, rec in zip(test_frames, recorded, strict=True):
        assert np.array_equal(orig, rec)


def test_process_frame_normalization(temp_audio_dir: Path) -> None:
    recorder = AudioRecorder(temp_dir=temp_audio_dir)

    # int16 frame
    int16_frame = np.array([0, 1000, -1000], dtype=np.int16)
    proc_int16 = recorder.process_frame(int16_frame)
    assert proc_int16.dtype == np.int16
    assert np.array_equal(proc_int16, int16_frame)

    # float32 frame in [-1.0, 1.0]
    float_frame = np.array([0.0, 0.5, -0.5, 1.0, -1.0], dtype=np.float32)
    proc_float = recorder.process_frame(float_frame)
    assert proc_float.dtype == np.int16
    assert proc_float[0] == 0
    assert 16000 < proc_float[1] < 16500
    assert -16500 < proc_float[2] < -16000


def test_export_wav_valid_riff_header(temp_audio_dir: Path) -> None:
    recorder = AudioRecorder(temp_dir=temp_audio_dir)
    frames = [_make_frame(30, freq=440.0) for _ in range(5)]  # 150ms total, 2400 samples

    target_wav = temp_audio_dir / "output.wav"
    result_path = recorder.export_wav(frames, output_path=target_wav)

    assert result_path == target_wav
    assert result_path.exists()

    with wave.open(str(result_path), "rb") as wf:
        assert wf.getnchannels() == 1
        assert wf.getsampwidth() == 2  # 16 bits = 2 bytes
        assert wf.getframerate() == 16000
        assert wf.getnframes() == 2400  # 5 * 480


def test_export_wav_empty_frames_raises(temp_audio_dir: Path) -> None:
    recorder = AudioRecorder(temp_dir=temp_audio_dir)
    with pytest.raises(ValueError, match="No audio frames to export."):
        recorder.export_wav([])


def test_stop_stream_handles_portaudio_error(temp_audio_dir: Path) -> None:
    recorder = AudioRecorder(temp_dir=temp_audio_dir)
    mock_stream = MagicMock()
    mock_stream.active = True
    mock_stream.stop.side_effect = sd.PortAudioError("Device disconnected")

    recorder._stream = mock_stream
    recorder._is_listening = True

    recorder.stop_stream()
    assert not recorder.is_listening
    assert recorder._stream is None
