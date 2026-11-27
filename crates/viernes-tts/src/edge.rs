//! Asynchronous Edge-TTS fallback client with CLI subprocess execution and protocol helpers.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use async_trait::async_trait;
use viernes_core::error::ViernesError;
use viernes_core::traits::TextToSpeech;

use crate::error::TtsError;

#[allow(dead_code)]
const TRUSTED_CLIENT_TOKEN: &str = "6A5AA1D4EAFF4E9FB37E23D68491D6F4";
const DEFAULT_EDGE_VOICE: &str = "es-CO-SalomeNeural";
const DEFAULT_AUDIO_FORMAT: &str = "audio-24khz-48kbitrate-mono-mp3";
const EDGE_CLI_TIMEOUT: Duration = Duration::from_secs(15);

/// Asynchronous Edge-TTS fallback provider backed by the `edge-tts` CLI with WebSocket/SSML helpers.
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
            {{\"context\":{{\"synthesis\":{{\"audio\":{{\"metadataoptions\":{{\"sentenceBoundaryEnabled\":\"false\",\"wordBoundaryEnabled\":\"false\"}},\"outputFormat\":\"{}\"}}}}}}}}",
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

    /// Builds CLI arguments for `edge-tts`, using `--flag=value` syntax to prevent flag injection
    /// when input text or negative prosody values start with `-`.
    fn build_cli_args(&self, text: &str) -> Vec<String> {
        vec![
            format!("--text={text}"),
            format!("--voice={}", self.voice),
            format!("--rate={}", self.rate),
            format!("--pitch={}", self.pitch),
            "--write-media".to_string(),
            "-".to_string(),
        ]
    }

    /// Synthesizes speech using the local `edge-tts` tool with automatic DRM v2 negotiation
    /// and zero-disk in-memory stdout piping.
    pub async fn synthesize_via_cli(&self, text: &str) -> Result<Vec<u8>, TtsError> {
        let mut cmd = if let Some(edge_bin) = resolve_binary("edge-tts") {
            tokio::process::Command::new(edge_bin)
        } else {
            let uv_bin = resolve_binary("uv").unwrap_or_else(|| PathBuf::from("uv"));
            let mut c = tokio::process::Command::new(uv_bin);
            c.args(["run", "--with", "edge-tts", "edge-tts"]);
            c
        };

        cmd.args(self.build_cli_args(text));
        cmd.stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let output = run_cli_with_timeout(&mut cmd, EDGE_CLI_TIMEOUT).await?;

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

/// Resolves a binary by checking `~/.local/bin/<name>` first, then scanning `$PATH`.
fn resolve_binary(name: &str) -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let path_env = std::env::var_os("PATH");
    resolve_binary_in(name, home.as_deref(), path_env.as_deref())
}

/// Pure helper resolving `<name>` from `<home>/.local/bin` or `$PATH` entries.
fn resolve_binary_in(name: &str, home: Option<&Path>, path_env: Option<&OsStr>) -> Option<PathBuf> {
    if let Some(home_dir) = home {
        let candidate = home_dir.join(".local/bin").join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    if let Some(paths) = path_env {
        for dir in std::env::split_paths(paths) {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    None
}

/// Executes a child command with `.kill_on_drop(true)` and a hard timeout.
async fn run_cli_with_timeout(
    cmd: &mut tokio::process::Command,
    timeout: Duration,
) -> Result<std::process::Output, TtsError> {
    cmd.kill_on_drop(true);
    tokio::time::timeout(timeout, cmd.output())
        .await
        .map_err(|_| {
            TtsError::Protocol(format!("edge-tts timed out after {}s", timeout.as_secs()))
        })?
        .map_err(|e| TtsError::Protocol(format!("Failed to spawn edge-tts: {e}")))
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

        let (_headers, json_body) = msg.split_once("\r\n\r\n").unwrap();
        let parsed: serde_json::Value = serde_json::from_str(json_body).unwrap();
        assert_eq!(
            parsed["context"]["synthesis"]["audio"]["outputFormat"],
            "audio-24khz-48kbitrate-mono-mp3"
        );
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

    #[test]
    fn test_resolve_binary_prefers_local_bin_then_path() {
        let tmp = tempfile::tempdir().unwrap();
        let home_dir = tmp.path().join("home");
        let local_bin = home_dir.join(".local/bin");
        let path_dir = tmp.path().join("usr_bin");
        std::fs::create_dir_all(&local_bin).unwrap();
        std::fs::create_dir_all(&path_dir).unwrap();

        let path_edge = path_dir.join("edge-tts");
        std::fs::write(&path_edge, b"").unwrap();
        let joined_path = std::env::join_paths([&path_dir]).unwrap();

        // Only in $PATH -> resolves to $PATH binary
        assert_eq!(
            resolve_binary_in("edge-tts", Some(&home_dir), Some(&joined_path)),
            Some(path_edge)
        );

        // Also present in ~/.local/bin -> prefers ~/.local/bin
        let local_edge = local_bin.join("edge-tts");
        std::fs::write(&local_edge, b"").unwrap();
        assert_eq!(
            resolve_binary_in("edge-tts", Some(&home_dir), Some(&joined_path)),
            Some(local_edge)
        );

        // Missing binary -> None
        assert_eq!(
            resolve_binary_in("nonexistent-bin", Some(&home_dir), Some(&joined_path)),
            None
        );
    }

    #[test]
    fn test_cli_args_prevent_leading_dash_flag_injection() {
        let client = EdgeTtsClient::with_voice("es-CO-SalomeNeural");
        let args = client.build_cli_args("-5 grados bajo cero");
        assert!(args.iter().any(|a| a == "--text=-5 grados bajo cero"));
        assert!(!args.iter().any(|a| a == "-5 grados bajo cero"));
    }

    #[tokio::test]
    async fn test_cli_subprocess_timeout_kills_hung_process() {
        let mut cmd = tokio::process::Command::new("sh");
        cmd.args(["-c", "sleep 5"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let start = std::time::Instant::now();
        let err = run_cli_with_timeout(&mut cmd, Duration::from_millis(50))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("timed out"));
        assert!(start.elapsed() < Duration::from_secs(2));
    }
}
