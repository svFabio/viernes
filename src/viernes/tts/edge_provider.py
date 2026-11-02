import asyncio
import concurrent.futures
import logging
from pathlib import Path

import edge_tts

from viernes.tts.base import TTSProvider

logger = logging.getLogger(__name__)

DEFAULT_EDGE_VOICE = "es-CO-SalomeNeural"


class EdgeTTSProvider(TTSProvider):
    """Microsoft Neural TTS provider using edge-tts.

    Provides high-quality, zero-cost, unlimited neural voices
    with native Latin American accents (e.g. es-CO-SalomeNeural).
    """

    def __init__(
        self,
        voice: str = DEFAULT_EDGE_VOICE,
        temp_dir: Path | None = None,
    ) -> None:
        self.voice = voice
        self.temp_dir = temp_dir or Path("/tmp/viernes")
        self.temp_dir.mkdir(parents=True, exist_ok=True)

    def synthesize(self, text: str, output_path: Path | None = None) -> Path:
        if not text or not text.strip():
            raise ValueError("Text cannot be empty for synthesis.")

        out = output_path or (self.temp_dir / f"edge_{abs(hash(text))}.mp3")
        out.parent.mkdir(parents=True, exist_ok=True)

        async def _run() -> None:
            communicate = edge_tts.Communicate(text, self.voice)
            await communicate.save(str(out))

        try:
            loop = asyncio.get_running_loop()
        except RuntimeError:
            loop = None

        if loop is not None and loop.is_running():
            with concurrent.futures.ThreadPoolExecutor(max_workers=1) as pool:
                pool.submit(lambda: asyncio.run(_run())).result()
        else:
            asyncio.run(_run())

        if not out.exists() or out.stat().st_size == 0:
            raise RuntimeError(f"EdgeTTS failed to generate audio file at {out}")

        logger.info("Synthesized audio with EdgeTTS (%s): %s", self.voice, out)
        return out
