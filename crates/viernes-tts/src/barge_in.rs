//! Atomic Barge-in controller for instantaneous speech interruption and cancellation.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};
use viernes_core::error::ViernesError;
use viernes_core::events::SystemEvent;
use viernes_core::traits::AudioPlayback;

/// Manages atomic interrupt signals, cancellation tokens, and acoustic echo protection windows.
pub struct BargeInController {
    interrupted: Arc<AtomicBool>,
    cancellation_token: Arc<RwLock<CancellationToken>>,
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
            cancellation_token: Arc::new(RwLock::new(CancellationToken::new())),
            echo_grace_window,
            playback_started_at: Arc::new(tokio::sync::Mutex::new(None)),
        }
    }

    /// Obtains a child cancellation token for binding to active in-flight network TTS requests.
    #[must_use]
    pub fn child_cancellation_token(&self) -> CancellationToken {
        self.cancellation_token
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .child_token()
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
        self.cancellation_token
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .cancel();

        // 3. Immediately halt audio hardware output
        player.stop_immediately()?;

        let latency = start_time.elapsed();
        info!(
            latency_micros = latency.as_micros(),
            "Barge-in executed; playback halted and queue discarded"
        );

        Ok(latency)
    }

    /// Subscribes to the central event bus to automatically trigger barge-in on `VoiceStarted` or `BargeIn`.
    pub fn spawn_event_listener(
        self: Arc<Self>,
        mut event_rx: broadcast::Receiver<SystemEvent>,
        mut player: Box<dyn AudioPlayback>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                match event_rx.recv().await {
                    Ok(event) => {
                        if matches!(event, SystemEvent::VoiceStarted | SystemEvent::BargeIn)
                            && player.is_playing()
                        {
                            if let Err(err) = self.trigger_barge_in(player.as_mut()).await {
                                warn!(error = %err, "Failed to execute barge-in from event listener");
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        warn!(
                            skipped = skipped,
                            "Barge-in event listener lagged behind broadcast channel; continuing"
                        );
                        continue;
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        debug!("Barge-in event channel closed; stopping listener");
                        break;
                    }
                }
            }
        })
    }

    /// Resets the controller state for the next dialogue turn.
    pub async fn reset(&self) {
        self.interrupted.store(false, Ordering::SeqCst);
        {
            let mut guard = self
                .cancellation_token
                .write()
                .unwrap_or_else(|e| e.into_inner());
            *guard = CancellationToken::new();
        }
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
    use async_trait::async_trait;

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

    #[tokio::test]
    async fn test_reset_behind_arc_issues_fresh_cancellation_token() {
        let controller = Arc::new(BargeInController::new(Duration::from_millis(0)));
        let mut sink = SilentAudioSink::new();
        let old_token = controller.child_cancellation_token();

        controller.trigger_barge_in(&mut sink).await.unwrap();
        assert!(old_token.is_cancelled());
        assert!(controller.is_interrupted());

        controller.reset().await;
        assert!(!controller.is_interrupted());
        let new_token = controller.child_cancellation_token();
        assert!(!new_token.is_cancelled());
        assert!(old_token.is_cancelled());
    }

    #[derive(Debug)]
    struct StubPlayingSink {
        playing: Arc<AtomicBool>,
    }

    #[async_trait]
    impl AudioPlayback for StubPlayingSink {
        async fn enqueue_chunk(&mut self, _audio_bytes: Vec<u8>) -> Result<(), ViernesError> {
            self.playing.store(true, Ordering::SeqCst);
            Ok(())
        }

        async fn enqueue_filler(&mut self, _audio_bytes: Vec<u8>) -> Result<(), ViernesError> {
            self.playing.store(true, Ordering::SeqCst);
            Ok(())
        }

        fn stop_immediately(&mut self) -> Result<(), ViernesError> {
            self.playing.store(false, Ordering::SeqCst);
            Ok(())
        }

        fn is_playing(&self) -> bool {
            self.playing.load(Ordering::SeqCst)
        }
    }

    #[tokio::test]
    async fn test_spawn_event_listener_continues_after_lagged_recv() {
        let controller = Arc::new(BargeInController::new(Duration::from_millis(0)));
        let (tx, rx) = broadcast::channel(2);
        let playing = Arc::new(AtomicBool::new(true));
        let player = Box::new(StubPlayingSink {
            playing: Arc::clone(&playing),
        });

        let handle = Arc::clone(&controller).spawn_event_listener(rx, player);

        // Overflow capacity=2 channel before listener polls so first recv yields RecvError::Lagged
        let _ = tx.send(SystemEvent::PlaybackFinished);
        let _ = tx.send(SystemEvent::PlaybackFinished);
        let _ = tx.send(SystemEvent::PlaybackFinished);
        let _ = tx.send(SystemEvent::VoiceStarted);

        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(controller.is_interrupted());
        assert!(!playing.load(Ordering::SeqCst));

        drop(tx);
        handle.await.unwrap();
    }
}
