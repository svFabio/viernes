from pathlib import Path
from unittest.mock import MagicMock

import pytest

from viernes.tts.base import TTSProvider
from viernes.tts.key_pool import KeyPoolElevenLabsProvider


class MockProvider(TTSProvider):
    def __init__(self, key: str, should_fail_quota: bool = False, should_fail_other: bool = False):
        self.key = key
        self.should_fail_quota = should_fail_quota
        self.should_fail_other = should_fail_other
        self.call_count = 0

    def synthesize(self, text: str, output_path: Path | None = None) -> Path:
        self.call_count += 1
        if self.should_fail_quota:
            raise RuntimeError("402 payment_required: credits exhausted for free tier")
        if self.should_fail_other:
            raise ConnectionError("Connection refused by peer")
        out = output_path or Path("/tmp/mock.mp3")
        return out


def test_key_pool_empty_keys_raises() -> None:
    with pytest.raises(ValueError, match="requires at least one valid API key"):
        KeyPoolElevenLabsProvider(api_keys=[])


def test_key_pool_single_key_success(tmp_path: Path) -> None:
    target_out = tmp_path / "audio.mp3"
    provider_mock = MockProvider(key="key1")

    pool = KeyPoolElevenLabsProvider(
        api_keys=["key1"],
        provider_factory=lambda key, vid: provider_mock,
    )
    result = pool.synthesize("Hola mundo", target_out)

    assert result == target_out
    assert provider_mock.call_count == 1
    assert pool.active_key_index == 0
    assert pool.remaining_keys_count == 1


def test_key_pool_failover_to_next_key_on_quota(tmp_path: Path) -> None:
    target_out = tmp_path / "audio.mp3"
    p1 = MockProvider(key="key1", should_fail_quota=True)
    p2 = MockProvider(key="key2", should_fail_quota=False)

    factory_map = {"key1": p1, "key2": p2}

    pool = KeyPoolElevenLabsProvider(
        api_keys=["key1", "key2"],
        provider_factory=lambda key, vid: factory_map[key],
    )

    result = pool.synthesize("Texto de prueba", target_out)

    assert result == target_out
    assert p1.call_count == 1
    assert p2.call_count == 1
    assert pool.active_key_index == 1
    assert pool.remaining_keys_count == 1


def test_key_pool_all_exhausted_raises(tmp_path: Path) -> None:
    target_out = tmp_path / "audio.mp3"
    p1 = MockProvider(key="key1", should_fail_quota=True)
    p2 = MockProvider(key="key2", should_fail_quota=True)

    factory_map = {"key1": p1, "key2": p2}

    pool = KeyPoolElevenLabsProvider(
        api_keys=["key1", "key2"],
        provider_factory=lambda key, vid: factory_map[key],
    )

    with pytest.raises(
        RuntimeError,
        match="Quota exceeded: all 2 ElevenLabs API keys in credential pool are exhausted",
    ):
        pool.synthesize("Texto", target_out)

    assert pool.remaining_keys_count == 0


def test_key_pool_non_quota_error_does_not_rotate(tmp_path: Path) -> None:
    target_out = tmp_path / "audio.mp3"
    p1 = MockProvider(key="key1", should_fail_other=True)
    p2 = MockProvider(key="key2", should_fail_quota=False)

    factory_map = {"key1": p1, "key2": p2}

    pool = KeyPoolElevenLabsProvider(
        api_keys=["key1", "key2"],
        provider_factory=lambda key, vid: factory_map[key],
    )

    with pytest.raises(ConnectionError, match="Connection refused"):
        pool.synthesize("Texto", target_out)

    # Key should NOT have rotated because it was a connection error, not quota
    assert pool.active_key_index == 0
    assert pool.remaining_keys_count == 2


def test_key_pool_circuit_breaker_integration(tmp_path: Path) -> None:
    from viernes.tts.resilient import ResilientTTS

    p1 = MockProvider(key="key1", should_fail_quota=True)
    p2 = MockProvider(key="key2", should_fail_quota=True)
    piper_mock = MockProvider(key="piper_local", should_fail_quota=False)

    factory_map = {"key1": p1, "key2": p2}
    pool = KeyPoolElevenLabsProvider(
        api_keys=["key1", "key2"],
        provider_factory=lambda key, vid: factory_map[key],
    )

    resilient = ResilientTTS(primary=pool, fallback=piper_mock)

    out = tmp_path / "piper_out.mp3"
    result = resilient.synthesize("Fallback test", out)

    assert result == out
    assert resilient.quota_exhausted is True
    assert piper_mock.call_count == 1
    assert p1.call_count == 1
    assert p2.call_count == 1
