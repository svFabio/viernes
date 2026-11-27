//! Asynchronous ElevenLabs HTTP client with connection pooling, streaming, and multi-key rotation.

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use serde::Serialize;
use tracing::{debug, warn};
use viernes_core::error::ViernesError;
use viernes_core::traits::TextToSpeech;

use crate::error::TtsError;
use crate::key_pool::KeyPool;

const DEFAULT_ELEVENLABS_BASE_URL: &str = "https://api.elevenlabs.io";
const DEFAULT_VOICE_ID: &str = "21m00Tcm4TlvDq8ikWAM"; // Default Rachel voice
const DEFAULT_MODEL_ID: &str = "eleven_multilingual_v2";
const DEFAULT_OUTPUT_FORMAT: &str = "mp3_44100_128";

#[derive(Serialize)]
struct VoiceSettings {
    stability: f32,
    similarity_boost: f32,
}

#[derive(Serialize)]
struct SynthesisPayload<'a> {
    text: &'a str,
    model_id: &'a str,
    voice_settings: VoiceSettings,
}

/// ElevenLabs HTTP client supporting connection pooling, TLS, and automatic key rotation.
pub struct ElevenLabsClient {
    client: Client,
    key_pool: Arc<KeyPool>,
    voice_id: String,
    model_id: String,
    base_url: String,
}

impl fmt::Debug for ElevenLabsClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ElevenLabsClient")
            .field("voice_id", &self.voice_id)
            .field("model_id", &self.model_id)
            .field("base_url", &self.base_url)
            .field("key_pool", &self.key_pool)
            .finish()
    }
}

impl ElevenLabsClient {
    /// Creates a new `ElevenLabsClient` with the given key pool.
    pub fn new(key_pool: KeyPool) -> Self {
        Self::with_options(key_pool, None, None, None)
    }

    /// Creates a client with customized voice, model, and base URL (useful for testing).
    pub fn with_options(
        key_pool: KeyPool,
        voice_id: Option<String>,
        model_id: Option<String>,
        base_url: Option<String>,
    ) -> Self {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(15))
            .pool_idle_timeout(Duration::from_secs(60))
            .pool_max_idle_per_host(5)
            .gzip(true)
            .brotli(true)
            .build()
            .unwrap_or_else(|err| {
                warn!(
                    error = %err,
                    "Failed to build configured reqwest Client; falling back to default Client"
                );
                Client::new()
            });

        let voice = voice_id
            .or_else(|| std::env::var("ELEVENLABS_VOICE_ID").ok())
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_VOICE_ID.to_string());

        let model = model_id.unwrap_or_else(|| DEFAULT_MODEL_ID.to_string());
        let base = base_url.unwrap_or_else(|| DEFAULT_ELEVENLABS_BASE_URL.to_string());

        Self {
            client,
            key_pool: Arc::new(key_pool),
            voice_id: voice,
            model_id: model,
            base_url: base,
        }
    }

    /// Reference to the underlying key pool.
    #[must_use]
    pub fn key_pool(&self) -> &KeyPool {
        &self.key_pool
    }

    /// Reference to the configured model ID.
    #[must_use]
    pub fn model_id(&self) -> &str {
        &self.model_id
    }

    /// Internal synthesis loop with atomic rotation on 401 or 429.
    async fn synthesize_internal(&self, text: &str) -> Result<Vec<u8>, TtsError> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Err(TtsError::EmptyText);
        }

        let total_keys = self.key_pool.len();
        if total_keys == 0 {
            return Err(TtsError::NoKeysConfigured);
        }

        let endpoint = format!(
            "{}/v1/text-to-speech/{}/stream?output_format={}",
            self.base_url, self.voice_id, DEFAULT_OUTPUT_FORMAT
        );

        let payload = SynthesisPayload {
            text: trimmed,
            model_id: &self.model_id,
            voice_settings: VoiceSettings {
                stability: 0.5,
                similarity_boost: 0.75,
            },
        };

        let mut attempts = 0;

        while attempts < total_keys {
            attempts += 1;
            let Some(active) = self.key_pool.get_active_key_with_index() else {
                return Err(TtsError::AllKeysExhausted(total_keys));
            };

            debug!(
                attempt = attempts,
                max_attempts = total_keys,
                key_idx = active.index + 1,
                "Dispatching ElevenLabs synthesis request"
            );

            let response = self
                .client
                .post(&endpoint)
                .header("xi-api-key", &active.key)
                .header("Content-Type", "application/json")
                .header("Accept", "audio/mpeg")
                .json(&payload)
                .send()
                .await;

            match response {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        let audio_bytes = resp.bytes().await.map_err(TtsError::Http)?;
                        return Ok(audio_bytes.to_vec());
                    } else if status == StatusCode::UNAUTHORIZED
                        || status == StatusCode::TOO_MANY_REQUESTS
                    {
                        let is_unauthorized = status == StatusCode::UNAUTHORIZED;
                        warn!(
                            status = %status,
                            key_idx = active.index + 1,
                            permanent = is_unauthorized,
                            attempt = attempts,
                            "ElevenLabs API key rejected or quota exceeded; rotating key"
                        );
                        self.key_pool
                            .mark_exhausted_at(active.index, is_unauthorized);
                    } else {
                        let err_text = resp.text().await.unwrap_or_default();
                        return Err(TtsError::Protocol(format!(
                            "ElevenLabs server returned HTTP {status}: {err_text}"
                        )));
                    }
                }
                Err(err) => {
                    warn!(error = %err, "HTTP request failed connecting to ElevenLabs");
                    return Err(TtsError::Http(err));
                }
            }
        }

        Err(TtsError::AllKeysExhausted(total_keys))
    }
}

#[async_trait]
impl TextToSpeech for ElevenLabsClient {
    async fn synthesize(&self, text: &str) -> Result<Vec<u8>, ViernesError> {
        self.synthesize_internal(text)
            .await
            .map_err(ViernesError::from)
    }

    fn provider_name(&self) -> &'static str {
        "ElevenLabs"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path_regex};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn test_empty_text_rejection() {
        let pool = KeyPool::new(vec!["fake_key".into()]);
        let client = ElevenLabsClient::new(pool);
        let result = client.synthesize("   ").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_key_rotation_on_429() {
        let mock_server = MockServer::start().await;

        // Key 1 triggers 429 Too Many Requests
        Mock::given(method("POST"))
            .and(path_regex(r"^/v1/text-to-speech/.*/stream"))
            .and(header("xi-api-key", "key_alpha"))
            .respond_with(ResponseTemplate::new(429).set_body_string("Quota Exceeded"))
            .expect(1)
            .mount(&mock_server)
            .await;

        // Key 2 responds with 200 OK and synthetic MP3 stream
        let dummy_mp3 = vec![0xFF, 0xFB, 0x90, 0x64, 0x01, 0x02, 0x03];
        Mock::given(method("POST"))
            .and(path_regex(r"^/v1/text-to-speech/.*/stream"))
            .and(header("xi-api-key", "key_beta"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(dummy_mp3.clone()))
            .expect(1)
            .mount(&mock_server)
            .await;

        let pool = KeyPool::new(vec!["key_alpha".into(), "key_beta".into()]);
        let client = ElevenLabsClient::with_options(pool, None, None, Some(mock_server.uri()));

        let audio = client
            .synthesize("Hola mundo")
            .await
            .expect("Synthesis failed");
        assert_eq!(audio, dummy_mp3);
    }

    #[tokio::test]
    async fn test_all_keys_exhausted_failover_signal() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path_regex(r"^/v1/text-to-speech/.*/stream"))
            .respond_with(ResponseTemplate::new(429))
            .expect(2)
            .mount(&mock_server)
            .await;

        let pool = KeyPool::new(vec!["key_1".into(), "key_2".into()]);
        let client = ElevenLabsClient::with_options(pool, None, None, Some(mock_server.uri()));

        let result = client.synthesize("A la orden").await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("exhausted"));
    }

    #[tokio::test]
    async fn test_401_permanently_disables_invalid_key_and_rotates() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path_regex(r"^/v1/text-to-speech/.*/stream"))
            .and(header("xi-api-key", "invalid_key"))
            .respond_with(ResponseTemplate::new(401).set_body_string("Unauthorized"))
            .expect(1)
            .mount(&mock_server)
            .await;

        let dummy_mp3 = vec![0xFF, 0xFB, 0x90, 0x64, 0x11, 0x22];
        Mock::given(method("POST"))
            .and(path_regex(r"^/v1/text-to-speech/.*/stream"))
            .and(header("xi-api-key", "valid_key"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(dummy_mp3.clone()))
            .expect(2)
            .mount(&mock_server)
            .await;

        let pool = KeyPool::new(vec!["invalid_key".into(), "valid_key".into()]);
        let client = ElevenLabsClient::with_options(
            pool,
            None,
            Some("eleven_flash_v2_5".into()),
            Some(mock_server.uri()),
        );
        assert_eq!(client.model_id(), "eleven_flash_v2_5");

        let audio1 = client.synthesize("First call").await.unwrap();
        assert_eq!(audio1, dummy_mp3);

        // Second call should go straight to valid_key without retrying permanently disabled invalid_key
        let audio2 = client.synthesize("Second call").await.unwrap();
        assert_eq!(audio2, dummy_mp3);
    }

    #[tokio::test]
    async fn test_no_keys_configured_returns_error() {
        let client = ElevenLabsClient::new(KeyPool::new(vec![]));
        let err = client.synthesize("Hello").await.unwrap_err();
        assert!(err.to_string().contains("No API keys configured"));
    }
}
