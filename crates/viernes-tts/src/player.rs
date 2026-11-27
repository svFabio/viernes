//! Non-blocking streaming audio playback engine with in-memory MP3 decoding and headless CI support.

use std::collections::VecDeque;
use std::fmt;
use std::io::Cursor;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use rodio::cpal::{
    self,
    traits::{DeviceTrait, HostTrait},
};
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, StreamError};
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
    SetVolume(f32),
    Stop,
    Shutdown,
}

/// Tracks queued item types (`true` = filler, `false` = response chunk) in FIFO order.
#[derive(Debug, Default)]
struct PlaybackQueueTracker {
    queued_is_filler: VecDeque<bool>,
}

impl PlaybackQueueTracker {
    fn push_chunk(&mut self) {
        self.queued_is_filler.push_back(false);
    }

    fn push_filler(&mut self) {
        self.queued_is_filler.push_back(true);
    }

    fn clear(&mut self) {
        self.queued_is_filler.clear();
    }

    /// Reconciles tracked entries against the sink's current queue length and returns
    /// the type of the most recently finished item (`true` = filler), if any finished.
    fn reconcile(&mut self, sink_len: usize) -> Option<bool> {
        let mut last_finished = None;
        while self.queued_is_filler.len() > sink_len {
            if let Some(is_filler) = self.queued_is_filler.pop_front() {
                last_finished = Some(is_filler);
            }
        }
        last_finished
    }
}

/// Resolves an output device index from device names using numeric index, exact name, or partial match.
fn find_matching_device_index(names: &[String], query: &str) -> Option<usize> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Ok(idx) = trimmed.parse::<usize>() {
        if idx < names.len() {
            return Some(idx);
        }
        warn!(
            device_index = idx,
            available = names.len(),
            "Requested output device index out of range"
        );
    }

    let query_lower = trimmed.to_lowercase();
    if let Some(idx) = names.iter().position(|n| n.to_lowercase() == query_lower) {
        return Some(idx);
    }

    names
        .iter()
        .position(|n| n.to_lowercase().contains(&query_lower))
}

/// Opens a `rodio::OutputStream` for the requested output device, falling back to default on failure.
fn open_output_stream(
    output_device: Option<&str>,
) -> Result<(OutputStream, OutputStreamHandle), StreamError> {
    let query = output_device.map(str::trim).filter(|s| !s.is_empty());

    if let Some(query) = query {
        let host = cpal::default_host();
        match host.output_devices() {
            Ok(devices) => {
                let devices: Vec<cpal::Device> = devices.collect();
                let names: Vec<String> = devices
                    .iter()
                    .enumerate()
                    .map(|(i, d)| d.name().unwrap_or_else(|_| format!("Output Device #{i}")))
                    .collect();

                if let Some(idx) = find_matching_device_index(&names, query) {
                    match OutputStream::try_from_device(&devices[idx]) {
                        Ok(stream) => {
                            info!(
                                device = %names[idx],
                                index = idx,
                                "Opened selected audio output device"
                            );
                            return Ok(stream);
                        }
                        Err(err) => {
                            warn!(
                                device = %names[idx],
                                error = %err,
                                "Failed to open selected audio output device; falling back to default"
                            );
                        }
                    }
                } else {
                    warn!(
                        query = %query,
                        available = names.len(),
                        "Requested audio output device not found; falling back to default"
                    );
                }
            }
            Err(err) => {
                warn!(
                    error = %err,
                    "Failed to enumerate audio output devices; falling back to default"
                );
            }
        }
    }

    OutputStream::try_default()
}

/// Real-time streaming audio player backed by `rodio` Sink and dedicated thread.
pub struct StreamingAudioPlayer {
    command_tx: crossbeam_channel::Sender<PlayerCommand>,
    playing_flag: Arc<AtomicBool>,
    worker_handle: Option<std::thread::JoinHandle<()>>,
    _event_tx: Option<broadcast::Sender<SystemEvent>>,
}

impl fmt::Debug for StreamingAudioPlayer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StreamingAudioPlayer")
            .field("playing", &self.playing_flag.load(Ordering::SeqCst))
            .finish()
    }
}

impl Drop for StreamingAudioPlayer {
    fn drop(&mut self) {
        if let Err(err) = self.command_tx.send(PlayerCommand::Shutdown) {
            debug!(error = %err, "Audio playback command channel already closed on drop");
        }
        if let Some(handle) = self.worker_handle.take() {
            if handle.thread().id() != std::thread::current().id() {
                if let Err(err) = handle.join() {
                    warn!(
                        "Audio playback worker thread panicked on shutdown: {:?}",
                        err
                    );
                }
            }
        }
    }
}

impl StreamingAudioPlayer {
    /// Creates and initializes the streaming audio player on a dedicated OS thread with default 1.0 volume.
    pub fn new(event_tx: Option<broadcast::Sender<SystemEvent>>) -> Result<Self, TtsError> {
        Self::new_with_options(event_tx, 1.0, None)
    }

    /// Creates and initializes the streaming audio player with a specified volume multiplier.
    pub fn new_with_volume(
        event_tx: Option<broadcast::Sender<SystemEvent>>,
        initial_volume: f32,
    ) -> Result<Self, TtsError> {
        Self::new_with_options(event_tx, initial_volume, None)
    }

    /// Creates and initializes the streaming audio player with volume and optional output device selection.
    pub fn new_with_options(
        event_tx: Option<broadcast::Sender<SystemEvent>>,
        initial_volume: f32,
        output_device: Option<&str>,
    ) -> Result<Self, TtsError> {
        let (command_tx, command_rx) = crossbeam_channel::unbounded::<PlayerCommand>();
        let (init_tx, init_rx) = crossbeam_channel::bounded::<Result<(), String>>(1);

        let playing_flag = Arc::new(AtomicBool::new(false));
        let playing_clone = Arc::clone(&playing_flag);
        let event_tx_clone = event_tx.clone();
        let requested_device = output_device
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let worker_handle = std::thread::Builder::new()
            .name("viernes-audio-playback".into())
            .spawn(move || {
                // Initialize OutputStream inside dedicated playback thread to satisfy Send bounds
                let (_stream, stream_handle) =
                    match open_output_stream(requested_device.as_deref()) {
                        Ok(pair) => pair,
                        Err(e) => {
                            let _ = init_tx
                                .send(Err(format!("No hardware audio device available: {e}")));
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

                let mut current_volume: f32 = initial_volume.max(0.0);
                sink.set_volume(current_volume);

                // Notify parent thread that audio hardware is ready
                let _ = init_tx.send(Ok(()));
                let mut tracker = PlaybackQueueTracker::default();

                loop {
                    match command_rx.recv_timeout(Duration::from_millis(50)) {
                        Ok(PlayerCommand::PlayChunk(bytes)) => {
                            tracker.reconcile(sink.len());
                            let cursor = Cursor::new(bytes);
                            match Decoder::new(cursor) {
                                Ok(source) => {
                                    sink.append(source);
                                    tracker.push_chunk();
                                    playing_clone.store(true, Ordering::SeqCst);
                                }
                                Err(err) => {
                                    warn!(error = %err, "Failed decoding MP3 audio stream");
                                    if sink.empty() && command_rx.is_empty() {
                                        playing_clone.store(false, Ordering::SeqCst);
                                    }
                                }
                            }
                        }
                        Ok(PlayerCommand::PlayFiller(bytes)) => {
                            tracker.reconcile(sink.len());
                            let cursor = Cursor::new(bytes);
                            match Decoder::new(cursor) {
                                Ok(source) => {
                                    sink.append(source);
                                    tracker.push_filler();
                                    playing_clone.store(true, Ordering::SeqCst);
                                }
                                Err(err) => {
                                    warn!(error = %err, "Failed decoding filler MP3 audio stream");
                                    if sink.empty() && command_rx.is_empty() {
                                        playing_clone.store(false, Ordering::SeqCst);
                                    }
                                }
                            }
                        }
                        Ok(PlayerCommand::SetVolume(vol)) => {
                            current_volume = vol.max(0.0);
                            sink.set_volume(current_volume);
                            debug!(volume = current_volume, "Updated audio playback volume factor");
                        }
                        Ok(PlayerCommand::Stop) => {
                            debug!("Immediate stop received; recreating sink to prevent rodio freeze");
                            match Sink::try_new(&stream_handle) {
                                Ok(new_sink) => {
                                    sink.stop();
                                    new_sink.set_volume(current_volume);
                                    sink = new_sink;
                                }
                                Err(err) => {
                                    warn!("Failed to recreate rodio Sink after stop: {err}");
                                    sink.stop();
                                }
                            }
                            tracker.clear();
                            playing_clone.store(false, Ordering::SeqCst);
                        }
                        Ok(PlayerCommand::Shutdown) => {
                            debug!("Audio shutdown command received; stopping playback thread");
                            sink.stop();
                            tracker.clear();
                            playing_clone.store(false, Ordering::SeqCst);
                            break;
                        }
                        Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                            let finished = tracker.reconcile(sink.len());
                            if playing_clone.load(Ordering::SeqCst)
                                && sink.empty()
                                && command_rx.is_empty()
                            {
                                playing_clone.store(false, Ordering::SeqCst);
                                if finished.unwrap_or(false) {
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
                    worker_handle: Some(worker_handle),
                    _event_tx: event_tx,
                })
            }
            Ok(Err(err)) => {
                let _ = worker_handle.join();
                Err(TtsError::Playback(err))
            }
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
        Self::create_resilient_player_with_options(event_tx, 1.0, None)
    }

    /// Creates an auto-fallback player with a specific volume multiplier.
    #[must_use]
    pub fn create_resilient_player_with_volume(
        event_tx: Option<broadcast::Sender<SystemEvent>>,
        initial_volume: f32,
    ) -> Box<dyn AudioPlayback> {
        Self::create_resilient_player_with_options(event_tx, initial_volume, None)
    }

    /// Creates an auto-fallback player with a specific volume multiplier and optional output device.
    #[must_use]
    pub fn create_resilient_player_with_options(
        event_tx: Option<broadcast::Sender<SystemEvent>>,
        initial_volume: f32,
        output_device: Option<&str>,
    ) -> Box<dyn AudioPlayback> {
        match Self::new_with_options(event_tx.clone(), initial_volume, output_device) {
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
    fn set_volume(&mut self, volume: f32) -> Result<(), ViernesError> {
        self.command_tx
            .send(PlayerCommand::SetVolume(volume))
            .map_err(|e| ViernesError::Audio(format!("Failed to set playback volume: {e}")))?;
        Ok(())
    }

    async fn enqueue_chunk(&mut self, audio_bytes: Vec<u8>) -> Result<(), ViernesError> {
        if audio_bytes.is_empty() {
            return Err(ViernesError::Audio("Cannot play empty audio chunk".into()));
        }

        self.playing_flag.store(true, Ordering::SeqCst);
        self.command_tx
            .send(PlayerCommand::PlayChunk(audio_bytes))
            .map_err(|e| ViernesError::Audio(format!("Failed to enqueue audio chunk: {e}")))?;

        Ok(())
    }

    async fn enqueue_filler(&mut self, audio_bytes: Vec<u8>) -> Result<(), ViernesError> {
        if audio_bytes.is_empty() {
            return Err(ViernesError::Audio("Cannot play empty audio chunk".into()));
        }

        self.playing_flag.store(true, Ordering::SeqCst);
        self.command_tx
            .send(PlayerCommand::PlayFiller(audio_bytes))
            .map_err(|e| {
                ViernesError::Audio(format!("Failed to enqueue filler audio chunk: {e}"))
            })?;

        Ok(())
    }

    fn stop_immediately(&mut self) -> Result<(), ViernesError> {
        if let Err(err) = self.command_tx.send(PlayerCommand::Stop) {
            warn!(error = %err, "Failed to send Stop command to audio playback thread");
        }
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

    #[tokio::test]
    async fn test_resilient_player_with_options_fallback() {
        let (tx, _rx) = broadcast::channel(8);
        let mut player = StreamingAudioPlayer::create_resilient_player_with_options(
            Some(tx),
            0.75,
            Some("nonexistent-device-xyz"),
        );
        let dummy = vec![0xFF, 0xFB, 0x90, 0x64];
        assert!(player.enqueue_chunk(dummy).await.is_ok());
    }

    #[test]
    fn test_find_matching_device_index() {
        let devices = vec![
            "Built-in Audio Analog Stereo".to_string(),
            "USB Headset Pro".to_string(),
            "HDMI Output 0".to_string(),
        ];

        assert_eq!(find_matching_device_index(&devices, ""), None);
        assert_eq!(find_matching_device_index(&devices, "   "), None);
        assert_eq!(find_matching_device_index(&devices, "1"), Some(1));
        assert_eq!(
            find_matching_device_index(&devices, "usb headset pro"),
            Some(1)
        );
        assert_eq!(find_matching_device_index(&devices, "headset"), Some(1));
        assert_eq!(find_matching_device_index(&devices, "hdmi"), Some(2));
        assert_eq!(find_matching_device_index(&devices, "99"), None);
        assert_eq!(find_matching_device_index(&devices, "bluetooth"), None);
    }

    #[test]
    fn test_playback_queue_tracker_fifo_filler_and_chunk() {
        let mut tracker = PlaybackQueueTracker::default();
        tracker.push_filler();
        tracker.push_chunk();
        tracker.push_chunk();

        // Filler finishes while 2 chunks remain
        assert_eq!(tracker.reconcile(2), Some(true));
        // First chunk finishes while 1 chunk remains
        assert_eq!(tracker.reconcile(1), Some(false));
        // Last chunk finishes; queue drains
        assert_eq!(tracker.reconcile(0), Some(false));
        // No more items
        assert_eq!(tracker.reconcile(0), None);
    }

    #[test]
    fn test_player_drop_and_stop_with_closed_channel() {
        let (tx, rx) = crossbeam_channel::unbounded();
        drop(rx);
        let mut player = StreamingAudioPlayer {
            command_tx: tx,
            playing_flag: Arc::new(AtomicBool::new(true)),
            worker_handle: None,
            _event_tx: None,
        };

        assert!(player.stop_immediately().is_ok());
        assert!(!player.is_playing());
        drop(player);
    }
}
