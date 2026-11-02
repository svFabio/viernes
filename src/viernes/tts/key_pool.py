import logging
import threading
from collections.abc import Callable
from pathlib import Path
from typing import Any

from viernes.config import mask_secret
from viernes.tts.base import TTSProvider
from viernes.tts.elevenlabs_provider import ElevenLabsProvider
from viernes.tts.resilient import QUOTA_ERROR_KEYWORDS

logger = logging.getLogger(__name__)

# Additional quota/billing error keywords
POOL_QUOTA_KEYWORDS = tuple(
    set(QUOTA_ERROR_KEYWORDS)
    | {
        "paid_plan_required",
        "quota_exceeded",
        "exceeded",
        "out of credits",
        "insufficient_credits",
        "402",
        "free tier limit",
    }
)


class KeyPoolElevenLabsProvider(TTSProvider):
    """ElevenLabs TTS provider with sequential multi-account credential failover.

    Maintains a pool of ElevenLabs API keys. When an active key exhausts its free credits
    or hits a quota/billing limit (e.g. HTTP 402/429), it automatically rotates to the next
    available key in the pool and retries the synthesis seamlessly.

    Only when all keys in the pool are exhausted is a quota error propagated upward to
    trigger the Circuit Breaker to local Piper.
    """

    def __init__(
        self,
        api_keys: list[str],
        voice_id: str = "cgSgspJ2msm6clMCkdW9",
        model_id: str = "eleven_turbo_v2_5",
        temp_dir: Path | None = None,
        provider_factory: Callable[[str, str], TTSProvider] | None = None,
    ) -> None:
        self.api_keys: list[str] = [k.strip() for k in api_keys if k.strip()]
        self.voice_id = voice_id
        self.model_id = model_id
        self.temp_dir = temp_dir
        self._provider_factory = provider_factory or self._default_provider_factory

        self._active_index: int = 0
        self._exhausted_indices: set[int] = set()
        self._providers: dict[int, TTSProvider] = {}
        self._lock = threading.Lock()

        if not self.api_keys:
            raise ValueError("KeyPoolElevenLabsProvider requires at least one valid API key.")

    def _default_provider_factory(self, api_key: str, voice_id: str) -> TTSProvider:
        return ElevenLabsProvider(
            api_key=api_key,
            voice_id=voice_id,
            model_id=self.model_id,
            temp_dir=self.temp_dir,
        )

    def _get_provider_for_index(self, index: int) -> TTSProvider:
        if index not in self._providers:
            key = self.api_keys[index]
            self._providers[index] = self._provider_factory(key, self.voice_id)
        return self._providers[index]

    @property
    def active_key_index(self) -> int:
        with self._lock:
            return self._active_index

    @property
    def total_keys_count(self) -> int:
        return len(self.api_keys)

    @property
    def remaining_keys_count(self) -> int:
        with self._lock:
            return len(self.api_keys) - len(self._exhausted_indices)

    def is_quota_error(self, exc: Exception) -> bool:
        err_msg = str(exc).lower()
        return any(kw in err_msg for kw in POOL_QUOTA_KEYWORDS)

    def synthesize(self, text: str, output_path: Path | None = None) -> Path:
        """Synthesizes speech using the current active key, rotating on quota exhaustion."""
        if not text or not text.strip():
            raise ValueError("Text cannot be empty")

        with self._lock:
            while self._active_index < len(self.api_keys):
                idx = self._active_index
                masked = mask_secret(self.api_keys[idx])

                try:
                    provider = self._get_provider_for_index(idx)
                    logger.debug(
                        "Attempting synthesis with ElevenLabs key [%d/%d] (%s)",
                        idx + 1,
                        len(self.api_keys),
                        masked,
                    )
                    return provider.synthesize(text, output_path)
                except Exception as exc:
                    if self.is_quota_error(exc):
                        logger.warning(
                            "ElevenLabs key [%d/%d] (%s) quota/credits exhausted: %s. Rotating to next key...",
                            idx + 1,
                            len(self.api_keys),
                            masked,
                            exc,
                        )
                        self._exhausted_indices.add(idx)
                        self._active_index += 1
                        continue

                    # Non-quota error (e.g. network disconnect, invalid input) - propagate immediately
                    logger.error(
                        "Non-quota error encountered on ElevenLabs key [%d/%d] (%s): %s",
                        idx + 1,
                        len(self.api_keys),
                        masked,
                        exc,
                    )
                    raise

            # All keys in pool exhausted
            logger.error(
                "All %d ElevenLabs API keys in the credential pool have been exhausted.",
                len(self.api_keys),
            )
            raise RuntimeError(
                f"Quota exceeded: all {len(self.api_keys)} ElevenLabs API keys in credential pool are exhausted."
            )
