//! Push-to-Talk (PTT) keyboard shortcut listener and capture coordination.
//!
//! Provides deterministic keyboard press/release lifecycle management, sub-10ms capture initiation,
//! instantaneous buffer termination on key release, and precedence arbitration over active VAD.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use crossbeam_channel::{bounded, Receiver, Sender};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::UnixListener;
use tracing::{debug, info, warn};
use viernes_core::error::ViernesError;

/// Push-to-talk discrete key events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PttEvent {
    /// Hotkey depressed (commence capture immediately).
    Pressed,
    /// Hotkey released (cut capture immediately and emit buffer).
    Released,
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

/// Real-time Unix domain socket PTT listener for Wayland / Hyprland global shortcuts.
pub struct IpcSocketPttListener {
    socket_path: PathBuf,
    sender: Sender<PttEvent>,
    receiver: Receiver<PttEvent>,
    active: Arc<AtomicBool>,
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
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
        }
    }

    /// Constructs a default Unix Domain Socket PTT listener at `/tmp/viernes_ptt.sock`.
    #[must_use]
    pub fn from_env() -> Self {
        let path = std::env::var("VIERNES_PTT_SOCKET")
            .unwrap_or_else(|_| "/tmp/viernes_ptt.sock".to_string());
        Self::new(path)
    }
}

#[async_trait]
impl PttListener for IpcSocketPttListener {
    async fn start(&mut self) -> Result<(), ViernesError> {
        if self.active.load(Ordering::SeqCst) {
            return Ok(());
        }

        // Clean up stale socket file if it exists
        if self.socket_path.exists() {
            let _ = std::fs::remove_file(&self.socket_path);
        }

        let listener = UnixListener::bind(&self.socket_path).map_err(|e| {
            ViernesError::Audio(format!(
                "Failed to bind PTT IPC socket at {:?}: {e}",
                self.socket_path
            ))
        })?;

        info!("PTT IPC server listening on {:?}", self.socket_path);
        self.active.store(true, Ordering::SeqCst);

        let active = self.active.clone();
        let sender = self.sender.clone();
        let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel();
        self.shutdown_tx = Some(shutdown_tx);

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => {
                        debug!("PTT IPC server received shutdown signal");
                        break;
                    }
                    accept_res = listener.accept() => {
                        match accept_res {
                            Ok((stream, _)) => {
                                let s = sender.clone();
                                tokio::spawn(async move {
                                    let mut reader = BufReader::new(stream);
                                    let mut line = String::new();
                                    while let Ok(n) = reader.read_line(&mut line).await {
                                        if n == 0 {
                                            break;
                                        }
                                        let cmd = line.trim().to_lowercase();
                                        line.clear();
                                        match cmd.as_str() {
                                            "press" | "pressed" | "start" => {
                                                let _ = s.send(PttEvent::Pressed);
                                            }
                                            "release" | "released" | "finish" | "stop" => {
                                                let _ = s.send(PttEvent::Released);
                                            }
                                            _ => {}
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

        Ok(())
    }

    async fn stop(&mut self) -> Result<(), ViernesError> {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        if self.socket_path.exists() {
            let _ = std::fs::remove_file(&self.socket_path);
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
}
