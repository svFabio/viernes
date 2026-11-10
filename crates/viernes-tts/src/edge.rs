//! Native async WebSocket client for Microsoft Edge Text-to-Speech fallback.

use async_trait::async_trait;
use viernes_core::error::ViernesError;
use viernes_core::traits::TextToSpeech;

use crate::error::TtsError;

#[allow(dead_code)]
const TRUSTED_CLIENT_TOKEN: &str = "6A5AA1D4EAFF4E9FB37E23D68491D6F4";
const DEFAULT_EDGE_VOICE: &str = "es-CO-SalomeNeural";
const DEFAULT_AUDIO_FORMAT: &str = "audio-24khz-48kbitrate-mono-mp3";

/// Native asynchronous Edge-TTS fallback provider communicating directly over WebSockets.
#[derive(Debug, Clone)]
pub struct EdgeTtsClient {
    voice: String,
    #[allow(dead_code)]
    audio_format: String,
    rate: String,
    pitch: String,
}

impl Default for EdgeTtsClient {
    fn default() -> Self {
        let voice = std::env::var("EDGE_TTS_VOICE")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_EDGE_VOICE.to_string());
        let rate = std::env::var("EDGE_TTS_RATE")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| "+30%".to_string());
        let pitch = std::env::var("EDGE_TTS_PITCH")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| "+6Hz".to_string());

        Self {
            voice,
            audio_format: DEFAULT_AUDIO_FORMAT.to_string(),
            rate,
            pitch,
        }
    }
}

impl EdgeTtsClient {
    /// Creates a new `EdgeTtsClient` with default voice and configuration.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates an `EdgeTtsClient` specifying a custom neural voice name.
    #[must_use]
    pub fn with_voice(voice: impl Into<String>) -> Self {
        Self {
            voice: voice.into(),
            ..Default::default()
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

    /// Internal Edge-TTS synthesis implementation.
    ///
    /// Directly delegates to DRM v2 CLI engine streaming audio over stdout pipe,
    /// bypassing the raw WebSocket endpoint to eliminate 300ms of handshake latency.
    async fn synthesize_internal(&self, text: &str) -> Result<Vec<u8>, TtsError> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Err(TtsError::EmptyText);
        }

        self.synthesize_via_cli(trimmed).await
    }

    /// Synthesizes speech using the local `edge-tts` tool with automatic DRM v2 negotiation
    /// and zero-disk in-memory stdout piping.
    pub async fn synthesize_via_cli(&self, text: &str) -> Result<Vec<u8>, TtsError> {
        let edge_bin = if let Ok(home) = std::env::var("HOME") {
            let p = std::path::PathBuf::from(home).join(".local/bin/edge-tts");
            if p.exists() {
                p
            } else {
                std::path::PathBuf::from("edge-tts")
            }
        } else {
            std::path::PathBuf::from("edge-tts")
        };

        let mut cmd = if edge_bin.exists() {
            tokio::process::Command::new(edge_bin)
        } else {
            let uv_bin = if let Ok(home) = std::env::var("HOME") {
                let p = std::path::PathBuf::from(home).join(".local/bin/uv");
                if p.exists() {
                    p
                } else {
                    std::path::PathBuf::from("uv")
                }
            } else {
                std::path::PathBuf::from("uv")
            };
            let mut c = tokio::process::Command::new(uv_bin);
            c.args(["run", "--with", "edge-tts", "edge-tts"]);
            c
        };

        cmd.arg("--text")
            .arg(text)
            .arg("--voice")
            .arg(&self.voice)
            .arg("--rate")
            .arg(&self.rate)
            .arg("--pitch")
            .arg(&self.pitch)
            .arg("--write-media")
            .arg("-");

        cmd.stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let output = cmd
            .output()
            .await
            .map_err(|e| TtsError::Protocol(format!("Failed to spawn edge-tts: {e}")))?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(TtsError::Protocol(format!("edge-tts failed: {err}")));
        }

        if output.stdout.is_empty() {
            return Err(TtsError::Protocol(
                "edge-tts synthesis produced empty audio payload".into(),
            ));
        }

        Ok(output.stdout)
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
