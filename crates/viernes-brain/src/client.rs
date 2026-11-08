//! Subprocess NDJSON streaming client for Antigravity.

use futures::{Stream, StreamExt};
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio_stream::wrappers::LinesStream;
use tracing::{debug, error, info, warn};

use crate::error::BrainError;

/// Inactivity timeout threshold for tokens stream (60 seconds).
pub const INACTIVITY_TIMEOUT: Duration = Duration::from_secs(60);

/// Graceful shutdown timeout before sending SIGKILL (3 seconds).
pub const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(3);

/// Canonical system prompt defining Viernes voice-optimized persona and rules.
pub const VIERNES_SYSTEM_PROMPT: &str = "\
Sos Viernes, una asistente personal por voz inteligente, precisa y directa. \
Tus respuestas deben estar optimizadas exclusivamente para síntesis de voz (TTS): \
1. Sé concisa y responde en una o dos oraciones siempre que sea posible. \
2. No uses formato Markdown (sin negritas, sin cursivas, sin encabezados, sin viñetas ni bloques de código). \
3. No uses emojis ni caracteres especiales extraños. \
4. Expresa números, fechas y horas en texto natural fluido. \
5. Si necesitas listar elementos, hazlo en prosa conversacional breve (máximo 3 elementos). \
6. Trata al usuario siempre con la máxima deferencia dirigiéndote a él exclusivamente como 'Señor' (nunca por su nombre de pila ni de manera informal).";

/// Typed NDJSON events emitted by Antigravity streaming process.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BrainEvent {
    /// Internal reasoning token or status update.
    Thought {
        /// Content of the thought.
        content: String,
    },
    /// Incremental natural language response chunk.
    TextChunk {
        /// Text fragment.
        content: String,
    },
    /// Subagent or tool execution intent.
    ToolCall {
        /// Name of the tool.
        name: String,
        /// Tool input arguments as JSON.
        args: serde_json::Value,
    },
    /// Subprocess or engine error message.
    Error {
        /// Error diagnostic message.
        message: String,
    },
    /// Completion marker for the current response stream.
    Done,
}

/// Asynchronous streaming client communicating over NDJSON with `antigravity-cli`.
pub struct AntigravityClient {
    child: Child,
    stdin: ChildStdin,
    lines_stream: LinesStream<BufReader<ChildStdout>>,
    binary_path: PathBuf,
    active_conversation_id: Option<String>,
    turns_sent: usize,
}

impl AntigravityClient {
    /// Return the path to the configured antigravity binary.
    #[must_use]
    pub fn binary_path(&self) -> &Path {
        &self.binary_path
    }

    /// Return the active conversation ID confirmed by Antigravity, if known.
    #[must_use]
    pub fn conversation_id(&self) -> Option<&str> {
        self.active_conversation_id.as_deref()
    }

    /// Spawn a new `antigravity-cli` child process with piped standard I/O streams.
    pub fn spawn<P: AsRef<Path>>(binary_path: P, args: &[String]) -> Result<Self, BrainError> {
        let path_buf = binary_path.as_ref().to_path_buf();
        info!(binary = %path_buf.display(), "Spawning Antigravity process");

        let mut child = Command::new(&path_buf)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(BrainError::ProcessSpawn)?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| BrainError::Internal("failed to capture child stdin".into()))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| BrainError::Internal("failed to capture child stdout".into()))?;

        let lines_stream = LinesStream::new(BufReader::new(stdout).lines());

        Ok(Self {
            child,
            stdin,
            lines_stream,
            binary_path: path_buf,
            active_conversation_id: None,
            turns_sent: 0,
        })
    }

    /// Send a user prompt line to the process stdin.
    pub async fn send_prompt(&mut self, prompt: &str) -> Result<(), BrainError> {
        let trimmed = prompt.trim();
        debug!(
            prompt_len = trimmed.len(),
            turns_sent = self.turns_sent,
            "Writing prompt line to Antigravity stdin"
        );
        let payload = if trimmed.starts_with('{') && trimmed.ends_with('}') {
            format!("{}\n", trimmed)
        } else {
            // For turn 1 of a session, provide concise voice persona instructions.
            // Subsequent turns send only the user speech to preserve context and clean conversation titles.
            let content = if self.turns_sent == 0 {
                format!(
                    "[Instrucciones de Voz: Sos Viernes, la asistente de voz del Señor. Respondé siempre en 1 o 2 oraciones breves para TTS, sin markdown ni viñetas, tratándolo exclusivamente como 'Señor'.]\n\n{}",
                    trimmed
                )
            } else {
                trimmed.to_string()
            };
            let msg = serde_json::json!({
                "event": "user",
                "message": {
                    "content": content
                }
            });
            format!("{}\n", serde_json::to_string(&msg).unwrap_or_default())
        };

        self.turns_sent += 1;
        self.stdin
            .write_all(payload.as_bytes())
            .await
            .map_err(BrainError::ProcessSpawn)?;
        self.stdin.flush().await.map_err(BrainError::ProcessSpawn)?;
        Ok(())
    }

    /// Read the next event from the stream, bounded by the 60-second inactivity timeout.
    pub async fn next_event(&mut self) -> Result<Option<BrainEvent>, BrainError> {
        loop {
            match tokio::time::timeout(INACTIVITY_TIMEOUT, self.lines_stream.next()).await {
                Ok(Some(Ok(line))) => {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }

                    // Silently ignore non-JSON lines (e.g. CLI banners, environment notices, or raw logs)
                    if !trimmed.starts_with('{') {
                        debug!(raw_line = %trimmed, "Ignoring non-JSON output line from child process");
                        continue;
                    }

                    debug!(raw_line = %trimmed, "Received NDJSON line from child process");

                    // 1. Try parsing agy stream-json protocol
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
                        if let Some(event_str) = value.get("event").and_then(|v| v.as_str()) {
                            match event_str {
                                "init" => {
                                    if let Some(conv_id) =
                                        value.get("conversation_id").and_then(|v| v.as_str())
                                    {
                                        debug!(conversation_id = %conv_id, "Antigravity session confirmed");
                                        self.active_conversation_id = Some(conv_id.to_string());
                                    }
                                    continue;
                                }
                                "step_update" => {
                                    if let Some(su) = value.get("step_update") {
                                        let step_type =
                                            su.get("step_type").and_then(|v| v.as_str());
                                        let state = su.get("state").and_then(|v| v.as_str());
                                        let delta = su.get("text_delta").and_then(|v| v.as_str());

                                        if step_type == Some("tool") && state == Some("ACTIVE") {
                                            let tool_name = su
                                                .get("tool_name")
                                                .and_then(|v| v.as_str())
                                                .unwrap_or("tool");
                                            return Ok(Some(BrainEvent::ToolCall {
                                                name: tool_name.to_string(),
                                                args: serde_json::Value::Null,
                                            }));
                                        }

                                        if let Some(text) = delta {
                                            if !text.is_empty() {
                                                return Ok(Some(BrainEvent::TextChunk {
                                                    content: text.to_string(),
                                                }));
                                            }
                                        }
                                    }
                                    continue;
                                }
                                "result" => {
                                    return Ok(Some(BrainEvent::Done));
                                }
                                _ => continue,
                            }
                        }
                    }

                    // 2. Direct BrainEvent deserialization fallback (e.g. tests & mock servers)
                    match serde_json::from_str::<BrainEvent>(trimmed) {
                        Ok(event) => return Ok(Some(event)),
                        Err(e) => {
                            debug!(error = %e, raw_line = %trimmed, "Ignoring unhandled or malformed JSON line from child process");
                            continue;
                        }
                    }
                }
                Ok(Some(Err(io_err))) => return Err(BrainError::ProcessSpawn(io_err)),
                Ok(None) => {
                    let exit_status = self.child.try_wait().map_err(BrainError::ProcessSpawn)?;
                    if let Some(status) = exit_status {
                        if !status.success() {
                            let stderr_content = self.read_remaining_stderr().await;
                            return Err(BrainError::ProcessPrematureExit {
                                code: status.code(),
                                stderr: stderr_content,
                            });
                        }
                    }
                    return Ok(None);
                }
                Err(_) => {
                    let secs = INACTIVITY_TIMEOUT.as_secs();
                    error!(
                        timeout_secs = secs,
                        "Antigravity process inactive for >{secs}s; raising inactivity timeout"
                    );
                    return Err(BrainError::InactivityTimeout {
                        duration_secs: secs,
                    });
                }
            }
        }
    }

    /// Read any accumulated stderr output for diagnostics.
    async fn read_remaining_stderr(&mut self) -> String {
        if let Some(mut stderr) = self.child.stderr.take() {
            let mut buf = Vec::new();
            use tokio::io::AsyncReadExt;
            let _ = stderr.read_to_end(&mut buf).await;
            String::from_utf8_lossy(&buf).into_owned()
        } else {
            String::new()
        }
    }

    /// Drain any residual unconsumed lines left in the stdout stream buffer.
    pub async fn drain_residual_lines(&mut self) {
        while let Ok(Some(_)) =
            tokio::time::timeout(Duration::from_millis(10), self.lines_stream.next()).await
        {
        }
    }

    /// Terminate the child process cleanly with SIGTERM -> SIGKILL if needed.
    pub async fn terminate(&mut self) -> Result<(), BrainError> {
        if let Some(pid_val) = self.child.id() {
            let pid = Pid::from_raw(pid_val as i32);
            info!(pid = %pid_val, "Terminating Antigravity process with SIGTERM");
            let _ = kill(pid, Signal::SIGTERM);

            match tokio::time::timeout(SHUTDOWN_TIMEOUT, self.child.wait()).await {
                Ok(Ok(status)) => {
                    info!(code = ?status.code(), "Antigravity process exited cleanly");
                }
                _ => {
                    warn!(pid = %pid_val, "Process did not terminate within 3s; forcing SIGKILL");
                    let _ = kill(pid, Signal::SIGKILL);
                    let _ = self.child.wait().await;
                }
            }
        }
        Ok(())
    }

    /// Perform clean termination: SIGTERM -> wait up to 3 seconds -> SIGKILL if needed.
    pub async fn shutdown(mut self) -> Result<(), BrainError> {
        self.terminate().await
    }
}

/// Simulated Antigravity NDJSON engine for unit and integration testing without binary dependencies.
pub struct MockAntigravityServer {
    events_to_emit: Vec<BrainEvent>,
}

impl MockAntigravityServer {
    /// Create a new mock server configured with predetermined events.
    #[must_use]
    pub fn new(events: Vec<BrainEvent>) -> Self {
        Self {
            events_to_emit: events,
        }
    }

    /// Return an async stream of NDJSON serialized lines.
    pub fn into_event_stream(
        self,
    ) -> impl Stream<Item = Result<BrainEvent, BrainError>> + Send + 'static {
        futures::stream::iter(self.events_to_emit.into_iter().map(Ok))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serde_brain_events_roundtrip() {
        let events = vec![
            BrainEvent::Thought {
                content: "Consultando contexto de usuario...".into(),
            },
            BrainEvent::TextChunk {
                content: "Hola Señor, ¿en qué puedo ayudarle?".into(),
            },
            BrainEvent::ToolCall {
                name: "run_command".into(),
                args: serde_json::json!({"command": "ls -la"}),
            },
            BrainEvent::Error {
                message: "API quota exceeded".into(),
            },
            BrainEvent::Done,
        ];

        for ev in events {
            let json = serde_json::to_string(&ev).expect("serialization failed");
            let deserialized: BrainEvent =
                serde_json::from_str(&json).expect("deserialization failed");
            assert_eq!(ev, deserialized);
        }
    }

    #[tokio::test]
    async fn test_mock_antigravity_streaming() {
        let expected = vec![
            BrainEvent::Thought {
                content: "Analizando".into(),
            },
            BrainEvent::TextChunk {
                content: "Buenas noches".into(),
            },
            BrainEvent::Done,
        ];

        let mock = MockAntigravityServer::new(expected.clone());
        let mut stream = Box::pin(mock.into_event_stream());

        let mut collected = Vec::new();
        while let Some(res) = stream.next().await {
            collected.push(res.unwrap());
        }

        assert_eq!(collected, expected);
    }
}
