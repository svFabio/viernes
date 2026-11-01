import subprocess
from unittest.mock import MagicMock, patch

import pytest

from viernes.audio.player import AudioPlayer


def test_resolve_player_priority():
    with patch("shutil.which") as mock_which:
        # If all exist, choose mpv
        mock_which.side_effect = lambda cmd: f"/usr/bin/{cmd}"
        player = AudioPlayer()
        assert player.player_bin == "mpv"

    with patch("shutil.which") as mock_which:
        # If mpv is missing, choose pw-play
        mock_which.side_effect = lambda cmd: f"/usr/bin/{cmd}" if cmd != "mpv" else None
        player = AudioPlayer()
        assert player.player_bin == "pw-play"


def test_resolve_player_none_found():
    with (
        patch("shutil.which", return_value=None),
        pytest.raises(RuntimeError, match="No supported audio player found in PATH"),
    ):
        AudioPlayer()


def test_play_file_not_found(tmp_path):
    with patch("shutil.which", return_value="/usr/bin/mpv"):
        player = AudioPlayer()
        missing_file = tmp_path / "non_existent.mp3"
        with pytest.raises(FileNotFoundError, match="Audio file does not exist"):
            player.play(missing_file)


def test_play_blocking(tmp_path):
    audio_file = tmp_path / "sample.mp3"
    audio_file.write_bytes(b"mock audio data")

    mock_proc = MagicMock(spec=subprocess.Popen)
    mock_proc.poll.return_value = 0

    with (
        patch("shutil.which", return_value="/usr/bin/mpv"),
        patch("subprocess.Popen", return_value=mock_proc) as mock_popen,
    ):
        player = AudioPlayer()
        result = player.play(audio_file, blocking=True)

        assert result == mock_proc
        mock_popen.assert_called_once_with(
            ["mpv", "--no-terminal", "--no-video", str(audio_file)],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        mock_proc.wait.assert_called_once()


def test_play_non_blocking(tmp_path):
    audio_file = tmp_path / "sample.mp3"
    audio_file.write_bytes(b"mock audio data")

    mock_proc = MagicMock(spec=subprocess.Popen)
    mock_proc.poll.return_value = None

    with (
        patch("shutil.which", return_value="/usr/bin/pw-play"),
        patch("subprocess.Popen", return_value=mock_proc),
    ):
        player = AudioPlayer()
        result = player.play(audio_file, blocking=False)

        assert result == mock_proc
        mock_proc.wait.assert_not_called()
        assert mock_proc in player._active_processes


def test_stop_all(tmp_path):
    audio_file = tmp_path / "sample.mp3"
    audio_file.write_bytes(b"mock audio data")

    mock_proc1 = MagicMock(spec=subprocess.Popen)
    mock_proc1.poll.return_value = None
    mock_proc1.pid = 1001

    mock_proc2 = MagicMock(spec=subprocess.Popen)
    mock_proc2.poll.return_value = 0  # Already finished
    mock_proc2.pid = 1002

    with (
        patch("shutil.which", return_value="/usr/bin/aplay"),
        patch("subprocess.Popen", side_effect=[mock_proc1, mock_proc2]),
    ):
        player = AudioPlayer()
        player.play(audio_file, blocking=False)
        player.play(audio_file, blocking=False)

        player.stop_all()
        mock_proc1.terminate.assert_called_once()
        mock_proc2.terminate.assert_not_called()
        assert len(player._active_processes) == 0


def test_play_mpv_with_device_parameter(tmp_path):
    audio_file = tmp_path / "sample.mp3"
    audio_file.write_bytes(b"mock audio data")

    mock_proc = MagicMock(spec=subprocess.Popen)
    mock_proc.poll.return_value = 0

    with (
        patch("shutil.which", return_value="/usr/bin/mpv"),
        patch("subprocess.Popen", return_value=mock_proc) as mock_popen,
    ):
        player = AudioPlayer()
        player.play(audio_file, device="alsa/hdmi:CARD=NVidia,DEV=0")

        mock_popen.assert_called_once_with(
            ["mpv", "--no-terminal", "--no-video", "--audio-device=alsa/hdmi:CARD=NVidia,DEV=0", str(audio_file)],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )


def test_play_mpv_with_env_device(tmp_path, monkeypatch):
    monkeypatch.setenv("AUDIO_OUTPUT_DEVICE", "pulse/alsa_output.pci-0000_00_1f.3.analog-stereo")
    audio_file = tmp_path / "sample.mp3"
    audio_file.write_bytes(b"mock audio data")

    mock_proc = MagicMock(spec=subprocess.Popen)
    mock_proc.poll.return_value = 0

    with (
        patch("shutil.which", return_value="/usr/bin/mpv"),
        patch("subprocess.Popen", return_value=mock_proc) as mock_popen,
    ):
        player = AudioPlayer()
        player.play(audio_file)

        mock_popen.assert_called_once_with(
            ["mpv", "--no-terminal", "--no-video", "--audio-device=pulse/alsa_output.pci-0000_00_1f.3.analog-stereo", str(audio_file)],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )


def test_play_pw_play_with_device_parameter(tmp_path):
    audio_file = tmp_path / "sample.wav"
    audio_file.write_bytes(b"mock audio data")

    mock_proc = MagicMock(spec=subprocess.Popen)
    mock_proc.poll.return_value = 0

    with (
        patch("shutil.which", return_value="/usr/bin/pw-play"),
        patch("subprocess.Popen", return_value=mock_proc) as mock_popen,
    ):
        player = AudioPlayer(backend="pw-play")
        player.play(audio_file, device="alsa_output.usb-device")

        mock_popen.assert_called_once_with(
            ["pw-play", "--target=alsa_output.usb-device", str(audio_file)],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )


def test_play_pw_play_with_env_device(tmp_path, monkeypatch):
    monkeypatch.setenv("AUDIO_OUTPUT_DEVICE", "42")
    audio_file = tmp_path / "sample.wav"
    audio_file.write_bytes(b"mock audio data")

    mock_proc = MagicMock(spec=subprocess.Popen)
    mock_proc.poll.return_value = 0

    with (
        patch("shutil.which", return_value="/usr/bin/pw-play"),
        patch("subprocess.Popen", return_value=mock_proc) as mock_popen,
    ):
        player = AudioPlayer(backend="pw-play")
        player.play(audio_file)

        mock_popen.assert_called_once_with(
            ["pw-play", "--target=42", str(audio_file)],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )

