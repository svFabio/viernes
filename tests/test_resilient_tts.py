from pathlib import Path
from unittest.mock import MagicMock

from viernes.tts.base import TTSProvider
from viernes.tts.resilient import ResilientTTS


def test_primary_success():
    primary = MagicMock(spec=TTSProvider)
    fallback = MagicMock(spec=TTSProvider)
    output_path = Path("/tmp/test.mp3")

    primary.synthesize.return_value = output_path
    resilient = ResilientTTS(primary=primary, fallback=fallback)

    result = resilient.synthesize("Hello world", output_path)

    assert result == output_path
    primary.synthesize.assert_called_once_with("Hello world", output_path)
    fallback.synthesize.assert_not_called()
    assert not resilient.quota_exhausted


def test_fallback_on_quota_exceeded():
    primary = MagicMock(spec=TTSProvider)
    fallback = MagicMock(spec=TTSProvider)
    output_path = Path("/tmp/test.mp3")

    # Simulate ElevenLabs 429 quota error
    primary.synthesize.side_effect = RuntimeError("Status 429: quota_exceeded, insufficient credits")
    fallback.synthesize.return_value = output_path

    on_fallback = MagicMock()
    resilient = ResilientTTS(primary=primary, fallback=fallback, on_fallback_activated=on_fallback)

    # First call: should hit primary, catch quota, switch to fallback
    result = resilient.synthesize("First message", output_path)

    assert result == output_path
    assert resilient.quota_exhausted is True
    on_fallback.assert_called_once()
    fallback.synthesize.assert_called_once_with("First message", output_path)

    # Second call: should bypass primary completely and call fallback directly
    resilient.synthesize("Second message", output_path)
    assert primary.synthesize.call_count == 1  # Not called again!
    assert fallback.synthesize.call_count == 2
