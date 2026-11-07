//! Atomic Barge-in controller for instantaneous speech interruption and cancellation.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};
use viernes_core::error::ViernesError;
use viernes_core::events::SystemEvent;
use viernes_core::traits::AudioPlayback;

/// Manages atomic interrupt signals, cancellation tokens, and acoustic echo protection windows.
pub struct BargeInController {
    interrupted: Arc<AtomicBool>,
    cancellation_token: CancellationToken,
    echo_grace_window: Duration,
    playback_started_at: Arc<tokio::sync::Mutex<Option<Instant>>>,
}

impl Default for BargeInController {
    fn default() -> Self {
        Self::new(Duration::from_millis(150))
    }
}

impl BargeInController {
    /// Creates a new `BargeInController` with a specified acoustic echo protection window.
    #[must_use]
    pub fn new(echo_grace_window: Duration) -> Self {
        Self {
            interrupted: Arc::new(AtomicBool::new(false)),
            cancellation_token: CancellationToken::new(),
            echo_grace_window,
            playback_started_at: Arc::new(tokio::sync::Mutex::new(None)),
        }
    }

    /// Obtains a child cancellation token for binding to active in-flight network TTS requests.
    #[must_use]
    pub fn child_cancellation_token(&self) -> CancellationToken {
        self.cancellation_token.child_token()
    }

    /// Records the timestamp when playback has started to arm the acoustic echo grace window.
    pub async fn notify_playback_started(&self) {
        let mut lock = self.playback_started_at.lock().await;
        *lock = Some(Instant::now());
        self.interrupted.store(false, Ordering::SeqCst);
    }

    /// Triggers atomic interruption (Barge-in): halts sink and cancels in-flight synthesis in < 80ms.
    pub async fn trigger_barge_in(
        &self,
        player: &mut dyn AudioPlayback,
    ) -> Result<Duration, ViernesError> {
        let start_time = Instant::now();

        // Check acoustic echo window if playback recently started
        {
            let lock = self.playback_started_at.lock().await;
            if let Some(started) = *lock {
                let elapsed = started.elapsed();
                if elapsed < self.echo_grace_window {
                    debug!(
                        elapsed_ms = elapsed.as_millis(),
                        grace_ms = self.echo_grace_window.as_millis(),
                        "Barge-in suppressed during acoustic echo grace window"
                    );
                    return Ok(elapsed);
                }
            }
        }

        // 1. Mark atomic flag
        self.interrupted.store(true, Ordering::SeqCst);

        // 2. Cancel in-flight HTTP requests
        self.cancellation_token.cancel();

        // 3. Immediately halt audio hardware output
        player.stop_immediately()?;

        let latency = start_time.elapsed();
        info!(
            latency_micros = latency.as_micros(),
            "Barge-in executed; playback halted and queue discarded"
        );

        Ok(latency)
    }

    /// Subscribes to the central event bus to automatically trigger barge-in on `VoiceStarted`.
    pub fn spawn_event_listener(
        self: Arc<Self>,
        mut event_rx: broadcast::Receiver<SystemEvent>,
        mut player: Box<dyn AudioPlayback>,
    ) {
        tokio::spawn(async move {
            while let Ok(event) = event_rx.recv().await {
                if matches!(event, SystemEvent::VoiceStarted | SystemEvent::BargeIn)
                    && player.is_playing()
                {
                    let _ = self.trigger_barge_in(player.as_mut()).await;
                }
            }
        });
    }

    /// Resets the controller state for the next dialogue turn.
    pub async fn reset(&mut self) {
        self.interrupted.store(false, Ordering::SeqCst);
        self.cancellation_token = CancellationToken::new();
        let mut lock = self.playback_started_at.lock().await;
        *lock = None;
    }

    /// Returns whether the current playback sequence has been interrupted.
    #[must_use]
    pub fn is_interrupted(&self) -> bool {
        self.interrupted.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::SilentAudioSink;

    #[tokio::test]
    async fn test_barge_in_latency_under_80ms() {
        let mut sink = SilentAudioSink::new();
        sink.enqueue_chunk(vec![0xFF, 0xFB, 0x90, 0x64])
            .await
            .unwrap();

        let controller = BargeInController::new(Duration::from_millis(0));
        let token = controller.child_cancellation_token();

        let latency = controller.trigger_barge_in(&mut sink).await.unwrap();

        assert!(
            latency < Duration::from_millis(80),
            "Barge-in latency was {:?}, expected < 80ms",
            latency
        );
        assert!(token.is_cancelled());
        assert!(!sink.is_playing());
        assert!(controller.is_interrupted());
    }

    #[tokio::test]
    async fn test_acoustic_echo_grace_window() {
        let mut sink = SilentAudioSink::new();
        let controller = BargeInController::new(Duration::from_millis(200));

        controller.notify_playback_started().await;

        // Immediate interrupt during grace window is suppressed
        let elapsed = controller.trigger_barge_in(&mut sink).await.unwrap();
        assert!(elapsed < Duration::from_millis(200));
        assert!(!controller.is_interrupted());

        // Wait past grace window
        tokio::time::sleep(Duration::from_millis(210)).await;
        let _ = controller.trigger_barge_in(&mut sink).await.unwrap();
        assert!(controller.is_interrupted());
    }
}
