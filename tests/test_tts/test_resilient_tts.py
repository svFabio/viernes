from pathlib import Path
from unittest.mock import MagicMock

import pytest

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


def test_empty_text_raises_value_error():
    primary = MagicMock(spec=TTSProvider)
    fallback = MagicMock(spec=TTSProvider)
    resilient = ResilientTTS(primary=primary, fallback=fallback)

    with pytest.raises(ValueError, match="Text cannot be empty"):
        resilient.synthesize("")

    with pytest.raises(ValueError, match="Text cannot be empty"):
        resilient.synthesize("   \t  ")

    primary.synthesize.assert_not_called()
    fallback.synthesize.assert_not_called()


def test_fallback_on_quota_exceeded():
    primary = MagicMock(spec=TTSProvider)
    fallback = MagicMock(spec=TTSProvider)
    output_path = Path("/tmp/test.mp3")

    # Simulate ElevenLabs 429 quota error
    primary.synthesize.side_effect = RuntimeError("Status 429: quota_exceeded, insufficient credits")
    fallback.synthesize.return_value = output_path

    on_fallback = MagicMock()
    resilient = ResilientTTS(primary=primary, fallback=fallback, on_fallback_activated=on_fallback)

    # First call: hits primary, detects 429, opens circuit, executes callback, falls back
    result = resilient.synthesize("First message", output_path)

    assert result == output_path
    assert resilient.quota_exhausted is True
    on_fallback.assert_called_once()
    fallback.synthesize.assert_called_once_with("First message", output_path)

    # Second call: bypasses primary completely
    resilient.synthesize("Second message", output_path)
    assert primary.synthesize.call_count == 1  # Not called again!
    assert fallback.synthesize.call_count == 2
    assert on_fallback.call_count == 1  # Callback not called again


def test_transient_error_does_not_open_circuit():
    primary = MagicMock(spec=TTSProvider)
    fallback = MagicMock(spec=TTSProvider)
    output_path = Path("/tmp/test.mp3")

    # Transient network error (not quota related)
    primary.synthesize.side_effect = TimeoutError("Temporary socket timeout")
    fallback.synthesize.return_value = output_path

    on_fallback = MagicMock()
    resilient = ResilientTTS(primary=primary, fallback=fallback, on_fallback_activated=on_fallback)

    result = resilient.synthesize("Transient issue message", output_path)

    assert result == output_path
    assert resilient.quota_exhausted is False
    on_fallback.assert_not_called()
    fallback.synthesize.assert_called_once_with("Transient issue message", output_path)

    # Next call should still attempt primary
    primary.synthesize.side_effect = None
    primary.synthesize.return_value = output_path
    resilient.synthesize("Second call succeeds", output_path)
    assert primary.synthesize.call_count == 2


def test_callback_error_does_not_block_fallback():
    primary = MagicMock(spec=TTSProvider)
    fallback = MagicMock(spec=TTSProvider)
    output_path = Path("/tmp/test.mp3")

    primary.synthesize.side_effect = RuntimeError("rate limit exceeded (429)")
    fallback.synthesize.return_value = output_path

    broken_callback = MagicMock(side_effect=Exception("Logger crashed"))
    resilient = ResilientTTS(primary=primary, fallback=fallback, on_fallback_activated=broken_callback)

    result = resilient.synthesize("Message with broken callback", output_path)
    assert result == output_path
    assert resilient.quota_exhausted is True
    fallback.synthesize.assert_called_once()
