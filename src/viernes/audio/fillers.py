import hashlib
import json
import logging
import os
import random
import tempfile
from collections.abc import Callable
from pathlib import Path

logger = logging.getLogger(__name__)

DEFAULT_FILLER_PHRASES: list[str] = [
    "Deme un segundo, Señor, ya lo reviso.",
    "Enseguida, Señor, estoy consultando información.",
    "Un momento, Señor.",
    "Revisando sus registros, deme un segundo.",
    "Lo escucho, Señor, un segundo.",
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

PATIENCE_FILLER_PHRASES: list[str] = [
    "Señor, demoraré un momento en responder, por favor espéreme.",
    "Esta operación tomará unos segundos más, Señor, por favor espéreme.",
]

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


def get_default_fillers_config_file() -> Path:
    """Returns standard path to the custom filler phrases JSON configuration."""
    override = os.getenv("VIERNES_FILLERS_FILE")
    if override:
        return Path(override)
    return Path.home() / ".config" / "viernes" / "fillers.json"


def load_custom_fillers(
    config_path: Path | None = None,
) -> tuple[list[str], dict[str, list[str]], list[str]]:
    """Loads configured filler phrases from file or returns standard defaults."""
    path = config_path or get_default_fillers_config_file()
    if not path.is_file():
        return (
            list(DEFAULT_FILLER_PHRASES),
            {k: list(v) for k, v in TOOL_FILLER_PHRASES.items()},
            list(PATIENCE_FILLER_PHRASES),
        )

    try:
        with open(path, encoding="utf-8") as f:
            data = json.load(f)

        raw_generic = data.get("generic")
        generic = (
            [p.strip() for p in raw_generic if isinstance(p, str) and p.strip()]
            if isinstance(raw_generic, list)
            else list(DEFAULT_FILLER_PHRASES)
        )

        raw_patience = data.get("patience")
        patience = (
            [p.strip() for p in raw_patience if isinstance(p, str) and p.strip()]
            if isinstance(raw_patience, list)
            else list(PATIENCE_FILLER_PHRASES)
        )

        raw_tools = data.get("tools")
        tools: dict[str, list[str]] = {}
        if isinstance(raw_tools, dict):
            for cat, cat_list in raw_tools.items():
                if isinstance(cat_list, list):
                    clean_list = [p.strip() for p in cat_list if isinstance(p, str) and p.strip()]
                    if clean_list:
                        tools[str(cat)] = clean_list

        if not tools:
            tools = {k: list(v) for k, v in TOOL_FILLER_PHRASES.items()}

        return (
            generic or list(DEFAULT_FILLER_PHRASES),
            tools,
            patience or list(PATIENCE_FILLER_PHRASES),
        )
    except (json.JSONDecodeError, OSError) as exc:
        logger.warning("Could not read custom fillers config from %s: %s", path, exc)
        return (
            list(DEFAULT_FILLER_PHRASES),
            {k: list(v) for k, v in TOOL_FILLER_PHRASES.items()},
            list(PATIENCE_FILLER_PHRASES),
        )


def save_custom_fillers(
    generic: list[str],
    tool_phrases: dict[str, list[str]],
    patience: list[str],
    config_path: Path | None = None,
) -> None:
    """Atomically persists custom filler phrases to JSON configuration file."""
    path = config_path or get_default_fillers_config_file()
    path.parent.mkdir(parents=True, exist_ok=True)

    data = {
        "generic": [p.strip() for p in generic if p.strip()],
        "patience": [p.strip() for p in patience if p.strip()],
        "tools": {
            cat: [p.strip() for p in phrases if p.strip()] for cat, phrases in tool_phrases.items()
        },
    }

    with tempfile.NamedTemporaryFile(
        mode="w",
        dir=path.parent,
        encoding="utf-8",
        delete=False,
    ) as temp_file:
        temp_path = Path(temp_file.name)
        try:
            json.dump(data, temp_file, indent=2, ensure_ascii=False)
            temp_file.flush()
            os.fsync(temp_file.fileno())
        except Exception:
            if temp_path.exists():
                temp_path.unlink()
            raise

    temp_path.replace(path)
    logger.info("Saved custom filler phrases to %s", path)


class FillerAudioCache:
    """Pre-caches and instantly serves conversational verbal acknowledgments (fillers).

    Masks brain thinking latency by immediately playing natural acknowledgments
    like 'Deme un segundo, Señor...' within < 200ms of user finishing speech,
    as well as context-aware tool acknowledgments and patience reminders for longer tasks.
    """

    def __init__(
        self,
        cache_dir: Path | None = None,
        phrases: list[str] | None = None,
        tool_phrases: dict[str, list[str]] | None = None,
        patience_phrases: list[str] | None = None,
        config_path: Path | None = None,
    ) -> None:
        self.cache_dir = cache_dir or Path("/tmp/viernes/fillers")
        self.cache_dir.mkdir(parents=True, exist_ok=True)
        self.config_path = config_path or get_default_fillers_config_file()

        if phrases is None and tool_phrases is None and patience_phrases is None:
            custom_generic, custom_tools, custom_patience = load_custom_fillers(self.config_path)
            self.phrases = custom_generic
            self.tool_phrases = custom_tools
            self.patience_phrases = custom_patience
        else:
            self.phrases = phrases or DEFAULT_FILLER_PHRASES
            self.tool_phrases = tool_phrases or TOOL_FILLER_PHRASES
            self.patience_phrases = patience_phrases or PATIENCE_FILLER_PHRASES

        self._cached_files: list[Path] = []
        self._cached_tool_files: dict[str, list[Path]] = {}
        self._cached_patience_files: list[Path] = []

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

        self._cached_patience_files = []
        for phrase in self.patience_phrases:
            target = self.get_filler_path_for_phrase(phrase)
            if not target.exists() or target.stat().st_size == 0:
                try:
                    logger.debug("Pre-caching patience filler audio: '%s'", phrase)
                    synthesizer(phrase, target)
                except Exception as exc:
                    logger.warning("Failed pre-caching patience filler '%s': %s", phrase, exc)
                    continue

            if target.exists() and target.stat().st_size > 0:
                self._cached_patience_files.append(target)

        logger.info(
            "Filler audio cache warmed up with %d generic, %d tool, and %d patience phrases.",
            len(self._cached_files),
            sum(len(v) for v in self._cached_tool_files.values()),
            len(self._cached_patience_files),
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
            phrases = self.tool_phrases.get(category, [])
            for phrase in phrases:
                p = self.get_filler_path_for_phrase(phrase)
                if p.is_file() and p.stat().st_size > 0:
                    available.append(p)
            self._cached_tool_files[category] = available

        if not available:
            return None
        return random.choice(available)

    def get_patience_filler(self) -> Path | None:
        """Returns a pre-cached patience reminder audio path for longer operations."""
        available = [f for f in self._cached_patience_files if f.is_file() and f.stat().st_size > 0]
        if not available:
            for phrase in self.patience_phrases:
                p = self.get_filler_path_for_phrase(phrase)
                if p.is_file() and p.stat().st_size > 0:
                    available.append(p)
            self._cached_patience_files = available

        if not available:
            return None
        return random.choice(available)
