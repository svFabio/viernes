//! Resilient multi-provider Text-to-Speech coordinator with transparent fallback.

use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use tracing::{info, warn};
use viernes_core::error::ViernesError;
use viernes_core::traits::TextToSpeech;

use crate::edge::EdgeTtsClient;
use crate::elevenlabs::ElevenLabsClient;
use crate::error::TtsError;
use crate::key_pool::KeyPool;

/// Resilient TTS engine orchestrating primary ElevenLabs synthesis with transparent Edge-TTS fallback.
#[derive(Debug)]
pub struct ResilientTtsEngine {
    primary: Arc<dyn TextToSpeech>,
    fallback: Option<Arc<dyn TextToSpeech>>,
}

impl ResilientTtsEngine {
    /// Creates a resilient engine with primary and fallback providers.
    pub fn new(primary: Arc<dyn TextToSpeech>, fallback: Arc<dyn TextToSpeech>) -> Self {
        let fallback = if Arc::ptr_eq(&primary, &fallback) {
            None
        } else {
            Some(fallback)
        };
        Self { primary, fallback }
    }

    /// Creates a resilient engine with only a primary provider and no duplicate fallback.
    pub fn primary_only(primary: Arc<dyn TextToSpeech>) -> Self {
        Self {
            primary,
            fallback: None,
        }
    }

    /// Automatically constructs a resilient engine from environment variables.
    ///
    /// If `ELEVENLABS_API_KEYS` or `ELEVENLABS_API_KEY` is present, `ElevenLabsClient` is primary
    /// (honoring `ELEVENLABS_MODEL_ID` when set).
    /// `EdgeTtsClient` acts as the reliable zero-cost fallback, or primary-only when `TTS_PROVIDER=edge`
    /// or no ElevenLabs keys are configured.
    #[must_use]
    pub fn from_env() -> Self {
        let provider_override = std::env::var("TTS_PROVIDER")
            .unwrap_or_default()
            .trim()
            .to_lowercase();

        if provider_override == "edge" {
            info!("TTS_PROVIDER=edge configured; prioritizing Edge-TTS with zero failover latency");
            return Self::primary_only(Arc::new(EdgeTtsClient::new()));
        }

        let pool = KeyPool::from_env();
        if pool.is_empty() {
            warn!("No ElevenLabs API keys configured; defaulting primary directly to Edge-TTS");
            return Self::primary_only(Arc::new(EdgeTtsClient::new()));
        }

        let model_id = std::env::var("ELEVENLABS_MODEL_ID")
            .ok()
            .map(|m| m.trim().to_string())
            .filter(|m| !m.is_empty());

        let primary: Arc<dyn TextToSpeech> =
            Arc::new(ElevenLabsClient::with_options(pool, None, model_id, None));
        let fallback: Arc<dyn TextToSpeech> = Arc::new(EdgeTtsClient::new());

        Self::new(primary, fallback)
    }

    /// Synthesizes text with automatic failover in < 200ms upon primary exhaustion or failure.
    pub async fn synthesize_resilient(&self, text: &str) -> Result<Vec<u8>, ViernesError> {
        if text.trim().is_empty() {
            return Err(ViernesError::from(TtsError::EmptyText));
        }

        let start = Instant::now();

        match self.primary.synthesize(text).await {
            Ok(bytes) => Ok(bytes),
            Err(primary_err) => {
                let Some(ref fallback) = self.fallback else {
                    warn!(
                        primary = self.primary.provider_name(),
                        error = %primary_err,
                        "Primary TTS failed and no fallback provider is configured"
                    );
                    return Err(primary_err);
                };

                let failover_start = Instant::now();
                warn!(
                    primary = self.primary.provider_name(),
                    fallback = fallback.provider_name(),
                    error = %primary_err,
                    "Primary TTS failed; initiating automatic fallback to Edge-TTS"
                );

                let fallback_result = fallback.synthesize(text).await;
                let failover_duration = failover_start.elapsed();

                match &fallback_result {
                    Ok(bytes) => {
                        info!(
                            bytes = bytes.len(),
                            failover_ms = failover_duration.as_millis(),
                            total_ms = start.elapsed().as_millis(),
                            "Fallback synthesis completed successfully"
                        );
                    }
                    Err(e) => {
                        warn!(
                            error = %e,
                            failover_ms = failover_duration.as_millis(),
                            "Fallback synthesis failed"
                        );
                    }
                }

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

    #[derive(Debug)]
    struct CountingFailingTts {
        calls: std::sync::atomic::AtomicUsize,
    }

    #[async_trait]
    impl TextToSpeech for CountingFailingTts {
        async fn synthesize(&self, _text: &str) -> Result<Vec<u8>, ViernesError> {
            self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Err(ViernesError::Tts("Edge failure".into()))
        }

        fn provider_name(&self) -> &'static str {
            "CountingFailingTts"
        }
    }

    #[tokio::test]
    async fn test_primary_only_and_same_arc_do_not_run_twice() {
        let provider = Arc::new(CountingFailingTts {
            calls: std::sync::atomic::AtomicUsize::new(0),
        });
        let engine = ResilientTtsEngine::primary_only(provider.clone());
        assert!(engine.synthesize("Hola").await.is_err());
        assert_eq!(provider.calls.load(std::sync::atomic::Ordering::SeqCst), 1);

        let shared = Arc::new(CountingFailingTts {
            calls: std::sync::atomic::AtomicUsize::new(0),
        });
        let deduped = ResilientTtsEngine::new(shared.clone(), shared.clone());
        assert!(deduped.synthesize("Hola").await.is_err());
        assert_eq!(shared.calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_empty_text_short_circuits_without_calling_providers() {
        let primary = Arc::new(CountingFailingTts {
            calls: std::sync::atomic::AtomicUsize::new(0),
        });
        let fallback = Arc::new(CountingFailingTts {
            calls: std::sync::atomic::AtomicUsize::new(0),
        });

        let engine = ResilientTtsEngine::new(primary.clone(), fallback.clone());
        let err = engine.synthesize("   \t\n ").await.unwrap_err();
        assert!(err.to_string().contains("empty text"));
        assert_eq!(primary.calls.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert_eq!(fallback.calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[test]
    fn test_from_env_modes_and_model_id() {
        static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        struct EnvGuard {
            saved: Vec<(&'static str, Option<std::ffi::OsString>)>,
        }
        impl Drop for EnvGuard {
            fn drop(&mut self) {
                for (key, val) in &self.saved {
                    match val {
                        Some(v) => std::env::set_var(key, v),
                        None => std::env::remove_var(key),
                    }
                }
            }
        }

        let _lock = ENV_LOCK.lock().unwrap();
        let keys = [
            "TTS_PROVIDER",
            "ELEVENLABS_API_KEYS",
            "ELEVENLABS_API_KEY",
            "ELEVENLABS_MODEL_ID",
        ];
        let _guard = EnvGuard {
            saved: keys.iter().map(|k| (*k, std::env::var_os(k))).collect(),
        };

        // 1. TTS_PROVIDER=edge -> primary Edge-TTS, no fallback
        std::env::set_var("TTS_PROVIDER", "edge");
        std::env::set_var("ELEVENLABS_API_KEYS", "k1,k2");
        let edge_engine = ResilientTtsEngine::from_env();
        assert_eq!(edge_engine.primary.provider_name(), "Edge-TTS");
        assert!(edge_engine.fallback.is_none());

        // 2. Empty keys -> primary Edge-TTS, no fallback
        std::env::remove_var("TTS_PROVIDER");
        std::env::remove_var("ELEVENLABS_API_KEYS");
        std::env::remove_var("ELEVENLABS_API_KEY");
        let no_keys_engine = ResilientTtsEngine::from_env();
        assert_eq!(no_keys_engine.primary.provider_name(), "Edge-TTS");
        assert!(no_keys_engine.fallback.is_none());

        // 3. Keys present + ELEVENLABS_MODEL_ID -> ElevenLabs primary + Edge fallback
        std::env::set_var("ELEVENLABS_API_KEYS", "k1");
        std::env::set_var("ELEVENLABS_MODEL_ID", "eleven_turbo_v2_5");
        let eleven_engine = ResilientTtsEngine::from_env();
        assert_eq!(eleven_engine.primary.provider_name(), "ElevenLabs");
        assert!(eleven_engine.fallback.is_some());
        assert!(format!("{:?}", eleven_engine.primary).contains("eleven_turbo_v2_5"));
    }
}
