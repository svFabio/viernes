//! Resilient multi-provider Text-to-Speech coordinator with transparent fallback.

use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use tracing::{info, warn};
use viernes_core::error::ViernesError;
use viernes_core::traits::TextToSpeech;

use crate::edge::EdgeTtsClient;
use crate::elevenlabs::ElevenLabsClient;
use crate::key_pool::KeyPool;

/// Resilient TTS engine orchestrating primary ElevenLabs synthesis with transparent Edge-TTS fallback.
#[derive(Debug)]
pub struct ResilientTtsEngine {
    primary: Arc<dyn TextToSpeech>,
    fallback: Arc<dyn TextToSpeech>,
}

impl ResilientTtsEngine {
    /// Creates a resilient engine with primary and fallback providers.
    pub fn new(primary: Arc<dyn TextToSpeech>, fallback: Arc<dyn TextToSpeech>) -> Self {
        Self { primary, fallback }
    }

    /// Automatically constructs a resilient engine from environment variables.
    ///
    /// If `ELEVENLABS_API_KEYS` or `ELEVENLABS_API_KEY` is present, `ElevenLabsClient` is primary.
    /// `EdgeTtsClient` acts as the reliable zero-cost fallback.
    #[must_use]
    pub fn from_env() -> Self {
        let pool = KeyPool::from_env();
        let primary: Arc<dyn TextToSpeech> = if pool.is_empty() {
            warn!("No ElevenLabs API keys configured; defaulting primary directly to Edge-TTS");
            Arc::new(EdgeTtsClient::new())
        } else {
            Arc::new(ElevenLabsClient::new(pool))
        };

        let fallback: Arc<dyn TextToSpeech> = Arc::new(EdgeTtsClient::new());

        Self::new(primary, fallback)
    }

    /// Synthesizes text with automatic failover in < 200ms upon primary exhaustion or failure.
    pub async fn synthesize_resilient(&self, text: &str) -> Result<Vec<u8>, ViernesError> {
        let start = Instant::now();

        match self.primary.synthesize(text).await {
            Ok(bytes) => Ok(bytes),
            Err(primary_err) => {
                let failover_start = Instant::now();
                warn!(
                    primary = self.primary.provider_name(),
                    fallback = self.fallback.provider_name(),
                    error = %primary_err,
                    "Primary TTS failed; initiating automatic fallback to Edge-TTS"
                );

                let fallback_result = self.fallback.synthesize(text).await;
                let failover_duration = failover_start.elapsed();

                info!(
                    failover_ms = failover_duration.as_millis(),
                    total_ms = start.elapsed().as_millis(),
                    "Fallback synthesis completed"
                );

                fallback_result
            }
        }
    }
}

#[async_trait]
impl TextToSpeech for ResilientTtsEngine {
    async fn synthesize(&self, text: &str) -> Result<Vec<u8>, ViernesError> {
        self.synthesize_resilient(text).await
    }

    fn provider_name(&self) -> &'static str {
        "ResilientTtsEngine"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use viernes_core::traits::FakeTextToSpeech;

    #[derive(Debug)]
    struct FaultyTts;

    #[async_trait]
    impl TextToSpeech for FaultyTts {
        async fn synthesize(&self, _text: &str) -> Result<Vec<u8>, ViernesError> {
            Err(ViernesError::Tts(
                "All ElevenLabs keys exhausted (429)".into(),
            ))
        }

        fn provider_name(&self) -> &'static str {
            "FaultyElevenLabs"
        }
    }

    #[tokio::test]
    async fn test_resilient_fallback_on_failure() {
        let primary = Arc::new(FaultyTts);
        let fallback = Arc::new(FakeTextToSpeech {
            name: "Edge-TTS-Fake",
        });

        let engine = ResilientTtsEngine::new(primary, fallback);
        let result = engine.synthesize("Hola mundo").await;

        assert!(result.is_ok());
        let bytes = result.unwrap();
        // Check dummy payload produced by fallback
        assert!(!bytes.is_empty());
    }
}
