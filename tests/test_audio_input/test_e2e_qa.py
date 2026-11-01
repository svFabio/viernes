import wave
from pathlib import Path
from unittest.mock import MagicMock, patch

import numpy as np

from viernes.audio.recorder import AudioRecorder, RecorderState
from viernes.input.push_to_talk import PushToTalkController, PushToTalkState
from viernes.stt.fake_provider import FakeSTTProvider
from viernes.stt.whisper_provider import WhisperSTTProvider


def _generate_synthetic_speech_frames(duration_ms: int = 600, freq: float = 400.0) -> list[np.ndarray]:
    """Generates synthetic 16kHz speech audio in 30ms chunks."""
    frames = []
    chunk_samples = 480  # 30ms at 16kHz
    num_chunks = duration_ms // 30
    for chunk_idx in range(num_chunks):
        t = np.linspace(chunk_idx * 0.03, (chunk_idx + 1) * 0.03, chunk_samples, endpoint=False)
        # Mix fundamental voice frequencies
        signal = 0.35 * np.sin(2 * np.pi * freq * t) + 0.15 * np.sin(2 * np.pi * (freq * 2.5) * t)
        frames.append((signal * 32767).astype(np.int16))
    return frames


def _generate_silence_frames(duration_ms: int = 900) -> list[np.ndarray]:
    """Generates synthetic 16kHz silence in 30ms chunks."""
    frames = []
    chunk_samples = 480
    num_chunks = duration_ms // 30
    for _ in range(num_chunks):
        frames.append(np.zeros(chunk_samples, dtype=np.int16))
    return frames


def test_e2e_pipeline_audio_recorder_to_stt(tmp_path: Path):
    """QA Scenario 1: Natural voice utterance captured via VAD and transcribed."""
    recorder = AudioRecorder(
        temp_dir=tmp_path / "audio",
        frame_duration_ms=30,
        silence_timeout_ms=800,
        min_speech_duration_ms=300,
    )

    # 1. Feed 600ms of active speech
    speech_frames = _generate_synthetic_speech_frames(600)
    for frame in speech_frames:
        recorder.process_frame(frame)

    assert recorder.state == RecorderState.RECORDING

    # 2. Feed 900ms of continuous silence (> 800ms cutoff)
    silence_frames = _generate_silence_frames(900)
    wav_path: Path | None = None
    for frame in silence_frames:
        res = recorder.process_frame(frame)
        if res:
            wav_path = res
            break

    # Audio file must be created with valid RIFF header
    assert wav_path is not None
    assert wav_path.exists()
    with wave.open(str(wav_path), "rb") as wf:
        assert wf.getframerate() == 16000
        assert wf.getnchannels() == 1
        assert wf.getsampwidth() == 2
        assert wf.getnframes() > 0

    # 3. Transcribe via STT Provider
    stt = FakeSTTProvider(default_text="  Viernes, ¿cuál es el clima hoy?  ")
    transcription = stt.transcribe(wav_path)

    assert transcription == "Viernes, ¿cuál es el clima hoy?"
    assert stt.call_count == 1
    assert stt.history[0] == wav_path


def test_e2e_push_to_talk_to_stt_transcription(tmp_path: Path):
    """QA Scenario 2: Push-to-Talk activation, release, and transcription dispatch."""
    recorder = AudioRecorder(temp_dir=tmp_path / "ptt_audio")
    recorded_audio_path: Path | None = None

    def start_recording():
        recorder.reset()
        recorder.state = RecorderState.RECORDING

    def finish_recording():
        # User released shortcut: export current buffer
        nonlocal recorded_audio_path
        samples = _generate_synthetic_speech_frames(500)
        recorded_audio_path = recorder.export_wav(samples)

    stt = FakeSTTProvider(default_text="abrir terminal")
    ptt = PushToTalkController(
        shortcut="Super+V",
        on_start=start_recording,
        on_finish=finish_recording,
    )

    # Key Down: state transitions to LISTENING
    ptt.handle_key_down()
    assert ptt.state == PushToTalkState.LISTENING
    assert recorder.state == RecorderState.RECORDING

    # Key Up: state transitions to PROCESSING
    ptt.handle_key_up()
    assert ptt.state == PushToTalkState.PROCESSING
    assert recorded_audio_path is not None
    assert recorded_audio_path.exists()

    # Transcribe output
    text = stt.transcribe(recorded_audio_path)
    assert text == "abrir terminal"


def test_e2e_push_to_talk_escape_cancellation(tmp_path: Path):
    """QA Scenario 3: User presses Escape to discard recording safely."""
    cancelled = False

    def cancel_recording():
        nonlocal cancelled
        cancelled = True

    stt = FakeSTTProvider()
    ptt = PushToTalkController(
        shortcut="Ctrl+Alt+Space",
        on_cancel=cancel_recording,
    )

    # Key Down
    ptt.handle_key_down()
    assert ptt.state == PushToTalkState.LISTENING

    # Escape pressed
    ptt.handle_escape()
    assert ptt.state == PushToTalkState.IDLE
    assert cancelled is True
    # STT must not be called
    assert stt.call_count == 0


def test_e2e_whisper_edge_cases(tmp_path: Path):
    """QA Scenario 4: WhisperSTTProvider edge cases with corrupted / empty audio."""
    # Corrupted / non-wav file
    corrupt_file = tmp_path / "corrupt.wav"
    corrupt_file.write_bytes(b"NOT A RIFF WAV FILE AT ALL")

    mock_model = MagicMock()
    mock_model.transcribe.side_effect = RuntimeError("Failed to decode audio header")

    provider = WhisperSTTProvider(model=mock_model)
    # Must return empty string cleanly without crashing
    assert provider.transcribe(corrupt_file) == ""


def test_e2e_robust_env_resilience(monkeypatch, tmp_path: Path):
    """QA Scenario 5: Resilience against malformed environment variables."""
    # Malformed non-numeric values in .env
    monkeypatch.setenv("VAD_THRESHOLD", "not_a_float")
    monkeypatch.setenv("SILENCE_TIMEOUT_MS", "invalid_int")
    monkeypatch.setenv("AUDIO_INPUT_DEVICE", "99999_non_existent")

    recorder = AudioRecorder(temp_dir=tmp_path)
    # Must fallback gracefully to defaults without crashing
    assert recorder.vad_threshold == 0.5
    assert recorder.silence_timeout_ms == 800
    assert recorder.device is None  # Defaults to system default


def test_e2e_main_cli_listen_loop(tmp_path: Path):
    """QA Scenario 6: Full end-to-end integration via CLI --listen flag."""
    sample_wav = tmp_path / "voice_cmd.wav"
    sample_wav.write_bytes(b"dummy audio")

    with patch("sys.argv", ["viernes", "--listen"]), \
         patch("viernes.audio.recorder.AudioRecorder.record_utterance", return_value=sample_wav), \
         patch("viernes.stt.whisper_provider.WhisperSTTProvider.transcribe", return_value="hola viernes"), \
         patch("viernes.tts.resilient.ResilientTTS.synthesize", return_value=tmp_path / "out.mp3") as mock_synth, \
         patch("viernes.audio.player.AudioPlayer.play") as mock_play:

        from viernes.main import main
        main()

        mock_synth.assert_called_once()
        assert mock_synth.call_args[0][0] == "hola viernes"
        mock_play.assert_called_once()
