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
            return sentences

        accumulated_clause = ""
        idx = 0

        while idx < len(parts) - 1:
            clause = parts[idx]
            delimiter = parts[idx + 1]
            candidate = (accumulated_clause + clause + delimiter).strip()

            words = candidate.split()
            last_word = words[-1].lower() if words else ""

            # Check if delimiter belongs to an abbreviation (e.g. Dr.)
            if last_word in COMMON_ABBREVIATIONS:
                accumulated_clause += clause + delimiter
                idx += 2
                continue

            # Check minimum word length if more parts follow
            if len(words) < self.min_words and idx + 2 < len(parts):
                accumulated_clause += clause + delimiter
                idx += 2
                continue

            sentences.append(candidate)
            accumulated_clause = ""
            idx += 2

        # Put back remainder plus any un-emitted accumulated clause
        remainder = accumulated_clause + "".join(parts[idx:])
        self._buffer = remainder
        return sentences
