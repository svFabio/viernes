//! Non-blocking streaming audio playback engine with in-memory MP3 decoding and headless CI support.

use std::fmt;
use std::io::Cursor;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use rodio::{Decoder, OutputStream, Sink};
use tokio::sync::broadcast;
use tracing::{debug, info, warn};
use viernes_core::error::ViernesError;
use viernes_core::events::SystemEvent;
use viernes_core::traits::AudioPlayback;

use crate::error::TtsError;

/// Headless in-memory audio sink for deterministic CI testing without sound devices.
#[derive(Debug, Clone)]
pub struct SilentAudioSink {
    playing: Arc<AtomicBool>,
    chunks_enqueued: Arc<AtomicUsize>,
    event_tx: Option<broadcast::Sender<SystemEvent>>,
}

impl Default for SilentAudioSink {
    fn default() -> Self {
        Self {
            playing: Arc::new(AtomicBool::new(false)),
            chunks_enqueued: Arc::new(AtomicUsize::new(0)),
            event_tx: None,
        }
    }
}

impl SilentAudioSink {
    /// Constructs a new `SilentAudioSink`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Constructs a `SilentAudioSink` with an event broadcast channel to emit `PlaybackFinished`.
    #[must_use]
    pub fn with_event_sender(event_tx: broadcast::Sender<SystemEvent>) -> Self {
        Self {
            playing: Arc::new(AtomicBool::new(false)),
            chunks_enqueued: Arc::new(AtomicUsize::new(0)),
            event_tx: Some(event_tx),
        }
    }

    /// Number of audio chunks processed.
    #[must_use]
    pub fn chunks_enqueued(&self) -> usize {
        self.chunks_enqueued.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl AudioPlayback for SilentAudioSink {
    async fn enqueue_chunk(&mut self, audio_bytes: Vec<u8>) -> Result<(), ViernesError> {
        if audio_bytes.is_empty() {
            return Err(ViernesError::Audio("Cannot play empty audio chunk".into()));
        }

        self.playing.store(true, Ordering::SeqCst);
        self.chunks_enqueued.fetch_add(1, Ordering::SeqCst);

        // In headless mode, simulate instantaneous or brief playback completion
        if let Some(ref tx) = self.event_tx {
            let _ = tx.send(SystemEvent::PlaybackFinished);
        }
        self.playing.store(false, Ordering::SeqCst);

        Ok(())
    }

    async fn enqueue_filler(&mut self, audio_bytes: Vec<u8>) -> Result<(), ViernesError> {
        if audio_bytes.is_empty() {
            return Err(ViernesError::Audio("Cannot play empty audio chunk".into()));
        }

        self.playing.store(true, Ordering::SeqCst);
        self.chunks_enqueued.fetch_add(1, Ordering::SeqCst);

        if let Some(ref tx) = self.event_tx {
            let _ = tx.send(SystemEvent::FillerPlaybackFinished);
        }
        self.playing.store(false, Ordering::SeqCst);

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

/// Commands sent to the dedicated audio thread.
enum PlayerCommand {
    PlayChunk(Vec<u8>),
    PlayFiller(Vec<u8>),
    Stop,
}

/// Real-time streaming audio player backed by `rodio` Sink and dedicated thread.
pub struct StreamingAudioPlayer {
    command_tx: crossbeam_channel::Sender<PlayerCommand>,
    playing_flag: Arc<AtomicBool>,
    _event_tx: Option<broadcast::Sender<SystemEvent>>,
}

impl fmt::Debug for StreamingAudioPlayer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StreamingAudioPlayer")
            .field("playing", &self.playing_flag.load(Ordering::SeqCst))
            .finish()
    }
}

impl StreamingAudioPlayer {
    /// Creates and initializes the streaming audio player on a dedicated OS thread.
    ///
    /// If hardware initialization fails (e.g., in headless CI), returns a descriptive `TtsError::Playback`.
    pub fn new(event_tx: Option<broadcast::Sender<SystemEvent>>) -> Result<Self, TtsError> {
        let (command_tx, command_rx) = crossbeam_channel::unbounded::<PlayerCommand>();
        let (init_tx, init_rx) = crossbeam_channel::bounded::<Result<(), String>>(1);

        let playing_flag = Arc::new(AtomicBool::new(false));
        let playing_clone = Arc::clone(&playing_flag);
        let event_tx_clone = event_tx.clone();

        std::thread::Builder::new()
            .name("viernes-audio-playback".into())
            .spawn(move || {
                // Initialize OutputStream inside dedicated playback thread to satisfy Send bounds
                let (_stream, stream_handle) = match OutputStream::try_default() {
                    Ok(pair) => pair,
                    Err(e) => {
                        let _ =
                            init_tx.send(Err(format!("No hardware audio device available: {e}")));
                        return;
                    }
                };

                let mut sink = match Sink::try_new(&stream_handle) {
                    Ok(s) => s,
                    Err(e) => {
                        let _ = init_tx.send(Err(format!("Failed to create rodio Sink: {e}")));
                        return;
                    }
                };

                // Notify parent thread that audio hardware is ready
                let _ = init_tx.send(Ok(()));
                let mut current_is_filler = false;

                loop {
                    match command_rx.recv_timeout(Duration::from_millis(50)) {
                        Ok(PlayerCommand::PlayChunk(bytes)) => {
                            let cursor = Cursor::new(bytes);
                            match Decoder::new(cursor) {
                                Ok(source) => {
                                    sink.append(source);
                                    current_is_filler = false;
                                    playing_clone.store(true, Ordering::SeqCst);
                                }
                                Err(err) => {
                                    warn!(error = %err, "Failed decoding MP3 audio stream");
                                }
                            }
                        }
                        Ok(PlayerCommand::PlayFiller(bytes)) => {
                            let cursor = Cursor::new(bytes);
                            match Decoder::new(cursor) {
                                Ok(source) => {
                                    sink.append(source);
                                    current_is_filler = true;
                                    playing_clone.store(true, Ordering::SeqCst);
                                }
                                Err(err) => {
                                    warn!(error = %err, "Failed decoding filler MP3 audio stream");
                                }
                            }
                        }
                        Ok(PlayerCommand::Stop) => {
                            debug!("Immediate stop received; recreating sink to prevent rodio freeze");
                            match Sink::try_new(&stream_handle) {
                                Ok(new_sink) => {
                                    sink.stop();
                                    sink = new_sink;
                                }
                                Err(err) => {
                                    warn!("Failed to recreate rodio Sink after stop: {err}");
                                    sink.stop();
                                }
                            }
                            current_is_filler = false;
                            playing_clone.store(false, Ordering::SeqCst);
                        }
                        Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                            if playing_clone.load(Ordering::SeqCst) && sink.empty() {
                                playing_clone.store(false, Ordering::SeqCst);
                                if current_is_filler {
                                    current_is_filler = false;
                                    debug!("Filler audio playback queue drained; emitting FillerPlaybackFinished");
                                    if let Some(ref tx) = event_tx_clone {
                                        let _ = tx.send(SystemEvent::FillerPlaybackFinished);
                                    }
                                } else {
                                    debug!("Audio playback queue drained; emitting PlaybackFinished");
                                    if let Some(ref tx) = event_tx_clone {
                                        let _ = tx.send(SystemEvent::PlaybackFinished);
                                    }
                                }
                            }
                        }
                        Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                            debug!("Audio command channel disconnected; stopping playback thread");
                            break;
                        }
                    }
                }
            })
            .map_err(|e| {
                TtsError::Playback(format!("Failed to spawn audio playback thread: {e}"))
            })?;

        match init_rx.recv_timeout(Duration::from_millis(500)) {
            Ok(Ok(())) => {
                info!("StreamingAudioPlayer hardware initialized successfully");
                Ok(Self {
                    command_tx,
                    playing_flag,
                    _event_tx: event_tx,
                })
            }
            Ok(Err(err)) => Err(TtsError::Playback(err)),
            Err(_) => Err(TtsError::Playback(
                "Audio hardware initialization timed out (headless environment)".into(),
            )),
        }
    }

    /// Creates an auto-fallback player: attempts real audio hardware, falling back to `SilentAudioSink` if unavailable.
    #[must_use]
    pub fn create_resilient_player(
        event_tx: Option<broadcast::Sender<SystemEvent>>,
    ) -> Box<dyn AudioPlayback> {
        match Self::new(event_tx.clone()) {
            Ok(player) => Box::new(player),
            Err(err) => {
                warn!("Falling back to SilentAudioSink due to: {}", err);
                if let Some(tx) = event_tx {
                    Box::new(SilentAudioSink::with_event_sender(tx))
                } else {
                    Box::new(SilentAudioSink::new())
                }
            }
        }
    }
}

#[async_trait]
impl AudioPlayback for StreamingAudioPlayer {
    async fn enqueue_chunk(&mut self, audio_bytes: Vec<u8>) -> Result<(), ViernesError> {
        if audio_bytes.is_empty() {
            return Err(ViernesError::Audio("Cannot play empty audio chunk".into()));
        }

        self.command_tx
            .send(PlayerCommand::PlayChunk(audio_bytes))
            .map_err(|e| ViernesError::Audio(format!("Failed to enqueue audio chunk: {e}")))?;

        Ok(())
    }

    async fn enqueue_filler(&mut self, audio_bytes: Vec<u8>) -> Result<(), ViernesError> {
        if audio_bytes.is_empty() {
            return Err(ViernesError::Audio("Cannot play empty audio chunk".into()));
        }

        self.command_tx
            .send(PlayerCommand::PlayFiller(audio_bytes))
            .map_err(|e| {
                ViernesError::Audio(format!("Failed to enqueue filler audio chunk: {e}"))
            })?;

        Ok(())
    }

    fn stop_immediately(&mut self) -> Result<(), ViernesError> {
        let _ = self.command_tx.send(PlayerCommand::Stop);
        self.playing_flag.store(false, Ordering::SeqCst);
        Ok(())
    }

    fn is_playing(&self) -> bool {
        self.playing_flag.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_silent_sink_enqueue_and_drain() {
        let (tx, mut rx) = broadcast::channel(16);
        let mut sink = SilentAudioSink::with_event_sender(tx);

        assert!(!sink.is_playing());
        let dummy = vec![0xFF, 0xFB, 0x90, 0x64];
        sink.enqueue_chunk(dummy).await.unwrap();

        assert_eq!(sink.chunks_enqueued(), 1);
        let event = rx.recv().await.unwrap();
        assert_eq!(event, SystemEvent::PlaybackFinished);
    }

    #[tokio::test]
    async fn test_silent_sink_enqueue_filler_emits_filler_event() {
        let (tx, mut rx) = broadcast::channel(16);
        let mut sink = SilentAudioSink::with_event_sender(tx);

        assert!(!sink.is_playing());
        let dummy = vec![0xFF, 0xFB, 0x90, 0x64];
        sink.enqueue_filler(dummy).await.unwrap();

        assert_eq!(sink.chunks_enqueued(), 1);
        let event = rx.recv().await.unwrap();
        assert_eq!(event, SystemEvent::FillerPlaybackFinished);
    }

    #[tokio::test]
    async fn test_silent_sink_stop_immediately() {
        let mut sink = SilentAudioSink::new();
        sink.stop_immediately().unwrap();
        assert!(!sink.is_playing());
    }

    #[tokio::test]
    async fn test_resilient_player_fallback_creation() {
        let (tx, _rx) = broadcast::channel(8);
        let mut player = StreamingAudioPlayer::create_resilient_player(Some(tx));
        let dummy = vec![0xFF, 0xFB, 0x90, 0x64];
        let result = player.enqueue_chunk(dummy).await;
        assert!(result.is_ok());
    }
}
