from pathlib import Path
from unittest.mock import MagicMock
import numpy as np
import pytest

from viernes.tts.base import TTSProvider
from viernes.tts.resilient import ResilientTTS
from viernes.tts.piper_provider import PiperProvider
from viernes.tools.security import (
    DEFAULT_ALLOWLIST,
    FORBIDDEN_TOOLS,
    SecurityGate,
    SecurityPolicy,
    SecurityViolationError,
    sanitize_params,
)
from viernes.audio.vad import VoiceActivityDetector, VADState


def test_adversarial_tts_circuit_breaker_stress():
    """Simulates consecutive HTTP 429 quota exhaustion errors."""
    primary = MagicMock(spec=TTSProvider)
    fallback = MagicMock(spec=TTSProvider)
    output_path = Path("/tmp/test_stress.wav")

    primary.synthesize.side_effect = RuntimeError(
        "429 Too Many Requests: quota_exceeded, payment_required"
    )
    fallback.synthesize.return_value = output_path
    on_fallback = MagicMock()

    resilient = ResilientTTS(primary=primary, fallback=fallback, on_fallback_activated=on_fallback)

    # First attempt: catches 429, triggers callback, switches to fallback
    res = resilient.synthesize("Hello 1", output_path)
    assert res == output_path
    assert resilient.quota_exhausted is True
    on_fallback.assert_called_once()
    assert primary.synthesize.call_count == 1
    assert fallback.synthesize.call_count == 1

    # Subsequent 10 attempts: must NEVER touch primary again
    for i in range(10):
        resilient.synthesize(f"Hello {i + 2}", output_path)

    assert primary.synthesize.call_count == 1
    assert fallback.synthesize.call_count == 11


def test_deep_security_parameter_sanitization():
    """Validates that deeply nested sensitive keys are masked in audit logs."""
    raw_payload = {
        "service": "elevenlabs",
        "credentials": {
            "api_key": "sk_live_1234567890abcdef",
            "nested": {
                "token": "bearer_secret_xyz",
                "password": "super_secret_pass",
            },
        },
        "headers": [
            {"Authorization": "Bearer confidential"},
            {"public_header": "ok"},
        ],
    }

    sanitized = sanitize_params(raw_payload)

    assert sanitized["credentials"]["api_key"] == "[REDACTED]"
    assert sanitized["credentials"]["nested"]["token"] == "[REDACTED]"
    assert sanitized["credentials"]["nested"]["password"] == "[REDACTED]"
    assert sanitized["headers"][0]["Authorization"] == "[REDACTED]"
    assert sanitized["headers"][1]["public_header"] == "ok"


def test_security_rejection_forbidden_tools():
    """Ensures forbidden command execution primitives are strictly blocked."""
    policy = SecurityPolicy()

    for forbidden in FORBIDDEN_TOOLS:
        assert not policy.is_tool_allowed(forbidden)
        with pytest.raises(SecurityViolationError):
            policy.validate_tool(forbidden)


def test_vad_noise_immunity():
    """Verifies that high-frequency noise and low-energy clicks do not trigger speech."""
    vad = VoiceActivityDetector(sample_rate=16000, threshold=0.5)

    # Pure silence
    silent_chunk = np.zeros(480, dtype=np.int16)
    res = vad.process_chunk(silent_chunk)
    assert res.is_speech is False
    assert res.state == VADState.SILENCE

    # Very low level white noise
    low_noise = np.random.randint(-15, 15, size=480, dtype=np.int16)
    res_noise = vad.process_chunk(low_noise)
    assert res_noise.is_speech is False
    assert res_noise.state == VADState.SILENCE


def test_piper_missing_executable():
    """Ensures missing binary throws clean FileNotFoundError."""
    piper = PiperProvider(piper_bin="nonexistent_binary_xyz_123")
    with pytest.raises(FileNotFoundError, match="not found in PATH"):
        piper.synthesize("Valid text", Path("/tmp/out.wav"))
