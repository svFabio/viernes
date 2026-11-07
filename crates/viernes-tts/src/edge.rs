//! Native async WebSocket client for Microsoft Edge Text-to-Speech fallback.

use async_trait::async_trait;
use futures::{SinkExt, StreamExt};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, warn};
use uuid::Uuid;
use viernes_core::error::ViernesError;
use viernes_core::traits::TextToSpeech;

use crate::error::TtsError;

const TRUSTED_CLIENT_TOKEN: &str = "6A5AA1D4EAFF4E9FB37E23D68491D6F4";
const DEFAULT_EDGE_VOICE: &str = "es-ES-ElviraNeural";
const DEFAULT_AUDIO_FORMAT: &str = "audio-24khz-48kbitrate-mono-mp3";

/// Native asynchronous Edge-TTS fallback provider communicating directly over WebSockets.
#[derive(Debug, Clone)]
pub struct EdgeTtsClient {
    voice: String,
    audio_format: String,
}

impl Default for EdgeTtsClient {
    fn default() -> Self {
        Self {
            voice: DEFAULT_EDGE_VOICE.to_string(),
            audio_format: DEFAULT_AUDIO_FORMAT.to_string(),
        }
    }
}

impl EdgeTtsClient {
    /// Creates a new `EdgeTtsClient` with the default Spanish voice (`es-ES-ElviraNeural`).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates an `EdgeTtsClient` specifying a custom neural voice name.
    #[must_use]
    pub fn with_voice(voice: impl Into<String>) -> Self {
        Self {
            voice: voice.into(),
            audio_format: DEFAULT_AUDIO_FORMAT.to_string(),
        }
    }

    /// Escapes XML special characters for valid SSML body formatting.
    #[must_use]
    pub fn escape_xml(input: &str) -> String {
        input
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&apos;")
    }

    /// Constructs the initial speech.config JSON payload.
    #[must_use]
    pub fn build_config_message(audio_format: &str) -> String {
        format!(
            "Content-Type:application/json; charset=utf-8\r\nPath:speech.config\r\n\r\n\
            {{\"context\":{{\"synthesis\":{{\"audio\":{{\"metadataoptions\":{{\"sentenceBoundaryEnabled\":\"false\",\"wordBoundaryEnabled\":\"false\"}},\"outputFormat\":\"{}\"}}}}}}",
            audio_format
        )
    }

    /// Constructs the SSML synthesis packet for a given request ID and text.
    #[must_use]
    pub fn build_ssml_message(req_id: &str, voice: &str, text: &str) -> String {
        let escaped = Self::escape_xml(text);
        format!(
            "X-RequestId:{req_id}\r\nContent-Type:application/ssml+xml\r\nPath:ssml\r\n\r\n\
            <speak version='1.0' xmlns='http://www.w3.org/2001/10/synthesis' xml:lang='es-ES'>\
            <voice name='{voice}'>{escaped}</voice>\
            </speak>"
        )
    }

    /// Parses a binary WebSocket message according to the Edge-TTS audio frame protocol.
    ///
    /// Binary format:
    /// - 2 bytes: Big-endian length of text header `L`
    /// - `L` bytes: ASCII text header (e.g. `Path:audio\r\n...`)
    /// - Remaining bytes: Raw MP3 binary payload
    pub fn parse_binary_audio_frame(data: &[u8]) -> Option<Vec<u8>> {
        if data.len() < 2 {
            return None;
        }

        let header_len = u16::from_be_bytes([data[0], data[1]]) as usize;
        let total_header = 2 + header_len;

        if data.len() < total_header {
            return None;
        }

        let header_str = String::from_utf8_lossy(&data[2..total_header]);
        if header_str.contains("Path:audio") {
            let audio_payload = &data[total_header..];
            Some(audio_payload.to_vec())
        } else {
            None
        }
    }

    /// Internal WebSocket synthesis implementation.
    async fn synthesize_internal(&self, text: &str) -> Result<Vec<u8>, TtsError> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Err(TtsError::EmptyText);
        }

        let conn_id = Uuid::new_v4().simple().to_string();
        let req_id = Uuid::new_v4().simple().to_string();

        let wss_url = format!(
            "wss://speech.platform.bing.com/consumer/speech/synthesize/readahead/edge/v1?TrustedClientToken={TRUSTED_CLIENT_TOKEN}&ConnectionId={conn_id}"
        );

        let mut request = wss_url
            .into_client_request()
            .map_err(|e| TtsError::WebSocket(format!("Invalid WSS URL: {e}")))?;

        // Attach required client headers
        let headers = request.headers_mut();
        headers.insert(
            "User-Agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36 Edg/130.0.0.0"
                .parse()
                .unwrap(),
        );
        headers.insert("Pragma", "no-cache".parse().unwrap());
        headers.insert("Cache-Control", "no-cache".parse().unwrap());
        headers.insert(
            "Origin",
            "chrome-extension://jdiccldimpdaibmpdkjnbmckianbfold"
                .parse()
                .unwrap(),
        );

        debug!("Connecting to Edge-TTS WebSocket endpoint");
        let (ws_stream, _) = connect_async(request)
            .await
            .map_err(|e| TtsError::WebSocket(format!("WebSocket handshake failed: {e}")))?;

        let (mut write, mut read) = ws_stream.split();

        // Step 1: Send speech.config
        let config_msg = Self::build_config_message(&self.audio_format);
        write
            .send(Message::Text(config_msg))
            .await
            .map_err(|e| TtsError::WebSocket(format!("Failed sending speech.config: {e}")))?;

        // Step 2: Send SSML packet
        let ssml_msg = Self::build_ssml_message(&req_id, &self.voice, trimmed);
        write
            .send(Message::Text(ssml_msg))
            .await
            .map_err(|e| TtsError::WebSocket(format!("Failed sending SSML: {e}")))?;

        let mut audio_accumulator = Vec::new();

        // Step 3: Stream and parse response frames
        while let Some(msg_result) = read.next().await {
            match msg_result {
                Ok(Message::Binary(bin_data)) => {
                    if let Some(audio_chunk) = Self::parse_binary_audio_frame(&bin_data) {
                        audio_accumulator.extend_from_slice(&audio_chunk);
                    }
                }
                Ok(Message::Text(text_frame)) => {
                    if text_frame.contains("Path:turn.end") {
                        debug!("Edge-TTS synthesis completed (Path:turn.end received)");
                        break;
                    }
                }
                Ok(Message::Close(_)) => {
                    warn!("Edge-TTS WebSocket closed by remote host");
                    break;
                }
                Err(err) => {
                    return Err(TtsError::WebSocket(format!(
                        "Stream error from Edge-TTS: {err}"
                    )));
                }
                _ => {}
            }
        }

        if audio_accumulator.is_empty() {
            Err(TtsError::Protocol(
                "Edge-TTS synthesis completed but no audio packets were received".into(),
            ))
        } else {
            Ok(audio_accumulator)
        }
    }
}

#[async_trait]
impl TextToSpeech for EdgeTtsClient {
    async fn synthesize(&self, text: &str) -> Result<Vec<u8>, ViernesError> {
        self.synthesize_internal(text)
            .await
            .map_err(ViernesError::from)
    }

    fn provider_name(&self) -> &'static str {
        "Edge-TTS"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_xml() {
        assert_eq!(
            EdgeTtsClient::escape_xml("Viernes & <Jarvis> \"Hola\" 'Amigo'"),
            "Viernes &amp; &lt;Jarvis&gt; &quot;Hola&quot; &apos;Amigo&apos;"
        );
    }

    #[test]
    fn test_build_config_message() {
        let msg = EdgeTtsClient::build_config_message("audio-24khz-48kbitrate-mono-mp3");
        assert!(msg.contains("Path:speech.config"));
        assert!(msg.contains("audio-24khz-48kbitrate-mono-mp3"));
    }

    #[test]
    fn test_build_ssml_message() {
        let ssml = EdgeTtsClient::build_ssml_message(
            "req-123",
            "es-ES-ElviraNeural",
            "A la orden, Señor.",
        );
        assert!(ssml.contains("X-RequestId:req-123"));
        assert!(ssml.contains("<voice name='es-ES-ElviraNeural'>A la orden, Señor.</voice>"));
    }

    #[test]
    fn test_parse_binary_audio_frame() {
        let header = "Path:audio\r\nX-RequestId:abc\r\n";
        let header_bytes = header.as_bytes();
        let header_len = header_bytes.len() as u16;

        let payload = vec![0xFF, 0xFB, 0x90, 0x64, 0xAA, 0xBB];

        let mut frame = Vec::new();
        frame.extend_from_slice(&header_len.to_be_bytes());
        frame.extend_from_slice(header_bytes);
        frame.extend_from_slice(&payload);

        let parsed = EdgeTtsClient::parse_binary_audio_frame(&frame);
        assert!(parsed.is_some());
        assert_eq!(parsed.unwrap(), payload);
    }

    #[test]
    fn test_parse_non_audio_binary_frame() {
        let header = "Path:response\r\n";
        let header_bytes = header.as_bytes();
        let header_len = header_bytes.len() as u16;

        let mut frame = Vec::new();
        frame.extend_from_slice(&header_len.to_be_bytes());
        frame.extend_from_slice(header_bytes);

        assert!(EdgeTtsClient::parse_binary_audio_frame(&frame).is_none());
    }

    #[tokio::test]
    async fn test_empty_text_error() {
        let client = EdgeTtsClient::new();
        let res = client.synthesize("   ").await;
        assert!(res.is_err());
    }
}
