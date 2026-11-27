//! Subprocess NDJSON streaming client for Antigravity.

use futures::{Stream, StreamExt};
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio_stream::wrappers::LinesStream;
use tracing::{debug, error, info, warn};

use crate::error::BrainError;

/// Inactivity timeout threshold for tokens stream (120 seconds).
pub const INACTIVITY_TIMEOUT: Duration = Duration::from_secs(120);

/// Graceful shutdown timeout before sending SIGKILL (3 seconds).
pub const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(3);

/// Maximum number of stderr bytes retained in the background drain buffer (16 KiB).
const STDERR_BUFFER_CAP: usize = 16 * 1024;

/// Timeout while waiting for a child process to exit after stdout reaches EOF.
const EOF_WAIT_TIMEOUT: Duration = Duration::from_millis(500);

/// Grace period for the stderr drain task to flush trailing bytes after child exit.
const STDERR_DRAIN_GRACE: Duration = Duration::from_millis(50);

/// Canonical system prompt defining Viernes voice-optimized persona and rules.
pub const VIERNES_SYSTEM_PROMPT: &str = "\
Sos Viernes, una asistente personal por voz inteligente, enérgica, ágil, vivaz y con mucha chispa y entusiasmo. \
Tus respuestas deben estar optimizadas exclusivamente para síntesis de voz (TTS): \
1. Sé concisa y responde en una o dos oraciones con energía y rapidez. \
2. No uses formato Markdown (sin negritas, sin cursivas, sin encabezados, sin viñetas ni bloques de código). \
3. No uses emojis ni caracteres especiales extraños. \
4. Expresa números, fechas y horas en texto natural fluido. \
5. Si necesitas listar elementos, hazlo en prosa conversacional breve (máximo 3 elementos). \
6. Trata al usuario siempre con la máxima deferencia dirigiéndote a él como 'Señor', con una actitud alegre, resolutiva y bien despierta.";

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
    stderr_buf: Arc<Mutex<Vec<u8>>>,
    stderr_task: Option<JoinHandle<()>>,
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

        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| BrainError::Internal("failed to capture child stderr".into()))?;

        let stderr_buf = Arc::new(Mutex::new(Vec::new()));
        let stderr_buf_clone = Arc::clone(&stderr_buf);
        let stderr_task = if let Ok(handle) = tokio::runtime::Handle::try_current() {
            Some(handle.spawn(async move {
                drain_stderr(stderr, stderr_buf_clone).await;
            }))
        } else {
            warn!("No active Tokio runtime during spawn; stderr background drain task disabled");
            None
        };

        let lines_stream = LinesStream::new(BufReader::new(stdout).lines());

        Ok(Self {
            child,
            stdin,
            lines_stream,
            stderr_buf,
            stderr_task,
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
        let is_raw_ndjson = !trimmed.contains('\n')
            && !trimmed.contains('\r')
            && matches!(
                serde_json::from_str::<serde_json::Value>(trimmed),
                Ok(serde_json::Value::Object(_))
            );

        let payload = if is_raw_ndjson {
            format!("{trimmed}\n")
        } else {
            // For turn 1 of a session, provide concise voice persona instructions.
            // Subsequent turns send only the user speech to preserve context and clean conversation titles.
            let content = if self.turns_sent == 0 {
                format!(
                    "[Instrucciones de Voz: Sos Viernes, la asistente de voz del Señor. Respondé siempre en 1 o 2 oraciones breves para TTS, sin markdown ni viñetas, tratándolo exclusivamente como 'Señor'.]\n\n{trimmed}"
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
            let serialized = serde_json::to_string(&msg).map_err(|e| {
                BrainError::ProtocolError(format!("failed to serialize prompt payload: {e}"))
            })?;
            format!("{serialized}\n")
        };

        self.stdin
            .write_all(payload.as_bytes())
            .await
            .map_err(BrainError::ProcessSpawn)?;
        self.stdin.flush().await.map_err(BrainError::ProcessSpawn)?;
        self.turns_sent += 1;
        Ok(())
    }

    /// Read the next event from the stream, bounded by the 120-second inactivity timeout.
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
                    let parsed_value = serde_json::from_str::<serde_json::Value>(trimmed);
                    if let Ok(ref value) = parsed_value {
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
                                                .or_else(|| su.get("name"))
                                                .and_then(|v| v.as_str())
                                                .unwrap_or("tool");
                                            let mut args = extract_tool_args(su);
                                            if args.is_null() {
                                                args = extract_tool_args(value);
                                            }
                                            return Ok(Some(BrainEvent::ToolCall {
                                                name: tool_name.to_string(),
                                                args,
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
                                "tool_call" => {
                                    let payload = value.get("tool_call").unwrap_or(value);
                                    let tool_name = payload
                                        .get("name")
                                        .or_else(|| payload.get("tool_name"))
                                        .or_else(|| value.get("name"))
                                        .or_else(|| value.get("tool_name"))
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("tool");
                                    let mut args = extract_tool_args(payload);
                                    if args.is_null() {
                                        args = extract_tool_args(value);
                                    }
                                    return Ok(Some(BrainEvent::ToolCall {
                                        name: tool_name.to_string(),
                                        args,
                                    }));
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
                            if let Ok(ref value) = parsed_value {
                                if value.get("type").and_then(|v| v.as_str()) == Some("tool_call") {
                                    let payload = value.get("tool_call").unwrap_or(value);
                                    let tool_name = payload
                                        .get("name")
                                        .or_else(|| payload.get("tool_name"))
                                        .or_else(|| value.get("name"))
                                        .or_else(|| value.get("tool_name"))
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("tool");
                                    let mut args = extract_tool_args(payload);
                                    if args.is_null() {
                                        args = extract_tool_args(value);
                                    }
                                    return Ok(Some(BrainEvent::ToolCall {
                                        name: tool_name.to_string(),
                                        args,
                                    }));
                                }
                            }
                            debug!(error = %e, raw_line = %trimmed, "Ignoring unhandled or malformed JSON line from child process");
                            continue;
                        }
                    }
                }
                Ok(Some(Err(io_err))) => return Err(BrainError::ProcessSpawn(io_err)),
                Ok(None) => {
                    match tokio::time::timeout(EOF_WAIT_TIMEOUT, self.child.wait()).await {
                        Ok(Ok(status)) => {
                            if !status.success() {
                                let stderr_content = self.read_remaining_stderr().await;
                                return Err(BrainError::ProcessPrematureExit {
                                    code: status.code(),
                                    stderr: stderr_content,
                                });
                            }
                        }
                        Ok(Err(io_err)) => return Err(BrainError::ProcessSpawn(io_err)),
                        Err(_) => {
                            warn!("Antigravity stdout closed but child process did not exit within 500ms");
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

    /// Read accumulated stderr output from the bounded background drain buffer.
    async fn read_remaining_stderr(&mut self) -> String {
        if let Some(handle) = self.stderr_task.take() {
            let abort_handle = handle.abort_handle();
            if tokio::time::timeout(STDERR_DRAIN_GRACE, handle)
                .await
                .is_err()
            {
                abort_handle.abort();
            }
        }
        let guard = self.stderr_buf.lock().await;
        String::from_utf8_lossy(&guard).into_owned()
    }

    /// Drain any residual unconsumed lines left in the stdout stream buffer,
    /// while preserving any init event to capture the active conversation ID.
    pub async fn drain_residual_lines(&mut self) {
        while let Ok(Some(Ok(line))) =
            tokio::time::timeout(Duration::from_millis(10), self.lines_stream.next()).await
        {
            let trimmed = line.trim();
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
                if value.get("event").and_then(|v| v.as_str()) == Some("init") {
                    if let Some(conv_id) = value.get("conversation_id").and_then(|v| v.as_str()) {
                        debug!(conversation_id = %conv_id, "Captured conversation_id during buffer drain");
                        self.active_conversation_id = Some(conv_id.to_string());
                    }
                }
            }
        }
    }

    /// Terminate the child process cleanly with SIGTERM -> SIGKILL if needed.
    pub async fn terminate(&mut self) -> Result<(), BrainError> {
        if let Some(pid_val) = self.child.id() {
            let pid = Pid::from_raw(pid_val as i32);
            info!(pid = %pid_val, "Terminating Antigravity process with SIGTERM");
            if let Err(e) = kill(pid, Signal::SIGTERM) {
                warn!(pid = %pid_val, error = %e, "Failed to send SIGTERM to Antigravity process");
            }

            match tokio::time::timeout(SHUTDOWN_TIMEOUT, self.child.wait()).await {
                Ok(Ok(status)) => {
                    info!(code = ?status.code(), "Antigravity process exited cleanly");
                }
                Ok(Err(e)) => {
                    warn!(pid = %pid_val, error = %e, "Failed waiting for Antigravity process after SIGTERM");
                }
                Err(_) => {
                    warn!(pid = %pid_val, "Process did not terminate within 3s; forcing SIGKILL");
                    if let Err(e) = kill(pid, Signal::SIGKILL) {
                        warn!(pid = %pid_val, error = %e, "Failed to send SIGKILL to Antigravity process");
                    }
                    match tokio::time::timeout(EOF_WAIT_TIMEOUT, self.child.wait()).await {
                        Ok(Ok(status)) => {
                            debug!(pid = %pid_val, code = ?status.code(), "Antigravity process reaped after SIGKILL");
                        }
                        Ok(Err(e)) => {
                            warn!(pid = %pid_val, error = %e, "Failed reaping Antigravity process after SIGKILL");
                        }
                        Err(_) => {
                            warn!(pid = %pid_val, "Timed out waiting to reap Antigravity process after SIGKILL");
                        }
                    }
                }
            }
        }
        if let Some(handle) = self.stderr_task.take() {
            handle.abort();
        }
        Ok(())
    }

    /// Perform clean termination: SIGTERM -> wait up to 3 seconds -> SIGKILL if needed.
    pub async fn shutdown(mut self) -> Result<(), BrainError> {
        self.terminate().await
    }
}

impl Drop for AntigravityClient {
    fn drop(&mut self) {
        if let Some(handle) = self.stderr_task.take() {
            handle.abort();
        }
    }
}

/// Continuously drain child stderr into a bounded buffer so the 64 KiB OS pipe never fills.
async fn drain_stderr(mut stderr: ChildStderr, buffer: Arc<Mutex<Vec<u8>>>) {
    let mut chunk = [0u8; 4096];
    loop {
        match stderr.read(&mut chunk).await {
            Ok(0) => break,
            Ok(n) => {
                let mut guard = buffer.lock().await;
                append_bounded(&mut guard, &chunk[..n], STDERR_BUFFER_CAP);
            }
            Err(e) => {
                debug!(error = %e, "Stopped draining child stderr");
                break;
            }
        }
    }
}

/// Append `data` to `buf`, keeping at most the most recent `cap` bytes.
fn append_bounded(buf: &mut Vec<u8>, data: &[u8], cap: usize) {
    if cap == 0 {
        buf.clear();
        return;
    }
    if data.len() >= cap {
        buf.clear();
        buf.extend_from_slice(&data[data.len() - cap..]);
        return;
    }
    let new_len = buf.len() + data.len();
    if new_len > cap {
        let excess = new_len - cap;
        buf.drain(..excess);
    }
    buf.extend_from_slice(data);
}

fn parse_maybe_json_str(val: &serde_json::Value) -> serde_json::Value {
    if let Some(s) = val.as_str() {
        let trimmed = s.trim();
        if (trimmed.starts_with('{') && trimmed.ends_with('}'))
            || (trimmed.starts_with('[') && trimmed.ends_with(']'))
        {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(trimmed) {
                return parsed;
            }
        }
    }
    val.clone()
}

fn extract_tool_args(payload: &serde_json::Value) -> serde_json::Value {
    for key in [
        "args",
        "arguments",
        "input",
        "tool_input",
        "params",
        "parameters",
    ] {
        if let Some(val) = payload.get(key) {
            if !val.is_null() {
                return parse_maybe_json_str(val);
            }
        }
    }
    for cmd_key in ["command", "cmd"] {
        if let Some(val) = payload.get(cmd_key) {
            if !val.is_null() {
                return serde_json::json!({ cmd_key: val });
            }
        }
    }
    if let Some(nested) = payload.get("tool_call") {
        let nested_args = extract_tool_args(nested);
        if !nested_args.is_null() {
            return nested_args;
        }
    }
    serde_json::Value::Null
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

    #[test]
    fn test_append_bounded_caps_at_limit_and_keeps_tail() {
        let mut buf = Vec::new();
        let first = vec![b'a'; 10_000];
        let second = vec![b'b'; 10_000];
        append_bounded(&mut buf, &first, STDERR_BUFFER_CAP);
        assert_eq!(buf.len(), 10_000);

        append_bounded(&mut buf, &second, STDERR_BUFFER_CAP);
        assert_eq!(buf.len(), STDERR_BUFFER_CAP);
        assert_eq!(
            &buf[..STDERR_BUFFER_CAP - 10_000],
            &vec![b'a'; STDERR_BUFFER_CAP - 10_000][..]
        );
        assert_eq!(&buf[STDERR_BUFFER_CAP - 10_000..], &second[..]);

        let huge = vec![b'c'; STDERR_BUFFER_CAP + 500];
        append_bounded(&mut buf, &huge, STDERR_BUFFER_CAP);
        assert_eq!(buf.len(), STDERR_BUFFER_CAP);
        assert!(buf.iter().all(|&b| b == b'c'));
    }

    #[test]
    fn test_extract_tool_args_variants() {
        assert_eq!(
            extract_tool_args(&serde_json::json!({"args": {"path": "/tmp"}})),
            serde_json::json!({"path": "/tmp"})
        );
        assert_eq!(
            extract_tool_args(&serde_json::json!({"arguments": "{\"command\":\"ls\"}"})),
            serde_json::json!({"command": "ls"})
        );
        assert_eq!(
            extract_tool_args(&serde_json::json!({"input": {"query": "rust"}})),
            serde_json::json!({"query": "rust"})
        );
        assert_eq!(
            extract_tool_args(&serde_json::json!({"command": "rm -rf /tmp/x"})),
            serde_json::json!({"command": "rm -rf /tmp/x"})
        );
        assert_eq!(
            extract_tool_args(&serde_json::json!({"tool_name": "noop"})),
            serde_json::Value::Null
        );
    }

    #[tokio::test]
    async fn test_stderr_drained_continuously_past_64kib_without_deadlock() {
        // Write >96 KiB to stderr before writing stdout result; would deadlock if stderr isn't drained.
        let script = "python3 -c 'import sys; sys.stderr.write(\"x\" * 100000); sys.stderr.flush()' 2>&2 || { i=0; while [ $i -lt 1000 ]; do printf '0123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789\\n' >&2; i=$((i+1)); done; }; printf '{\"event\":\"result\"}\\n'";
        let args = vec!["-c".to_string(), script.to_string()];
        let mut client = AntigravityClient::spawn("/bin/sh", &args).unwrap();

        let ev = tokio::time::timeout(Duration::from_secs(5), client.next_event())
            .await
            .expect("timed out - stderr likely deadlocked")
            .unwrap();
        assert_eq!(ev, Some(BrainEvent::Done));

        let eof = client.next_event().await.unwrap();
        assert_eq!(eof, None);

        let drained = client.read_remaining_stderr().await;
        assert!(!drained.is_empty());
        assert!(drained.len() <= STDERR_BUFFER_CAP);
    }

    #[tokio::test]
    async fn test_premature_exit_reaps_and_returns_drained_stderr() {
        let args = vec![
            "-c".to_string(),
            "printf 'fatal error occurred\\n' >&2; exit 7".to_string(),
        ];
        let mut client = AntigravityClient::spawn("/bin/sh", &args).unwrap();

        let err = client.next_event().await.unwrap_err();
        match err {
            BrainError::ProcessPrematureExit { code, stderr } => {
                assert_eq!(code, Some(7));
                assert!(
                    stderr.contains("fatal error occurred"),
                    "unexpected stderr: {stderr}"
                );
            }
            other => panic!("expected ProcessPrematureExit, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_next_event_parses_tool_args_from_step_update_and_tool_call() {
        let lines = [
            r#"{"event":"step_update","step_update":{"step_type":"tool","state":"ACTIVE","tool_name":"run_command","arguments":{"command":"git status"}}}"#,
            r#"{"event":"step_update","step_update":{"step_type":"tool","state":"ACTIVE","tool_name":"bash","command":"rm -rf /tmp/test"}}"#,
            r#"{"event":"tool_call","name":"search_web","input":{"query":"viernes"}}"#,
            r#"{"type":"tool_call","name":"read_file","arguments":"{\"path\":\"Cargo.toml\"}"}"#,
            r#"{"event":"result"}"#,
        ]
        .join("\n");
        let args = vec!["-c".to_string(), format!("printf '%s\\n' '{lines}'")];
        let mut client = AntigravityClient::spawn("/bin/sh", &args).unwrap();

        assert_eq!(
            client.next_event().await.unwrap(),
            Some(BrainEvent::ToolCall {
                name: "run_command".into(),
                args: serde_json::json!({"command": "git status"}),
            })
        );
        assert_eq!(
            client.next_event().await.unwrap(),
            Some(BrainEvent::ToolCall {
                name: "bash".into(),
                args: serde_json::json!({"command": "rm -rf /tmp/test"}),
            })
        );
        assert_eq!(
            client.next_event().await.unwrap(),
            Some(BrainEvent::ToolCall {
                name: "search_web".into(),
                args: serde_json::json!({"query": "viernes"}),
            })
        );
        assert_eq!(
            client.next_event().await.unwrap(),
            Some(BrainEvent::ToolCall {
                name: "read_file".into(),
                args: serde_json::json!({"path": "Cargo.toml"}),
            })
        );
        assert_eq!(client.next_event().await.unwrap(), Some(BrainEvent::Done));
        assert_eq!(client.next_event().await.unwrap(), None);
    }

    #[tokio::test]
    async fn test_send_prompt_ndjson_wrapping_and_turns_sent() {
        // Echo stdin lines back to stdout using cat so we can inspect exact wire framing.
        let mut client = AntigravityClient::spawn("/bin/cat", &[]).unwrap();

        // 1. Invalid JSON braces should be wrapped safely (and turn 0 includes voice instructions)
        client.send_prompt("{hello}").await.unwrap();
        assert_eq!(client.turns_sent, 1);
        let line1 = client.lines_stream.next().await.unwrap().unwrap();
        let v1: serde_json::Value = serde_json::from_str(&line1).unwrap();
        assert_eq!(v1["event"], "user");
        let content1 = v1["message"]["content"].as_str().unwrap();
        assert!(content1.contains("Instrucciones de Voz"));
        assert!(content1.contains("{hello}"));

        // 2. Multi-line JSON must NOT be sent as raw NDJSON (prevents line injection)
        client.send_prompt("{\"a\":1}\n{\"b\":2}").await.unwrap();
        assert_eq!(client.turns_sent, 2);
        let line2 = client.lines_stream.next().await.unwrap().unwrap();
        let v2: serde_json::Value = serde_json::from_str(&line2).unwrap();
        assert_eq!(v2["event"], "user");
        assert_eq!(v2["message"]["content"], "{\"a\":1}\n{\"b\":2}");

        // 3. Bare JSON scalar must be wrapped safely
        client.send_prompt("42").await.unwrap();
        assert_eq!(client.turns_sent, 3);
        let line3 = client.lines_stream.next().await.unwrap().unwrap();
        let v3: serde_json::Value = serde_json::from_str(&line3).unwrap();
        assert_eq!(v3["event"], "user");
        assert_eq!(v3["message"]["content"], "42");

        // 4. Valid single-line JSON object is passed through as raw NDJSON
        let raw = r#"{"event":"user","message":{"content":"raw"}}"#;
        client.send_prompt(raw).await.unwrap();
        assert_eq!(client.turns_sent, 4);
        let line4 = client.lines_stream.next().await.unwrap().unwrap();
        assert_eq!(line4, raw);

        client.terminate().await.unwrap();
    }

    #[tokio::test]
    async fn test_send_prompt_does_not_increment_turns_sent_on_write_error() {
        let args = vec!["-c".to_string(), "exit 0".to_string()];
        let mut client = AntigravityClient::spawn("/bin/sh", &args).unwrap();
        // Wait until child exits so stdin pipe is broken
        let _ = client.child.wait().await;

        let res = client.send_prompt("Hola Viernes").await;
        assert!(res.is_err());
        assert_eq!(client.turns_sent, 0);
    }
}
