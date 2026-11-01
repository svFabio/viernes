import re
from collections.abc import AsyncIterator

# Common abbreviations in Spanish that should not split sentences
COMMON_ABBREVIATIONS = {
    "dr.",
    "dra.",
    "sr.",
    "sra.",
    "srta.",
    "prof.",
    "ej.",
    "p.ej.",
    "etc.",
    "vs.",
    "art.",
    "cap.",
    "pag.",
    "pág.",
}

PUNCTUATION_SPLIT_REGEX = re.compile(r"([.!?\n]+)")


class SentenceTokenizer:
    """Buffers streamed tokens and yields complete sentences on punctuation boundaries."""

    def __init__(self, min_words_per_sentence: int = 2) -> None:
        self.min_words = min_words_per_sentence
        self._buffer: str = ""

    def process_token(self, token: str) -> list[str]:
        """Appends a token to the internal buffer and returns any newly completed sentences."""
        self._buffer += token
        return self._extract_sentences()

    def flush(self) -> list[str]:
        """Flushes and returns any remaining text in the buffer as the final sentence."""
        remaining = self._buffer.strip()
        self._buffer = ""
        if remaining:
            return [remaining]
        return []

    async def tokenize_stream(self, token_stream: AsyncIterator[str]) -> AsyncIterator[str]:
        """Asynchronously converts a stream of text chunks/tokens into a stream of sentences."""
        async for token in token_stream:
            sentences = self.process_token(token)
            for sentence in sentences:
                yield sentence

        for final_sentence in self.flush():
            yield final_sentence

    def _extract_sentences(self) -> list[str]:
        sentences: list[str] = []

        # Split preserving punctuation delimiters
        parts = PUNCTUATION_SPLIT_REGEX.split(self._buffer)
        if len(parts) < 3:
            # Need at least: [text_before, delimiter, text_after]
            return sentences

        # Iterate pairs of (clause, delimiter)
        idx = 0
        while idx < len(parts) - 1:
            clause = parts[idx]
            delimiter = parts[idx + 1]
            candidate = (clause + delimiter).strip()

            # Check if delimiter is part of an abbreviation
            last_word = candidate.split()[-1].lower() if candidate.split() else ""
            if last_word in COMMON_ABBREVIATIONS:
                # Keep accumulating, don't split yet
                idx += 2
                continue

            # Ensure minimum word length
            if len(candidate.split()) < self.min_words and idx + 2 < len(parts):
                idx += 2
                continue

            sentences.append(candidate)
            idx += 2

        # Retain remainder in buffer
        remainder = "".join(parts[idx:])
        self._buffer = remainder
        return sentences
