import hashlib
import logging
import random
from collections.abc import Callable
from pathlib import Path

logger = logging.getLogger(__name__)

DEFAULT_FILLER_PHRASES: list[str] = [
    "Deme un segundo, Señor, ya lo reviso.",
    "Enseguida, Señor, consultando el sistema.",
    "Un momento, Señor, permítame verificar.",
    "Revisando sus registros, deme un segundo.",
    "Entendido, Señor, procesando.",
]

TOOL_FILLER_PHRASES: dict[str, list[str]] = {
    "files": [
        "Permítame revisar los archivos, Señor, un momento.",
        "Buscando en los archivos del proyecto, Señor.",
    ],
    "terminal": [
        "Usaré la terminal para verificar, Señor, un momento.",
        "Ejecutando la consulta en la terminal, Señor.",
    ],
    "search": [
        "Buscando la información en la red, Señor, un instante.",
    ],
}

TOOL_CATEGORY_MAP: dict[str, str] = {
    "read_file": "files",
    "view_file": "files",
    "grep_search": "files",
    "find_by_name": "files",
    "list_dir": "files",
    "run_command": "terminal",
    "send_command_input": "terminal",
    "command_status": "terminal",
    "search_web": "search",
    "search_marketplace": "search",
}


class FillerAudioCache:
    """Pre-caches and instantly serves conversational verbal acknowledgments (fillers).

    Masks brain thinking latency by immediately playing natural acknowledgments
    like 'Deme un segundo, Señor...' within < 200ms of user finishing speech,
    as well as context-aware tool acknowledgments when actions are triggered.
    """

    def __init__(
        self,
        cache_dir: Path | None = None,
        phrases: list[str] | None = None,
        tool_phrases: dict[str, list[str]] | None = None,
    ) -> None:
        self.cache_dir = cache_dir or Path("/tmp/viernes/fillers")
        self.cache_dir.mkdir(parents=True, exist_ok=True)
        self.phrases = phrases or DEFAULT_FILLER_PHRASES
        self.tool_phrases = tool_phrases or TOOL_FILLER_PHRASES
        self._cached_files: list[Path] = []
        self._cached_tool_files: dict[str, list[Path]] = {}

    def get_filler_path_for_phrase(self, phrase: str) -> Path:
        slug = hashlib.md5(phrase.encode("utf-8")).hexdigest()[:8]
        return self.cache_dir / f"filler_{slug}.mp3"

    def prewarm(self, synthesizer: Callable[[str, Path], Path]) -> None:
        """Pre-synthesizes filler phrases in background for zero-latency instant playback."""
        self._cached_files = []
        for phrase in self.phrases:
            target = self.get_filler_path_for_phrase(phrase)
            if not target.exists() or target.stat().st_size == 0:
                try:
                    logger.debug("Pre-caching filler audio: '%s'", phrase)
                    synthesizer(phrase, target)
                except Exception as exc:
                    logger.warning("Failed pre-caching filler phrase '%s': %s", phrase, exc)
                    continue

            if target.exists() and target.stat().st_size > 0:
                self._cached_files.append(target)

        self._cached_tool_files = {}
        for category, cat_phrases in self.tool_phrases.items():
            cat_list: list[Path] = []
            for phrase in cat_phrases:
                target = self.get_filler_path_for_phrase(phrase)
                if not target.exists() or target.stat().st_size == 0:
                    try:
                        logger.debug("Pre-caching tool filler audio (%s): '%s'", category, phrase)
                        synthesizer(phrase, target)
                    except Exception as exc:
                        logger.warning(
                            "Failed pre-caching tool filler '%s' (%s): %s",
                            phrase,
                            category,
                            exc,
                        )
                        continue

                if target.exists() and target.stat().st_size > 0:
                    cat_list.append(target)
            self._cached_tool_files[category] = cat_list

        logger.info(
            "Filler audio cache warmed up with %d generic and %d tool phrases.",
            len(self._cached_files),
            sum(len(v) for v in self._cached_tool_files.values()),
        )

    def get_random_filler(self) -> Path | None:
        """Returns a random pre-cached generic filler audio path, or None if none available."""
        available = [f for f in self._cached_files if f.is_file() and f.stat().st_size > 0]
        if not available:
            for phrase in self.phrases:
                p = self.get_filler_path_for_phrase(phrase)
                if p.is_file() and p.stat().st_size > 0:
                    available.append(p)
            self._cached_files = available

        if not available:
            return None
        return random.choice(available)

    def get_tool_filler(self, tool_name: str) -> Path | None:
        """Returns a context-appropriate filler audio path for a specific tool call."""
        category = TOOL_CATEGORY_MAP.get(tool_name)
        if not category:
            return None

        cat_files = self._cached_tool_files.get(category, [])
        available = [f for f in cat_files if f.is_file() and f.stat().st_size > 0]
        if not available:
            # Fall back to checking on disk
            phrases = self.tool_phrases.get(category, [])
            for phrase in phrases:
                p = self.get_filler_path_for_phrase(phrase)
                if p.is_file() and p.stat().st_size > 0:
                    available.append(p)
            self._cached_tool_files[category] = available

        if not available:
            return None
        return random.choice(available)
