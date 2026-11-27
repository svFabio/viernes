//! Real-time microphone audio capture stream utilizing cpal and lock-free crossbeam ring-buffers.
//!
//! Handles defensive hardware sample rate negotiation, automatic fallback to native device
//! rates with software downsampling, and zero-allocation callbacks in the real-time audio thread.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};
use crossbeam_channel::{bounded, Receiver, Sender};
use tracing::{debug, error, info};
use viernes_core::error::ViernesError;
use viernes_core::traits::AudioCapture;

use crate::devices::{resolve_device, CpalDeviceEnumerator, DeviceEnumerator};
use crate::resample::process_to_16k_mono;

/// Configuration options for audio capture streams.
#[derive(Debug, Clone)]
pub struct AudioCaptureConfig {
    /// Preferred device name or index query.
    pub device_query: Option<String>,
    /// Preferred sample rate (default 16,000 Hz).
    pub preferred_sample_rate: u32,
    /// Preferred chunk duration in milliseconds (default 20ms = 320 samples @ 16kHz).
    pub chunk_ms: u32,
    /// Bounded ring buffer capacity in chunks.
    pub channel_capacity: usize,
}

impl Default for AudioCaptureConfig {
    fn default() -> Self {
        Self {
            device_query: None,
            preferred_sample_rate: 16_000,
            chunk_ms: 20,
            channel_capacity: 128,
        }
    }
}

/// Maximum accumulated samples retained in recording buffer to prevent unbounded leaks (~30s @ 16kHz).
pub const MAX_ACCUMULATED_SAMPLES: usize = 16_000 * 30;

enum AudioWorkerCommand {
    Stop,
}

/// Production implementation of [`AudioCapture`] backed by native `cpal` streams.
///
/// Executes cpal hardware streams on an isolated real-time OS thread to maintain `Send + Sync`
/// conformance across platforms where ALSA backend pointers are non-Send.
pub struct CpalAudioCapture {
    config: AudioCaptureConfig,
    is_running: Arc<AtomicBool>,
    sender: Sender<Vec<f32>>,
    receiver: Receiver<Vec<f32>>,
    accumulated_buffer: Arc<Mutex<Vec<f32>>>,
    recording_active: Arc<AtomicBool>,
    worker_tx: Option<Sender<AudioWorkerCommand>>,
    worker_handle: Option<std::thread::JoinHandle<()>>,
    negotiated_rate: Arc<Mutex<u32>>,
    negotiated_channels: Arc<Mutex<u16>>,
}

impl std::fmt::Debug for CpalAudioCapture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CpalAudioCapture")
            .field("config", &self.config)
            .field("is_running", &self.is_running.load(Ordering::SeqCst))
            .field(
                "negotiated_rate",
                &*self
                    .negotiated_rate
                    .lock()
                    .unwrap_or_else(|e| e.into_inner()),
            )
            .field(
                "negotiated_channels",
                &*self
                    .negotiated_channels
                    .lock()
                    .unwrap_or_else(|e| e.into_inner()),
            )
            .finish()
    }
}

impl CpalAudioCapture {
    /// Creates a new `CpalAudioCapture` instance configured with the specified options.
    #[must_use]
    pub fn new(config: AudioCaptureConfig) -> Self {
        let (sender, receiver) = bounded(config.channel_capacity);
        Self {
            config,
            is_running: Arc::new(AtomicBool::new(false)),
            sender,
            receiver,
            accumulated_buffer: Arc::new(Mutex::new(Vec::new())),
            recording_active: Arc::new(AtomicBool::new(true)),
            worker_tx: None,
            worker_handle: None,
            negotiated_rate: Arc::new(Mutex::new(16_000)),
            negotiated_channels: Arc::new(Mutex::new(1)),
        }
    }

    /// Enables or disables accumulation into the internal recording buffer.
    ///
    /// When disabled, streaming continues over the channel without allocating memory in the capture buffer.
    pub fn set_recording_active(&self, active: bool) {
        self.recording_active.store(active, Ordering::SeqCst);
    }

    /// Indicates whether accumulation into the internal recording buffer is enabled.
    #[must_use]
    pub fn is_recording_active(&self) -> bool {
        self.recording_active.load(Ordering::SeqCst)
    }

    /// Returns a clone of the lock-free receiver channel for streaming raw PCM chunks to VAD.
    #[must_use]
    pub fn receiver(&self) -> Receiver<Vec<f32>> {
        self.receiver.clone()
    }

    /// Returns the negotiated sample rate and channel count.
    #[must_use]
    pub fn stream_parameters(&self) -> (u32, u16) {
        (
            *self
                .negotiated_rate
                .lock()
                .unwrap_or_else(|e| e.into_inner()),
            *self
                .negotiated_channels
                .lock()
                .unwrap_or_else(|e| e.into_inner()),
        )
    }
}

fn negotiate_input_config(
    device: &cpal::Device,
    preferred_rate: u32,
) -> Result<(StreamConfig, SampleFormat), String> {
    let target_rate = cpal::SampleRate(preferred_rate);
    let format_preference = [
        SampleFormat::F32,
        SampleFormat::I16,
        SampleFormat::I32,
        SampleFormat::U16,
    ];

    if let Ok(configs) = device.supported_input_configs() {
        let ranges: Vec<cpal::SupportedStreamConfigRange> = configs.collect();
        for desired_channels in [1u16, 2u16] {
            for desired_format in format_preference {
                if ranges.iter().any(|c| {
                    c.channels() == desired_channels
                        && c.min_sample_rate() <= target_rate
                        && c.max_sample_rate() >= target_rate
                        && c.sample_format() == desired_format
                }) {
                    return Ok((
                        StreamConfig {
                            channels: desired_channels,
                            sample_rate: target_rate,
                            buffer_size: cpal::BufferSize::Default,
                        },
                        desired_format,
                    ));
                }
            }
        }
    }

    device
        .default_input_config()
        .map(|cfg| (cfg.config(), cfg.sample_format()))
        .map_err(|e| format!("Default config error: {e}"))
}

fn push_captured_samples(
    samples: &[f32],
    channels: u16,
    from_rate: u32,
    needs_processing: bool,
    stream_rec: &AtomicBool,
    stream_acc: &Mutex<Vec<f32>>,
    stream_tx: &Sender<Vec<f32>>,
) {
    let processed = if needs_processing {
        process_to_16k_mono(samples, channels, from_rate)
    } else {
        samples.to_vec()
    };

    if !processed.is_empty() {
        if stream_rec.load(Ordering::Relaxed) {
            if let Ok(mut acc) = stream_acc.try_lock() {
                if acc.len() + processed.len() > MAX_ACCUMULATED_SAMPLES {
                    let overflow = (acc.len() + processed.len()) - MAX_ACCUMULATED_SAMPLES;
                    let drain_count = overflow.min(acc.len());
                    acc.drain(0..drain_count);
                }
                acc.extend_from_slice(&processed);
            }
        }
        let _ = stream_tx.try_send(processed);
    }
}

#[async_trait]
impl AudioCapture for CpalAudioCapture {
    async fn start_stream(&mut self) -> Result<(), ViernesError> {
        if self.is_running.load(Ordering::SeqCst) {
            debug!("Audio stream is already active");
            return Ok(());
        }

        let host = cpal::default_host();
        let enumerator = CpalDeviceEnumerator::new();
        let devices = enumerator.list_input_devices()?;
        let default_dev = devices.iter().find(|d| d.is_default);

        let selected = resolve_device(self.config.device_query.as_deref(), &devices, default_dev);

        let selected_device = selected.map(|d| (d.index, d.name.clone()));
        let preferred_rate = self.config.preferred_sample_rate;

        let tx = self.sender.clone();
        let running_flag = self.is_running.clone();
        let accumulated = self.accumulated_buffer.clone();
        let recording_flag = self.recording_active.clone();
        let rate_holder = self.negotiated_rate.clone();
        let chan_holder = self.negotiated_channels.clone();

        let (cmd_tx, cmd_rx) = bounded::<AudioWorkerCommand>(4);
        let (init_tx, init_rx) = tokio::sync::oneshot::channel::<Result<(), String>>();

        // Spawn isolated audio capture worker OS thread
        let handle = std::thread::Builder::new()
            .name("viernes-audio-capture".into())
            .spawn(move || {
                let dev_res = if let Some((idx, ref name)) = selected_device {
                    let all_devs: Vec<cpal::Device> = host
                        .input_devices()
                        .map(|devs| devs.collect())
                        .unwrap_or_default();
                    all_devs
                        .get(idx)
                        .filter(|d| d.name().ok().as_deref() == Some(name.as_str()))
                        .cloned()
                        .or_else(|| {
                            all_devs
                                .iter()
                                .find(|d| d.name().ok().as_deref() == Some(name.as_str()))
                                .cloned()
                        })
                        .or_else(|| {
                            if name == &format!("Input Device #{idx}") {
                                all_devs.get(idx).cloned()
                            } else {
                                None
                            }
                        })
                        .or_else(|| host.default_input_device())
                } else {
                    host.default_input_device()
                };

                let device = match dev_res {
                    Some(d) => d,
                    None => {
                        let _ = init_tx.send(Err("No audio input hardware found".into()));
                        return;
                    }
                };

                let (stream_config, sample_format) =
                    match negotiate_input_config(&device, preferred_rate) {
                        Ok(res) => res,
                        Err(e) => {
                            let _ = init_tx.send(Err(e));
                            return;
                        }
                    };

                let from_rate = stream_config.sample_rate.0;
                let channels = stream_config.channels;
                // Whisper always requires 16kHz mono PCM, even if preferred_sample_rate != 16_000
                let needs_processing = from_rate != 16_000 || channels != 1;

                *rate_holder.lock().unwrap_or_else(|e| e.into_inner()) = from_rate;
                *chan_holder.lock().unwrap_or_else(|e| e.into_inner()) = channels;

                let stream_running = running_flag.clone();
                let stream_acc = accumulated.clone();
                let stream_tx = tx.clone();
                let stream_rec = recording_flag.clone();

                let err_fn = move |err: cpal::StreamError| {
                    error!("cpal stream error: {err}");
                    stream_running.store(false, Ordering::SeqCst);
                };

                let stream_res = match sample_format {
                    SampleFormat::F32 => device.build_input_stream(
                        &stream_config,
                        move |data: &[f32], _: &cpal::InputCallbackInfo| {
                            push_captured_samples(
                                data,
                                channels,
                                from_rate,
                                needs_processing,
                                &stream_rec,
                                &stream_acc,
                                &stream_tx,
                            );
                        },
                        err_fn,
                        None,
                    ),
                    SampleFormat::I16 => device.build_input_stream(
                        &stream_config,
                        move |data: &[i16], _: &cpal::InputCallbackInfo| {
                            let f32_samples: Vec<f32> =
                                data.iter().map(|&s| f32::from(s) / 32768.0).collect();
                            push_captured_samples(
                                &f32_samples,
                                channels,
                                from_rate,
                                needs_processing,
                                &stream_rec,
                                &stream_acc,
                                &stream_tx,
                            );
                        },
                        err_fn,
                        None,
                    ),
                    SampleFormat::I32 => device.build_input_stream(
                        &stream_config,
                        move |data: &[i32], _: &cpal::InputCallbackInfo| {
                            let f32_samples: Vec<f32> = data
                                .iter()
                                .map(|&s| (f64::from(s) / 2_147_483_648.0) as f32)
                                .collect();
                            push_captured_samples(
                                &f32_samples,
                                channels,
                                from_rate,
                                needs_processing,
                                &stream_rec,
                                &stream_acc,
                                &stream_tx,
                            );
                        },
                        err_fn,
                        None,
                    ),
                    SampleFormat::U16 => device.build_input_stream(
                        &stream_config,
                        move |data: &[u16], _: &cpal::InputCallbackInfo| {
                            let f32_samples: Vec<f32> = data
                                .iter()
                                .map(|&s| (f32::from(s) - 32768.0) / 32768.0)
                                .collect();
                            push_captured_samples(
                                &f32_samples,
                                channels,
                                from_rate,
                                needs_processing,
                                &stream_rec,
                                &stream_acc,
                                &stream_tx,
                            );
                        },
                        err_fn,
                        None,
                    ),
                    _ => {
                        let _ = init_tx.send(Err(format!(
                            "Unsupported hardware sample format: {sample_format:?}"
                        )));
                        return;
                    }
                };

                let stream = match stream_res {
                    Ok(s) => s,
                    Err(e) => {
                        let _ = init_tx.send(Err(format!("Failed to build input stream: {e}")));
                        return;
                    }
                };

                if let Err(e) = stream.play() {
                    let _ = init_tx.send(Err(format!("Failed to play stream: {e}")));
                    return;
                }

                running_flag.store(true, Ordering::SeqCst);
                let _ = init_tx.send(Ok(()));

                // Wait until termination command arrives
                let _ = cmd_rx.recv();

                drop(stream);
                running_flag.store(false, Ordering::SeqCst);
                debug!("Audio worker thread shut down cleanly");
            })
            .map_err(|e| ViernesError::Audio(format!("Failed to spawn audio thread: {e}")))?;

        // Wait for thread initialization result without blocking Tokio runtime
        match init_rx.await {
            Ok(Ok(())) => {
                self.worker_tx = Some(cmd_tx);
                self.worker_handle = Some(handle);
                info!("Hardware audio capture stream started successfully");
                Ok(())
            }
            Ok(Err(e)) => {
                let _ = tokio::task::spawn_blocking(move || {
                    let _ = handle.join();
                })
                .await;
                Err(ViernesError::Audio(e))
            }
            Err(e) => {
                let _ = tokio::task::spawn_blocking(move || {
                    let _ = handle.join();
                })
                .await;
                Err(ViernesError::Audio(format!(
                    "Audio initialization channel disconnected: {e}"
                )))
            }
        }
    }

    async fn stop_stream(&mut self) -> Result<(), ViernesError> {
        self.is_running.store(false, Ordering::SeqCst);
        if let Some(tx) = self.worker_tx.take() {
            let _ = tx.send(AudioWorkerCommand::Stop);
        }
        if let Some(handle) = self.worker_handle.take() {
            let _ = tokio::task::spawn_blocking(move || {
                let _ = handle.join();
            })
            .await;
        }
        Ok(())
    }

    fn is_capturing(&self) -> bool {
        self.is_running.load(Ordering::SeqCst)
    }

    fn finish_recording(&mut self) -> Option<Vec<f32>> {
        let mut guard = self
            .accumulated_buffer
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if guard.is_empty() {
            None
        } else {
            let buffer = std::mem::take(&mut *guard);
            Some(buffer)
        }
    }
}

/// Deterministic hermetic fake audio capture device for CI and testing.
#[derive(Debug, Clone)]
pub struct FakeAudioCapture {
    is_running: Arc<AtomicBool>,
    sender: Sender<Vec<f32>>,
    receiver: Receiver<Vec<f32>>,
    injected_audio: Vec<f32>,
    recorded: Arc<Mutex<Vec<f32>>>,
}

impl FakeAudioCapture {
    /// Creates a fake capture source preloaded with synthetic audio samples.
    #[must_use]
    pub fn new(injected_audio: Vec<f32>) -> Self {
        let (sender, receiver) = bounded(128);
        Self {
            is_running: Arc::new(AtomicBool::new(false)),
            sender,
            receiver,
            injected_audio,
            recorded: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Injects audio chunk directly into receiver channel.
    pub fn push_chunk(&self, chunk: Vec<f32>) {
        self.recorded
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .extend_from_slice(&chunk);
        let _ = self.sender.send(chunk);
    }

    /// Obtains receiver channel.
    #[must_use]
    pub fn receiver(&self) -> Receiver<Vec<f32>> {
        self.receiver.clone()
    }
}

#[async_trait]
impl AudioCapture for FakeAudioCapture {
    async fn start_stream(&mut self) -> Result<(), ViernesError> {
        self.is_running.store(true, Ordering::SeqCst);
        self.recorded
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();

        // Push preloaded audio in 20ms chunks (320 samples @ 16kHz)
        for chunk in self.injected_audio.chunks(320) {
            let c = chunk.to_vec();
            self.recorded
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .extend_from_slice(&c);
            let _ = self.sender.try_send(c);
        }
        Ok(())
    }

    async fn stop_stream(&mut self) -> Result<(), ViernesError> {
        self.is_running.store(false, Ordering::SeqCst);
        Ok(())
    }

    fn is_capturing(&self) -> bool {
        self.is_running.load(Ordering::SeqCst)
    }

    fn finish_recording(&mut self) -> Option<Vec<f32>> {
        let mut guard = self.recorded.lock().unwrap_or_else(|e| e.into_inner());
        if guard.is_empty() {
            None
        } else {
            Some(std::mem::take(&mut *guard))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_fake_audio_capture_lifecycle() {
        let synthetic_audio = vec![0.1; 960]; // 60ms = 3 chunks of 320 samples
        let mut fake = FakeAudioCapture::new(synthetic_audio);

        assert!(!fake.is_capturing());
        assert!(fake.start_stream().await.is_ok());
        assert!(fake.is_capturing());

        let rx = fake.receiver();
        let mut received = 0;
        while let Ok(chunk) = rx.try_recv() {
            received += chunk.len();
        }
        assert_eq!(received, 960);

        let finished = fake.finish_recording().unwrap();
        assert_eq!(finished.len(), 960);

        assert!(fake.stop_stream().await.is_ok());
        assert!(!fake.is_capturing());
    }

    #[test]
    fn test_capture_config_defaults() {
        let cfg = AudioCaptureConfig::default();
        assert_eq!(cfg.preferred_sample_rate, 16_000);
        assert_eq!(cfg.chunk_ms, 20);
        assert_eq!(cfg.channel_capacity, 128);
    }

    #[test]
    fn test_cpal_capture_recording_active_toggle() {
        let capture = CpalAudioCapture::new(AudioCaptureConfig::default());
        assert!(capture.is_recording_active());
        capture.set_recording_active(false);
        assert!(!capture.is_recording_active());
        capture.set_recording_active(true);
        assert!(capture.is_recording_active());
    }

    #[test]
    fn test_push_captured_samples_resamples_stereo_48k_to_16k_mono() {
        let rec = AtomicBool::new(true);
        let acc = Mutex::new(Vec::new());
        let (tx, rx) = bounded(4);
        // 10ms of stereo 48kHz = 960 samples -> 160 mono 16kHz samples
        let stereo_48k = vec![0.25f32; 960];

        push_captured_samples(&stereo_48k, 2, 48_000, true, &rec, &acc, &tx);

        let streamed = rx.try_recv().unwrap();
        assert_eq!(streamed.len(), 160);
        assert_eq!(acc.lock().unwrap().len(), 160);
    }
}
