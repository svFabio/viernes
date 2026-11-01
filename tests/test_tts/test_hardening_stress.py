import concurrent.futures
import stat
import subprocess
from pathlib import Path
from unittest.mock import MagicMock, patch

import pytest

from viernes.audio.player import AudioPlayer
from viernes.config import load_settings
from viernes.tts.base import TTSProvider
from viernes.tts.piper_provider import PiperProvider
from viernes.tts.resilient import ResilientTTS


def test_stress_concurrent_resilient_tts(tmp_path):
    primary = MagicMock(spec=TTSProvider)
    fallback = MagicMock(spec=TTSProvider)
    fallback_output = tmp_path / "fallback.wav"
    fallback_output.write_bytes(b"fallback audio")

    # Primary always fails with 429 quota error
    primary.synthesize.side_effect = RuntimeError("429 Too Many Requests: Insufficient quota")
    fallback.synthesize.return_value = fallback_output

    on_fallback = MagicMock()
    resilient = ResilientTTS(primary=primary, fallback=fallback, on_fallback_activated=on_fallback)

    def _worker(thread_idx: int) -> Path:
        return resilient.synthesize(f"Concurrent prompt from thread {thread_idx}")

    # Launch 20 concurrent threads attempting synthesis simultaneously
    with concurrent.futures.ThreadPoolExecutor(max_workers=20) as executor:
        futures = [executor.submit(_worker, i) for i in range(20)]
        results = [f.result() for f in concurrent.futures.as_completed(futures)]

    assert len(results) == 20
    assert all(r == fallback_output for r in results)
    assert resilient.quota_exhausted is True
    # The callback MUST be invoked strictly once, never multiple times due to race conditions
    assert on_fallback.call_count == 1
    assert fallback.synthesize.call_count == 20


def test_resilient_tts_reset_circuit(tmp_path):
    primary = MagicMock(spec=TTSProvider)
    fallback = MagicMock(spec=TTSProvider)
    primary_output = tmp_path / "primary.mp3"
    fallback_output = tmp_path / "fallback.wav"

    primary.synthesize.side_effect = RuntimeError("429 quota_exceeded")
    fallback.synthesize.return_value = fallback_output

    resilient = ResilientTTS(primary=primary, fallback=fallback)
    resilient.synthesize("First message")
    assert resilient.quota_exhausted is True

    # Reset circuit
    resilient.reset_circuit()
    assert resilient.quota_exhausted is False

    # Next attempt tries primary again
    primary.synthesize.side_effect = None
    primary.synthesize.return_value = primary_output
    result = resilient.synthesize("After reset message")
    assert result == primary_output
    assert primary.synthesize.call_count == 2


def test_stress_audio_player_concurrency(tmp_path):
    audio_file = tmp_path / "sound.mp3"
    audio_file.write_bytes(b"dummy")

    mock_proc = MagicMock(spec=subprocess.Popen)
    mock_proc.poll.return_value = None
    mock_proc.pid = 9999

    with (
        patch("shutil.which", return_value="/usr/bin/mpv"),
        patch("subprocess.Popen", return_value=mock_proc),
    ):
        player = AudioPlayer()

        def _play():
            for _ in range(5):
                player.play(audio_file, blocking=False)

        def _stop():
            for _ in range(5):
                player.stop_all()

        def _check():
            for _ in range(10):
                player.is_playing()

        with concurrent.futures.ThreadPoolExecutor(max_workers=10) as executor:
            tasks = []
            for _ in range(4):
                tasks.append(executor.submit(_play))
            for _ in range(3):
                tasks.append(executor.submit(_stop))
            for _ in range(3):
                tasks.append(executor.submit(_check))

            for task in concurrent.futures.as_completed(tasks):
                task.result()


def test_piper_timeout_kills_process(tmp_path):
    mock_proc = MagicMock(spec=subprocess.Popen)
    mock_proc.communicate.side_effect = subprocess.TimeoutExpired(cmd=["piper"], timeout=1.0)

    with (
        patch("shutil.which", return_value="/usr/bin/piper"),
        patch("subprocess.Popen", return_value=mock_proc),
    ):
        provider = PiperProvider(temp_dir=tmp_path, timeout=1.0)
        with pytest.raises(TimeoutError, match="Piper execution timed out after 1.0s"):
            provider.synthesize("Timeout prompt", output_path=tmp_path / "timeout.wav")

        mock_proc.kill.assert_called_once()
        mock_proc.wait.assert_called_once()


def test_audio_player_is_playing_lifecycle(tmp_path):
    audio_file = tmp_path / "sound.mp3"
    audio_file.write_bytes(b"dummy")

    mock_proc = MagicMock(spec=subprocess.Popen)
    mock_proc.poll.return_value = None

    with (
        patch("shutil.which", return_value="/usr/bin/pw-play"),
        patch("subprocess.Popen", return_value=mock_proc),
    ):
        player = AudioPlayer()
        assert not player.is_playing()

        player.play(audio_file, blocking=False)
        assert player.is_playing()

        # Simulate playback completion
        mock_proc.poll.return_value = 0
        assert not player.is_playing()


def test_temp_dir_user_permissions(tmp_path):
    custom_temp = tmp_path / "secure_viernes_temp"
    with patch.dict("os.environ", {"VIERNES_TEMP_DIR": str(custom_temp)}):
        s = load_settings()
        assert s.temp_dir.exists()
        mode = s.temp_dir.stat().st_mode
        # Check permissions: owner read, write, execute (0o700)
        assert stat.S_IMODE(mode) == 0o700
