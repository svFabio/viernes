import sys
from unittest.mock import MagicMock

# Ensure test collection and execution succeeds in headless CI runners
# where no physical audio hardware or PulseAudio/PipeWire server is active.
try:
    import sounddevice as _sd
except Exception:
    mock_sd = MagicMock()
    mock_sd.default.device = (0, 0)
    mock_sd.query_devices.return_value = [
        {
            "name": "Dummy Input",
            "index": 0,
            "max_input_channels": 2,
            "max_output_channels": 0,
            "default_samplerate": 16000.0,
        },
        {
            "name": "Dummy Output",
            "index": 1,
            "max_input_channels": 0,
            "max_output_channels": 2,
            "default_samplerate": 44100.0,
        },
    ]
    mock_sd.PortAudioError = type("PortAudioError", (Exception,), {})
    mock_sd.CallbackStop = type("CallbackStop", (Exception,), {})
    mock_sd.CallbackAbort = type("CallbackAbort", (Exception,), {})
    sys.modules["sounddevice"] = mock_sd
