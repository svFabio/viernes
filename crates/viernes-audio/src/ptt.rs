//! Push-to-Talk (PTT) keyboard shortcut listener and capture coordination.
//!
//! Provides deterministic keyboard press/release lifecycle management, sub-10ms capture initiation,
//! instantaneous buffer termination on key release, and precedence arbitration over active VAD.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use crossbeam_channel::{bounded, Receiver, Sender};
use tracing::{debug, info};
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
}
