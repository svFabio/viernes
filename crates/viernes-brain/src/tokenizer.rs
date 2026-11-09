//! Streaming real-time sentence tokenizer for incremental TTS synthesis.

use tokio::sync::mpsc;
use tracing::debug;

use crate::error::BrainError;

/// Minimum character length required before emitting a sentence chunk.
pub const MIN_EMISSION_LENGTH: usize = 10;

/// Minimum character length required before emitting an early first clause chunk.
pub const MIN_FIRST_CLAUSE_LENGTH: usize = 12;

/// Smart sentence tokenizer that buffers incoming streaming tokens and segments by natural clauses.
pub struct SentenceTokenizer {
    buffer: String,
    output_tx: mpsc::Sender<String>,
    first_clause_emitted: bool,
}

impl SentenceTokenizer {
    /// Create a new tokenizer routing segmented sentences to the given channel.
    #[must_use]
    pub fn new(output_tx: mpsc::Sender<String>) -> Self {
        Self {
            buffer: String::new(),
            output_tx,
            first_clause_emitted: false,
        }
    }

    /// Feed an incremental token into the accumulator and emit completed sentences.
    pub async fn feed_token(&mut self, token: &str) -> Result<(), BrainError> {
        self.buffer.push_str(token);

        while let Some(index) = self.find_sentence_boundary() {
            let sentence = self.buffer[..=index].trim().to_string();
            self.buffer = self.buffer[index + 1..].to_string();

            if !sentence.is_empty() {
                self.first_clause_emitted = true;
                debug!(sentence = %sentence, "SentenceTokenizer emitted sentence");
                self.output_tx
                    .send(sentence)
                    .await
                    .map_err(|e| BrainError::Internal(e.to_string()))?;
            }
        }

        Ok(())
    }

    /// Flush any remaining text in the buffer when token stream completes.
    pub async fn flush(&mut self) -> Result<(), BrainError> {
        let remainder = self.buffer.trim().to_string();
        self.buffer.clear();
        self.first_clause_emitted = false;

        if !remainder.is_empty() {
            debug!(remainder = %remainder, "SentenceTokenizer flushed remainder buffer");
            self.output_tx
                .send(remainder)
                .await
                .map_err(|e| BrainError::Internal(e.to_string()))?;
        }

        Ok(())
    }

    /// Inspect buffer to find valid sentence boundary point satisfying defensive rules.
    fn find_sentence_boundary(&self) -> Option<usize> {
        let bytes = self.buffer.as_bytes();
        let len = bytes.len();

        for i in 0..len {
            let b = bytes[i];
            let is_sentence_delimiter = matches!(b, b'.' | b'?' | b'!' | b'\n');
            let is_early_clause = !self.first_clause_emitted && matches!(b, b',' | b';' | b':');

            if is_sentence_delimiter || is_early_clause {
                let min_len = if is_early_clause {
                    MIN_FIRST_CLAUSE_LENGTH
                } else {
                    MIN_EMISSION_LENGTH
                };

                // Defensive check 1: Minimum character threshold
                if i + 1 < min_len {
                    continue;
                }

                // Defensive check 2: Decimal number protection (e.g. 3.14 or 3,14)
                if (b == b'.' || b == b',') && i > 0 && i + 1 < len {
                    let prev = bytes[i - 1];
                    let next = bytes[i + 1];
                    if prev.is_ascii_digit() && next.is_ascii_digit() {
                        continue;
                    }
                }

                // Defensive check 3: Common Spanish and English abbreviations for '.'
                if b == b'.' {
                    let slice = &self.buffer[..=i];
                    if is_common_abbreviation(slice) {
                        continue;
                    }
                }

                // Defensive check 4: If delimiter is followed immediately by another delimiter (e.g. "...", "?!"), wait
                if i + 1 < len && matches!(bytes[i + 1], b'.' | b'?' | b'!' | b')' | b'"' | b',') {
                    continue;
                }

                // Defensive check 5: For early clause delimiters, ensure it is followed by whitespace or at end of buffer
                if is_early_clause && i + 1 < len && !bytes[i + 1].is_ascii_whitespace() {
                    continue;
                }

                return Some(i);
            }
        }

        None
    }
}

/// Helper identifying whether a period ends a known abbreviation rather than a sentence.
fn is_common_abbreviation(slice: &str) -> bool {
    let lower = slice.to_lowercase();
    let words = lower.split_whitespace().collect::<Vec<_>>();
    if let Some(last_word) = words.last() {
        let stripped = last_word.trim();
        matches!(
            stripped,
            "sr."
                | "sra."
                | "srta."
                | "dr."
                | "dra."
                | "ing."
                | "lic."
                | "etc."
                | "ej."
                | "p.ej."
                | "vs."
                | "prof."
                | "mr."
                | "mrs."
                | "ms."
        )
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_basic_token_segmentation() {
        let (tx, mut rx) = mpsc::channel(8);
        let mut tokenizer = SentenceTokenizer::new(tx);

        let tokens = vec![
            "Hola ",
            "Señor. ",
            "¿Cómo ",
            "se ",
            "encuentra ",
            "el ",
            "día ",
            "de ",
            "hoy? ",
            "Todo ",
            "está ",
            "funcionando ",
            "correctamente.",
        ];

        for t in tokens {
            tokenizer.feed_token(t).await.unwrap();
        }
        tokenizer.flush().await.unwrap();

        let s1 = rx.recv().await.unwrap();
        assert_eq!(s1, "Hola Señor.");

        let s2 = rx.recv().await.unwrap();
        assert_eq!(s2, "¿Cómo se encuentra el día de hoy?");

        let s3 = rx.recv().await.unwrap();
        assert_eq!(s3, "Todo está funcionando correctamente.");
    }

    #[tokio::test]
    async fn test_abbreviation_and_decimal_protection() {
        let (tx, mut rx) = mpsc::channel(8);
        let mut tokenizer = SentenceTokenizer::new(tx);

        // Should not split at "Dr." or "3.1416"
        let input = "El Dr. Smith midió exactamente 3.1416 en el laboratorio.";
        tokenizer.feed_token(input).await.unwrap();
        tokenizer.flush().await.unwrap();

        let s1 = rx.recv().await.unwrap();
        assert_eq!(
            s1,
            "El Dr. Smith midió exactamente 3.1416 en el laboratorio."
        );
    }

    #[tokio::test]
    async fn test_minimum_length_threshold() {
        let (tx, mut rx) = mpsc::channel(8);
        let mut tokenizer = SentenceTokenizer::new(tx);

        // "Ok." has 3 chars, under MIN_EMISSION_LENGTH (10). It shouldn't emit alone.
        tokenizer.feed_token("Ok. ").await.unwrap();
        assert!(rx.try_recv().is_err());

        // Feeding more content to exceed 10 chars
        tokenizer
            .feed_token("Procederemos con la actualización inmediata.")
            .await
            .unwrap();
        tokenizer.flush().await.unwrap();

        let s = rx.recv().await.unwrap();
        assert!(s.contains("Ok. Procederemos con la actualización inmediata."));
    }

    #[tokio::test]
    async fn test_early_first_clause_emission() {
        let (tx, mut rx) = mpsc::channel(8);
        let mut tokenizer = SentenceTokenizer::new(tx);

        let tokens = vec![
            "Buenas tardes, ",
            "Señor, ",
            "estoy a su entera disposición para lo que necesite.",
        ];

        for t in tokens {
            tokenizer.feed_token(t).await.unwrap();
        }
        tokenizer.flush().await.unwrap();

        // 1st emission happens early on the first comma (14 chars >= 12)
        let s1 = rx.recv().await.unwrap();
        assert_eq!(s1, "Buenas tardes,");

        // 2nd emission does NOT split on subsequent commas; waits for full sentence delimiter or flush
        let s2 = rx.recv().await.unwrap();
        assert_eq!(
            s2,
            "Señor, estoy a su entera disposición para lo que necesite."
        );
    }

    #[tokio::test]
    async fn test_comma_decimal_protection() {
        let (tx, mut rx) = mpsc::channel(8);
        let mut tokenizer = SentenceTokenizer::new(tx);

        // Should not split on comma in Spanish decimal "3,1416"
        let input = "El valor es 3,1416 en la prueba.";
        tokenizer.feed_token(input).await.unwrap();
        tokenizer.flush().await.unwrap();

        let s1 = rx.recv().await.unwrap();
        assert_eq!(s1, "El valor es 3,1416 en la prueba.");
    }
}
