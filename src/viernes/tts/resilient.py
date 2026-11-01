import logging
from pathlib import Path
from typing import Callable, Optional
from viernes.tts.base import TTSProvider

logger = logging.getLogger(__name__)

QUOTA_ERROR_KEYWORDS = (
    "quota",
    "rate limit",
    "429",
    "payment_required",
    "credits",
    "unusual_activity",
    "quota_exceeded",
    "insufficient",
)


class ResilientTTS(TTSProvider):
    """Resilient TTS implementing Circuit Breaker and Fallback patterns.

    Attempts synthesis using the primary provider (e.g. ElevenLabs).
    If quota or rate limits are reached, it automatically switches permanently
    (or per-session) to the fallback local provider (e.g. Piper).
    """

    def __init__(
        self,
        primary: TTSProvider,
        fallback: TTSProvider,
        on_fallback_activated: Optional[Callable[[Exception], None]] = None,
    ):
        self.primary = primary
        self.fallback = fallback
        self.on_fallback_activated = on_fallback_activated
        self.quota_exhausted: bool = False

    def synthesize(self, text: str, output_path: Path | None = None) -> Path:
        if not text or not text.strip():
            raise ValueError("Text cannot be empty")

        if not self.quota_exhausted:
            try:
                logger.info("Attempting synthesis with primary TTS provider.")
                return self.primary.synthesize(text, output_path)
            except Exception as exc:
                err_str = str(exc).lower()
                is_quota_error = any(kw in err_str for kw in QUOTA_ERROR_KEYWORDS)

                if is_quota_error:
                    logger.warning(
                        "Primary TTS quota limit reached: %s. Switching permanently to fallback provider.",
                        exc,
                    )
                    self.quota_exhausted = True
                    if self.on_fallback_activated:
                        try:
                            self.on_fallback_activated(exc)
                        except Exception as cb_exc:
                            logger.error("Error in on_fallback_activated callback: %s", cb_exc)
                else:
                    logger.error(
                        "Primary TTS failed with transient error: %s. Routing this call to fallback.",
                        exc,
                    )

        logger.info("Synthesizing audio using fallback local provider.")
        return self.fallback.synthesize(text, output_path)
