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

        while let Some(end_idx) = self.find_sentence_boundary() {
            let sentence = self.buffer[..end_idx].trim().to_string();
            self.buffer = self.buffer[end_idx..].to_string();

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
    /// Returns an exclusive byte index aligned on a UTF-8 char boundary.
    fn find_sentence_boundary(&self) -> Option<usize> {
        for (i, ch) in self.buffer.char_indices() {
            let is_sentence_delimiter = matches!(ch, '.' | '?' | '!' | '\n');
            let is_early_clause = !self.first_clause_emitted && matches!(ch, ',' | ';' | ':');

            if !(is_sentence_delimiter || is_early_clause) {
                continue;
            }

            let end_idx = i + ch.len_utf8();

            // Defensive check 1: Decimal number protection (e.g. 3.14 or 3,14)
            if ch == '.' || ch == ',' {
                let prev_is_digit = self.buffer[..i]
                    .chars()
                    .next_back()
                    .is_some_and(|c| c.is_ascii_digit());
                let next_is_digit = self.buffer[end_idx..]
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_digit());
                if prev_is_digit && next_is_digit {
                    continue;
                }
            }

            // Defensive check 2: Common Spanish and English abbreviations for '.'
            if ch == '.' && is_common_abbreviation(&self.buffer[..end_idx]) {
                continue;
            }

            // Defensive check 3: Lookahead for repeated delimiters, closing quotes/parens,
            // and early-clause whitespace boundaries.
            let mut split_end = end_idx;
            if let Some(next_ch) = self.buffer[end_idx..].chars().next() {
                if matches!(next_ch, '.' | '?' | '!' | ',') {
                    continue;
                }

                if matches!(ch, '.' | '?' | '!') && matches!(next_ch, '"' | '\'' | ')') {
                    let after_closing = &self.buffer[end_idx + next_ch.len_utf8()..];
                    if after_closing.is_empty() || after_closing.starts_with(char::is_whitespace) {
                        split_end = end_idx + next_ch.len_utf8();
                    } else {
                        continue;
                    }
                } else if is_early_clause && !next_ch.is_whitespace() {
                    continue;
                }
            }

            // Defensive check 4: Minimum character length threshold measured in Unicode chars
            let min_len = if is_early_clause {
                MIN_FIRST_CLAUSE_LENGTH
            } else {
                MIN_EMISSION_LENGTH
            };
            if self.buffer[..split_end].trim().chars().count() < min_len {
                continue;
            }

            return Some(split_end);
        }

        None
    }
}

/// Helper identifying whether a period ends a known abbreviation rather than a sentence.
fn is_common_abbreviation(slice: &str) -> bool {
    let lower = slice.to_lowercase();
    if let Some(last_word) = lower.split_whitespace().last() {
        let stripped = last_word
            .trim()
            .trim_start_matches(['(', '"', '\'', '¿', '¡']);
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

    #[tokio::test]
    async fn test_splitting_after_closing_quote_and_paren() {
        let (tx, mut rx) = mpsc::channel(8);
        let mut tokenizer = SentenceTokenizer::new(tx);

        tokenizer
            .feed_token("Dijo \"Buenos días.\" Luego salió. ")
            .await
            .unwrap();
        tokenizer
            .feed_token("(Nota importante.) ¿Qué opinas? ")
            .await
            .unwrap();
        tokenizer
            .feed_token("Gritó \"¡Cuidado!\" al instante.")
            .await
            .unwrap();
        tokenizer.flush().await.unwrap();

        assert_eq!(rx.recv().await.unwrap(), "Dijo \"Buenos días.\"");
        assert_eq!(rx.recv().await.unwrap(), "Luego salió.");
        assert_eq!(rx.recv().await.unwrap(), "(Nota importante.)");
        assert_eq!(rx.recv().await.unwrap(), "¿Qué opinas?");
        assert_eq!(rx.recv().await.unwrap(), "Gritó \"¡Cuidado!\"");
        assert_eq!(rx.recv().await.unwrap(), "al instante.");
    }

    #[tokio::test]
    async fn test_punctuation_prefixed_abbreviations_do_not_split() {
        assert!(is_common_abbreviation("(Dr."));
        assert!(is_common_abbreviation("\"Sr."));
        assert!(is_common_abbreviation("'Prof."));
        assert!(is_common_abbreviation("¿Dr."));
        assert!(is_common_abbreviation("¡Sr."));

        let (tx, mut rx) = mpsc::channel(8);
        let mut tokenizer = SentenceTokenizer::new(tx);

        let inputs = [
            "(Dr. García llegó puntualmente.)",
            "\"Sr. Pérez confirmó la reunión.\"",
            "'Prof. López explicó el tema.'",
            "¿Dr. Smith viene hoy?",
            "¡Sr. García llegó a tiempo!",
        ];

        for input in inputs {
            tokenizer.feed_token(input).await.unwrap();
            tokenizer.flush().await.unwrap();
            let emitted = rx.recv().await.unwrap();
            assert_eq!(emitted, input);
            assert!(rx.try_recv().is_err());
        }
    }

    #[tokio::test]
    async fn test_utf8_char_length_thresholds() {
        let (tx, mut rx) = mpsc::channel(8);
        let mut tokenizer = SentenceTokenizer::new(tx);

        // "¿Qué tal?" is 9 Unicode chars (< MIN_EMISSION_LENGTH=10), even though it is 11 UTF-8 bytes.
        tokenizer.feed_token("¿Qué tal? ").await.unwrap();
        assert!(
            rx.try_recv().is_err(),
            "Should not emit 9-char UTF-8 string early"
        );

        tokenizer.feed_token("Todo excelente.").await.unwrap();
        tokenizer.flush().await.unwrap();
        assert_eq!(rx.recv().await.unwrap(), "¿Qué tal? Todo excelente.");

        // Early clause: "¡Sí, señor," is 11 chars (< MIN_FIRST_CLAUSE_LENGTH=12), even though 14 UTF-8 bytes.
        tokenizer.feed_token("¡Sí, señor, ").await.unwrap();
        assert!(
            rx.try_recv().is_err(),
            "Should not emit 11-char UTF-8 early clause"
        );
        tokenizer.feed_token("vamos ahora.").await.unwrap();
        tokenizer.flush().await.unwrap();
        assert_eq!(rx.recv().await.unwrap(), "¡Sí, señor, vamos ahora.");
    }
}
