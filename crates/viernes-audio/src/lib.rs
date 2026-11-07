//! Viernes Audio Subsystem: Real-time microphone capture, defensive resampling,
//! Voice Activity Detection (VAD), Push-to-Talk coordination, and local STT.
//!
//! #![forbid(unsafe_code)] enforces 100% memory-safe Rust across all modules.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod capture;
pub mod devices;
pub mod ptt;
pub mod resample;
pub mod stt;
pub mod vad;

pub use capture::{AudioCaptureConfig, CpalAudioCapture, FakeAudioCapture};
pub use devices::{
    resolve_device, resolve_input_device_from_env, resolve_output_device_from_env, AudioDeviceInfo,
    CpalDeviceEnumerator, DeviceEnumerator, MockDeviceEnumerator,
};
pub use ptt::{CaptureArbitrationMode, MockPttListener, PttArbitrator, PttEvent, PttListener};
pub use resample::{downmix_to_mono, process_to_16k_mono, resample_linear};
pub use stt::{
    clean_and_filter_hallucinations, LocalWhisperTranscriber, MockTranscriber, WhisperConfig,
};
pub use vad::{compute_rms, VadEvent, VadState, VoiceActivityDetector};

use tokio::sync::mpsc;
use tracing::{debug, error, info};
use viernes_core::error::ViernesError;
use viernes_core::events::SystemEvent;
use viernes_core::traits::{AudioCapture, SpeechToText};

/// High-level audio pipeline coordinator.
///
/// Bridges the real-time audio capture stream, the VAD energy detector,
/// and Push-to-Talk arbitration, emitting strongly-typed [`SystemEvent`] messages
/// directly to the central Viernes state machine and Brain actor.
pub struct AudioPipeline<C: AudioCapture, S: SpeechToText> {
    capture: C,
    vad: VoiceActivityDetector,
    stt: S,
    arbitrator: PttArbitrator,
    event_tx: mpsc::Sender<SystemEvent>,
}

impl<C: AudioCapture, S: SpeechToText> AudioPipeline<C, S> {
    /// Creates a new unified audio processing pipeline.
    pub fn new(
        capture: C,
        vad: VoiceActivityDetector,
        stt: S,
        event_tx: mpsc::Sender<SystemEvent>,
    ) -> Self {
        Self {
            capture,
            vad,
            stt,
            arbitrator: PttArbitrator::new(),
            event_tx,
        }
    }

    /// Handles a raw audio chunk processed from the capture stream.
    pub async fn process_chunk(&mut self, chunk: &[f32]) -> Result<(), ViernesError> {
        let events = self.vad.feed_chunk(chunk);

        for ev in events {
            match ev {
                VadEvent::VoiceStarted => {
                    if self.arbitrator.on_vad_started() {
                        let _ = self.event_tx.send(SystemEvent::VoiceStarted).await;
                    }
                }
                VadEvent::VoiceFinished(samples) => {
                    self.arbitrator.on_vad_finished();
                    // Transcribe finished speech segment
                    match self.stt.transcribe(&samples, 16_000).await {
                        Ok(transcript) => {
                            if !transcript.is_empty() {
                                info!("AudioPipeline: Transcription ready: '{}'", transcript);
                                let _ = self
                                    .event_tx
                                    .send(SystemEvent::TranscriptReady(transcript))
                                    .await;
                            } else {
                                debug!("AudioPipeline: Transcription was empty or filtered");
                            }
                        }
                        Err(e) => {
                            error!("AudioPipeline: STT transcription error: {e}");
                            let _ = self
                                .event_tx
                                .send(SystemEvent::SystemError(e.to_string()))
                                .await;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Handles a Push-to-Talk event with deterministic priority over VAD.
    pub async fn handle_ptt_event(&mut self, ptt_event: PttEvent) -> Result<(), ViernesError> {
        match ptt_event {
            PttEvent::Pressed => {
                self.arbitrator.on_ptt_pressed();
                self.vad.reset();
                let _ = self.event_tx.send(SystemEvent::PttPressed).await;
            }
            PttEvent::Released => {
                self.arbitrator.on_ptt_released();
                let _ = self.event_tx.send(SystemEvent::PttReleased).await;
                if let Some(samples) = self.capture.finish_recording() {
                    match self.stt.transcribe(&samples, 16_000).await {
                        Ok(transcript) => {
                            if !transcript.is_empty() {
                                let _ = self
                                    .event_tx
                                    .send(SystemEvent::TranscriptReady(transcript))
                                    .await;
                            }
                        }
                        Err(e) => {
                            let _ = self
                                .event_tx
                                .send(SystemEvent::SystemError(e.to_string()))
                                .await;
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_pipeline_vad_to_transcript() {
        let (tx, mut rx) = mpsc::channel(16);

        let fake_capture = FakeAudioCapture::new(vec![]);
        let vad = VoiceActivityDetector::new(16000, 0.05, 100);
        let mock_stt = MockTranscriber::new("Hola Viernes, ¿cómo estás?");

        let mut pipeline = AudioPipeline::new(fake_capture, vad, mock_stt, tx);

        let voice_frame = vec![0.2; 320];
        let silence_frame = vec![0.001; 320];

        // 2 voice frames to trigger VoiceStarted
        pipeline.process_chunk(&voice_frame).await.unwrap();
        pipeline.process_chunk(&voice_frame).await.unwrap();

        let ev1 = rx.recv().await.unwrap();
        assert_eq!(ev1, SystemEvent::VoiceStarted);

        // Feed sustained speech (> 250ms = 13 frames of 20ms)
        for _ in 0..15 {
            pipeline.process_chunk(&voice_frame).await.unwrap();
        }

        // Feed silence to trigger VoiceFinished and STT
        for _ in 0..10 {
            pipeline.process_chunk(&silence_frame).await.unwrap();
        }

        let ev2 = rx.recv().await.unwrap();
        assert_eq!(
            ev2,
            SystemEvent::TranscriptReady("Hola Viernes, ¿cómo estás?".into())
        );
    }

    #[tokio::test]
    async fn test_pipeline_ptt_precedence() {
        let (tx, mut rx) = mpsc::channel(16);

        let mut fake_capture = FakeAudioCapture::new(vec![]);
        fake_capture.start_stream().await.unwrap();
        fake_capture.push_chunk(vec![0.1; 1600]);

        let vad = VoiceActivityDetector::new(16000, 0.05, 500);
        let mock_stt = MockTranscriber::new("Comando de prueba");

        let mut pipeline = AudioPipeline::new(fake_capture, vad, mock_stt, tx);

        // Press PTT
        pipeline.handle_ptt_event(PttEvent::Pressed).await.unwrap();
        let ev1 = rx.recv().await.unwrap();
        assert_eq!(ev1, SystemEvent::PttPressed);

        // Release PTT
        pipeline.handle_ptt_event(PttEvent::Released).await.unwrap();
        let ev2 = rx.recv().await.unwrap();
        assert_eq!(ev2, SystemEvent::PttReleased);

        let ev3 = rx.recv().await.unwrap();
        assert_eq!(
            ev3,
            SystemEvent::TranscriptReady("Comando de prueba".into())
        );
    }
}
