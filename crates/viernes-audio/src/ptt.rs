//! Push-to-Talk (PTT) keyboard shortcut listener and capture coordination.
//!
//! Provides deterministic keyboard press/release lifecycle management, sub-10ms capture initiation,
//! instantaneous buffer termination on key release, and precedence arbitration over active VAD.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use crossbeam_channel::{bounded, Receiver, Sender};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::net::UnixListener;
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};
use viernes_core::error::ViernesError;
use viernes_core::{default_ptt_socket_path, ensure_private_dir};

const MAX_IPC_CLIENTS: usize = 16;
const MAX_IPC_LINE_BYTES: usize = 256;
const IPC_READ_TIMEOUT: Duration = Duration::from_secs(2);

/// Push-to-talk discrete key events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PttEvent {
    /// Hotkey depressed (commence capture immediately).
    Pressed,
    /// Hotkey released (cut capture immediately and emit buffer).
    Released,
    /// Active voice turn cancelled (abort capture/thinking/playback).
    Cancel,
    /// Configuration or runtime state reload requested.
    Reload,
}

/// Abstract Push-to-Talk hotkey listener.
#[async_trait]
pub trait PttListener: Send + Sync {
    /// Starts background key monitoring.
    async fn start(&mut self) -> Result<(), ViernesError>;

    /// Stops key monitoring and releases background listener resources.
    async fn stop(&mut self) -> Result<(), ViernesError>;

    /// Indicates whether the listener is active.
    fn is_active(&self) -> bool;

    /// Obtains a receiver channel for incoming PTT state transitions.
    fn event_receiver(&self) -> Receiver<PttEvent>;
}

/// Hermetic simulated PTT listener for unit testing and CI test harnesses.
#[derive(Debug, Clone)]
pub struct MockPttListener {
    sender: Sender<PttEvent>,
    receiver: Receiver<PttEvent>,
    active: Arc<AtomicBool>,
}

impl MockPttListener {
    /// Creates a new mock PTT listener with a bounded event channel.
    #[must_use]
    pub fn new() -> Self {
        let (sender, receiver) = bounded(32);
        Self {
            sender,
            receiver,
            active: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Simulates pressing the global hotkey.
    pub fn trigger_press(&self) {
        let _ = self.sender.send(PttEvent::Pressed);
    }

    /// Simulates releasing the global hotkey.
    pub fn trigger_release(&self) {
        let _ = self.sender.send(PttEvent::Released);
    }

    /// Simulates cancelling the active voice turn.
    pub fn trigger_cancel(&self) {
        let _ = self.sender.send(PttEvent::Cancel);
    }

    /// Simulates requesting a configuration/runtime reload.
    pub fn trigger_reload(&self) {
        let _ = self.sender.send(PttEvent::Reload);
    }
}

impl Default for MockPttListener {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PttListener for MockPttListener {
    async fn start(&mut self) -> Result<(), ViernesError> {
        self.active.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn stop(&mut self) -> Result<(), ViernesError> {
        self.active.store(false, Ordering::SeqCst);
        Ok(())
    }

    fn is_active(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }

    fn event_receiver(&self) -> Receiver<PttEvent> {
        self.receiver.clone()
    }
}

fn remove_existing_socket(path: &Path) -> Result<(), ViernesError> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::FileTypeExt;
                if meta.file_type().is_socket() {
                    std::fs::remove_file(path).map_err(|e| {
                        ViernesError::Audio(format!(
                            "Failed to remove stale PTT IPC socket {:?}: {e}",
                            path
                        ))
                    })?;
                    return Ok(());
                }
            }
            let _ = meta;
            Err(ViernesError::Audio(format!(
                "Refusing to remove non-socket path at {:?}",
                path
            )))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(ViernesError::Audio(format!(
            "Failed to inspect PTT IPC socket path {:?}: {e}",
            path
        ))),
    }
}

/// Real-time Unix domain socket PTT listener for Wayland / Hyprland global shortcuts.
pub struct IpcSocketPttListener {
    socket_path: PathBuf,
    sender: Sender<PttEvent>,
    receiver: Receiver<PttEvent>,
    active: Arc<AtomicBool>,
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
    server_handle: Option<JoinHandle<()>>,
}

impl IpcSocketPttListener {
    /// Constructs a new Unix Domain Socket PTT listener at the given path.
    pub fn new<P: AsRef<Path>>(socket_path: P) -> Self {
        let (sender, receiver) = bounded(32);
        Self {
            socket_path: socket_path.as_ref().to_path_buf(),
            sender,
            receiver,
            active: Arc::new(AtomicBool::new(false)),
            shutdown_tx: None,
            server_handle: None,
        }
    }

    /// Constructs a default Unix Domain Socket PTT listener from `VIERNES_PTT_SOCKET`
    /// or [`viernes_core::default_ptt_socket_path`].
    #[must_use]
    pub fn from_env() -> Self {
        let path = std::env::var("VIERNES_PTT_SOCKET")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(default_ptt_socket_path);
        Self::new(path)
    }

    /// Returns the configured Unix Domain Socket path.
    #[must_use]
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }
}

#[async_trait]
impl PttListener for IpcSocketPttListener {
    async fn start(&mut self) -> Result<(), ViernesError> {
        if self.active.load(Ordering::SeqCst) {
            return Ok(());
        }

        if let Some(parent) = self.socket_path.parent() {
            if !parent.as_os_str().is_empty() {
                ensure_private_dir(parent).map_err(|e| {
                    ViernesError::Audio(format!(
                        "Failed to prepare private directory for PTT IPC socket {:?}: {e}",
                        self.socket_path
                    ))
                })?;
            }
        }

        // Clean up stale socket file only if it is actually a Unix socket
        remove_existing_socket(&self.socket_path)?;

        let listener = UnixListener::bind(&self.socket_path).map_err(|e| {
            ViernesError::Audio(format!(
                "Failed to bind PTT IPC socket at {:?}: {e}",
                self.socket_path
            ))
        })?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&self.socket_path, std::fs::Permissions::from_mode(0o600))
                .map_err(|e| {
                    ViernesError::Audio(format!(
                        "Failed to set 0600 permissions on PTT IPC socket {:?}: {e}",
                        self.socket_path
                    ))
                })?;
        }

        info!("PTT IPC server listening on {:?}", self.socket_path);
        self.active.store(true, Ordering::SeqCst);

        let active = self.active.clone();
        let sender = self.sender.clone();
        let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel();
        self.shutdown_tx = Some(shutdown_tx);
        let client_semaphore = Arc::new(Semaphore::new(MAX_IPC_CLIENTS));

        let handle = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => {
                        debug!("PTT IPC server received shutdown signal");
                        break;
                    }
                    accept_res = listener.accept() => {
                        match accept_res {
                            Ok((stream, _)) => {
                                let Ok(permit) = client_semaphore.clone().try_acquire_owned() else {
                                    warn!(
                                        max_clients = MAX_IPC_CLIENTS,
                                        "PTT IPC max concurrent clients reached; dropping connection"
                                    );
                                    drop(stream);
                                    continue;
                                };
                                let s = sender.clone();
                                tokio::spawn(async move {
                                    let _permit = permit;
                                    let mut reader = BufReader::new(stream);
                                    let mut buf = Vec::with_capacity(64);
                                    loop {
                                        buf.clear();
                                        let read_res = tokio::time::timeout(
                                            IPC_READ_TIMEOUT,
                                            (&mut reader)
                                                .take((MAX_IPC_LINE_BYTES + 1) as u64)
                                                .read_until(b'\n', &mut buf),
                                        )
                                        .await;

                                        match read_res {
                                            Ok(Ok(0)) => break,
                                            Ok(Ok(_)) => {
                                                if buf.len() > MAX_IPC_LINE_BYTES {
                                                    warn!(
                                                        bytes = buf.len(),
                                                        limit = MAX_IPC_LINE_BYTES,
                                                        "PTT IPC client line exceeded byte limit; closing connection"
                                                    );
                                                    break;
                                                }
                                                let Ok(line) = std::str::from_utf8(&buf) else {
                                                    warn!("PTT IPC client sent invalid UTF-8; closing connection");
                                                    break;
                                                };
                                                let cmd = line.trim().to_ascii_lowercase();
                                                let event = match cmd.as_str() {
                                                    "press" | "pressed" | "start" => {
                                                        Some(PttEvent::Pressed)
                                                    }
                                                    "release" | "released" | "finish" | "stop" => {
                                                        Some(PttEvent::Released)
                                                    }
                                                    "cancel" => Some(PttEvent::Cancel),
                                                    "reload" => Some(PttEvent::Reload),
                                                    _ => None,
                                                };
                                                if let Some(ev) = event {
                                                    if let Err(err) = s.try_send(ev) {
                                                        warn!(
                                                            event = ?ev,
                                                            error = %err,
                                                            "PTT IPC event channel full or disconnected; dropping event"
                                                        );
                                                    }
                                                }
                                            }
                                            Ok(Err(err)) => {
                                                debug!("PTT IPC client read error: {err}");
                                                break;
                                            }
                                            Err(_) => {
                                                debug!(
                                                    timeout = ?IPC_READ_TIMEOUT,
                                                    "PTT IPC client read timed out"
                                                );
                                                break;
                                            }
                                        }
                                    }
                                });
                            }
                            Err(err) => {
                                warn!("PTT IPC accept error: {err}");
                            }
                        }
                    }
                }
            }
            active.store(false, Ordering::SeqCst);
        });
        self.server_handle = Some(handle);

        Ok(())
    }

    async fn stop(&mut self) -> Result<(), ViernesError> {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        if let Some(handle) = self.server_handle.take() {
            let _ = handle.await;
        }
        if let Err(err) = remove_existing_socket(&self.socket_path) {
            warn!("PTT IPC stop socket cleanup failed: {err}");
        }
        self.active.store(false, Ordering::SeqCst);
        Ok(())
    }

    fn is_active(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }

    fn event_receiver(&self) -> Receiver<PttEvent> {
        self.receiver.clone()
    }
}

/// Active mode governing audio capture precedence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CaptureArbitrationMode {
    /// Neither VAD nor PTT is capturing.
    #[default]
    Idle,
    /// VAD is actively capturing speech automatically.
    VadActive,
    /// Push-to-Talk has seized exclusive priority.
    PttActive,
}

/// Arbitrator enforcing deterministic precedence of Push-to-Talk over automatic VAD.
pub struct PttArbitrator {
    mode: CaptureArbitrationMode,
}

impl Default for PttArbitrator {
    fn default() -> Self {
        Self::new()
    }
}

impl PttArbitrator {
    /// Creates a new capture arbitrator in quiescent state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            mode: CaptureArbitrationMode::Idle,
        }
    }

    /// Current arbitration status.
    #[must_use]
    pub fn mode(&self) -> CaptureArbitrationMode {
        self.mode
    }

    /// Handles VAD voice onset notification.
    ///
    /// If PTT is active, VAD onset is suppressed because PTT has total precedence.
    /// Returns true if VAD should proceed with speech capture.
    pub fn on_vad_started(&mut self) -> bool {
        match self.mode {
            CaptureArbitrationMode::Idle => {
                self.mode = CaptureArbitrationMode::VadActive;
                true
            }
            CaptureArbitrationMode::VadActive => true,
            CaptureArbitrationMode::PttActive => {
                debug!("Arbitrator: VAD voice onset discarded; Push-to-Talk has precedence");
                false
            }
        }
    }

    /// Handles VAD speech completion.
    pub fn on_vad_finished(&mut self) {
        if self.mode == CaptureArbitrationMode::VadActive {
            self.mode = CaptureArbitrationMode::Idle;
        }
    }

    /// Handles Push-to-Talk hotkey depression.
    ///
    /// Enforces immediate deterministic override over any active VAD session.
    pub fn on_ptt_pressed(&mut self) {
        if self.mode == CaptureArbitrationMode::VadActive {
            info!("Arbitrator: Push-to-Talk preempted ongoing VAD capture session");
        }
        self.mode = CaptureArbitrationMode::PttActive;
    }

    /// Handles Push-to-Talk hotkey release.
    pub fn on_ptt_released(&mut self) {
        self.mode = CaptureArbitrationMode::Idle;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ptt_listener_events() {
        let listener = MockPttListener::new();
        let rx = listener.event_receiver();

        listener.trigger_press();
        assert_eq!(rx.try_recv(), Ok(PttEvent::Pressed));

        listener.trigger_release();
        assert_eq!(rx.try_recv(), Ok(PttEvent::Released));
    }

    #[test]
    fn test_arbitrator_precedence_ptt_over_vad() {
        let mut arb = PttArbitrator::new();
        assert_eq!(arb.mode(), CaptureArbitrationMode::Idle);

        // VAD starts
        assert!(arb.on_vad_started());
        assert_eq!(arb.mode(), CaptureArbitrationMode::VadActive);

        // While VAD is active, user presses Push-to-Talk
        arb.on_ptt_pressed();
        assert_eq!(arb.mode(), CaptureArbitrationMode::PttActive);

        // Subsequent VAD events are ignored/suppressed
        assert!(!arb.on_vad_started());
        assert_eq!(arb.mode(), CaptureArbitrationMode::PttActive);

        // Releasing PTT returns system to Idle
        arb.on_ptt_released();
        assert_eq!(arb.mode(), CaptureArbitrationMode::Idle);
    }

    #[tokio::test]
    async fn test_ipc_socket_ptt_listener() {
        let temp_dir = tempfile::tempdir().unwrap();
        let sock_path = temp_dir.path().join("test_ptt.sock");
        let mut listener = IpcSocketPttListener::new(&sock_path);
        let rx = listener.event_receiver();

        assert!(listener.start().await.is_ok());
        assert!(listener.is_active());

        use tokio::io::AsyncWriteExt;
        let mut stream = tokio::net::UnixStream::connect(&sock_path).await.unwrap();
        stream.write_all(b"press\n").await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        assert_eq!(rx.try_recv(), Ok(PttEvent::Pressed));

        stream.write_all(b"release\n").await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        assert_eq!(rx.try_recv(), Ok(PttEvent::Released));

        assert!(listener.stop().await.is_ok());
        assert!(!listener.is_active());
    }

    #[tokio::test]
    async fn test_ipc_socket_cancel_reload_perms_and_oversized_line() {
        let temp_dir = tempfile::tempdir().unwrap();
        let nested_dir = temp_dir.path().join("private_run");
        let sock_path = nested_dir.join("test_ptt.sock");
        let mut listener = IpcSocketPttListener::new(&sock_path);
        let rx = listener.event_receiver();

        listener.start().await.unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let sock_mode = std::fs::metadata(&sock_path).unwrap().permissions().mode() & 0o777;
            assert_eq!(sock_mode, 0o600);
            let dir_mode = std::fs::metadata(&nested_dir).unwrap().permissions().mode() & 0o777;
            assert_eq!(dir_mode, 0o700);
        }

        use tokio::io::AsyncWriteExt;
        let mut stream = tokio::net::UnixStream::connect(&sock_path).await.unwrap();
        stream.write_all(b"cancel\nreload\n").await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(rx.try_recv(), Ok(PttEvent::Cancel));
        assert_eq!(rx.try_recv(), Ok(PttEvent::Reload));

        // Oversized line (> 256 bytes) must be rejected and connection closed
        let oversized = format!("{}press\n", "a".repeat(260));
        stream.write_all(oversized.as_bytes()).await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(rx.try_recv().is_err());

        listener.stop().await.unwrap();
        assert!(!sock_path.exists());
    }

    #[tokio::test]
    async fn test_ipc_socket_refuses_non_socket_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let regular_path = temp_dir.path().join("not_a_socket.txt");
        std::fs::write(&regular_path, b"keep-me").unwrap();

        let mut listener = IpcSocketPttListener::new(&regular_path);
        let res = listener.start().await;
        assert!(res.is_err());
        assert_eq!(std::fs::read(&regular_path).unwrap(), b"keep-me");
    }

    #[test]
    fn test_from_env_does_not_use_tmp_hardcoded_path() {
        let listener = IpcSocketPttListener::from_env();
        assert_ne!(listener.socket_path(), Path::new("/tmp/viernes_ptt.sock"));
    }
}
