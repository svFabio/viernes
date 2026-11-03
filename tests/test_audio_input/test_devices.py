from unittest.mock import patch

from viernes.audio.devices import (
    list_audio_devices,
    print_audio_devices,
    resolve_audio_device,
)
from viernes.audio.recorder import AudioRecorder

MOCK_DEVICES = [
    {
        "index": 0,
        "name": "Built-in Microphone",
        "max_input_channels": 2,
        "max_output_channels": 0,
        "default_samplerate": 44100.0,
        "is_default_input": True,
        "is_default_output": False,
    },
    {
        "index": 1,
        "name": "USB Gaming Headset",
        "max_input_channels": 1,
        "max_output_channels": 2,
        "default_samplerate": 48000.0,
        "is_default_input": False,
        "is_default_output": True,
    },
    {
        "index": 2,
        "name": "HDMI Stereo Out",
        "max_input_channels": 0,
        "max_output_channels": 2,
        "default_samplerate": 48000.0,
        "is_default_input": False,
        "is_default_output": False,
    },
]


def test_list_audio_devices_mocked():
    with (
        patch("sounddevice.query_devices", return_value=MOCK_DEVICES),
        patch("sounddevice.default.device", (0, 1)),
    ):
        devices = list_audio_devices()
        assert len(devices) == 3
        assert devices[0]["name"] == "Built-in Microphone"
        assert devices[0]["is_default_input"] is True


def test_print_audio_devices_runs(capsys):
    with patch("viernes.audio.devices.list_audio_devices", return_value=MOCK_DEVICES):
        print_audio_devices()
        captured = capsys.readouterr()
        assert "Available Audio Input Devices (Microphones):" in captured.out
        assert "[0] Built-in Microphone" in captured.out
        assert "Available Audio Output Devices" in captured.out
        assert "[1] USB Gaming Headset" in captured.out


def test_resolve_audio_device_default_and_none(monkeypatch):
    monkeypatch.delenv("AUDIO_INPUT_DEVICE", raising=False)
    monkeypatch.delenv("AUDIO_OUTPUT_DEVICE", raising=False)
    assert resolve_audio_device(None, devices=MOCK_DEVICES) is None
    assert resolve_audio_device("", devices=MOCK_DEVICES) is None
    assert resolve_audio_device("default", devices=MOCK_DEVICES) is None
    assert resolve_audio_device("auto", devices=MOCK_DEVICES) is None


def test_resolve_audio_device_by_index():
    # Valid input device index 0
    assert resolve_audio_device(0, kind="input", devices=MOCK_DEVICES) == 0
    assert resolve_audio_device("1", kind="input", devices=MOCK_DEVICES) == 1

    # Device index 2 has 0 input channels -> fallback to None with warning
    assert resolve_audio_device(2, kind="input", devices=MOCK_DEVICES) is None

    # Out of bounds index -> fallback to None with warning
    assert resolve_audio_device(99, kind="input", devices=MOCK_DEVICES) is None


def test_resolve_audio_device_by_substring():
    # "usb" matches "USB Gaming Headset"
    assert resolve_audio_device("usb", kind="input", devices=MOCK_DEVICES) == 1
    assert resolve_audio_device("gaming", kind="input", devices=MOCK_DEVICES) == 1
    assert resolve_audio_device("built-in", kind="input", devices=MOCK_DEVICES) == 0

    # Non-existent name -> fallback to None with warning
    assert resolve_audio_device("bluetooth-mic", kind="input", devices=MOCK_DEVICES) is None


def test_resolve_audio_device_from_env(monkeypatch):
    monkeypatch.setenv("AUDIO_INPUT_DEVICE", "gaming")
    assert resolve_audio_device(kind="input", devices=MOCK_DEVICES) == 1

    monkeypatch.setenv("AUDIO_INPUT_DEVICE", "99")
    assert resolve_audio_device(kind="input", devices=MOCK_DEVICES) is None


def test_recorder_config_from_env(monkeypatch, tmp_path):
    monkeypatch.setenv("VAD_THRESHOLD", "0.75")
    monkeypatch.setenv("SILENCE_TIMEOUT_MS", "1500")
    monkeypatch.setenv("AUDIO_INPUT_DEVICE", "gaming")

    with patch("viernes.audio.devices.list_audio_devices", return_value=MOCK_DEVICES):
        recorder = AudioRecorder(temp_dir=tmp_path)
        assert recorder.vad_threshold == 0.75
        assert recorder.silence_timeout_ms == 1500
        assert recorder.device == 1


def test_recorder_config_out_of_range_env_clamping(monkeypatch, tmp_path):
    # VAD_THRESHOLD out of range -> clamps to 0.5
    monkeypatch.setenv("VAD_THRESHOLD", "1.5")
    # SILENCE_TIMEOUT_MS out of range (< 300) -> clamps to 800
    monkeypatch.setenv("SILENCE_TIMEOUT_MS", "100")

    recorder = AudioRecorder(temp_dir=tmp_path)
    assert recorder.vad_threshold == 0.5
    assert recorder.silence_timeout_ms == 800
