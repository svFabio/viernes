//! Shared architectural contracts and abstractions for all Viernes subsystems.

use async_trait::async_trait;
use uuid::Uuid;

use crate::error::ViernesError;
use crate::events::SystemEvent;

/// Contract for real-time microphone audio capture and VAD streaming.
#[async_trait]
pub trait AudioCapture: Send + Sync + std::fmt::Debug {
    /// Commences audio streaming from the host input device.
    async fn start_stream(&mut self) -> Result<(), ViernesError>;

    /// Stops audio streaming and cleans up hardware buffers.
    async fn stop_stream(&mut self) -> Result<(), ViernesError>;

    /// Indicates whether the input stream is actively capturing.
    fn is_capturing(&self) -> bool;

    /// Forces immediate termination of active recording buffer (PTT release).
    fn finish_recording(&mut self) -> Option<Vec<f32>>;
}

/// Contract for local speech-to-text inference.
#[async_trait]
pub trait SpeechToText: Send + Sync + std::fmt::Debug {
    /// Transcribes an in-memory buffer of PCM Float32 audio samples.
    async fn transcribe(&self, samples: &[f32], sample_rate: u32) -> Result<String, ViernesError>;
}

/// Contract for Antigravity Brain streaming reasoning and session management.
#[async_trait]
pub trait BrainService: Send + Sync + std::fmt::Debug {
    /// Submits a user transcript and returns an event receiver stream.
    async fn send_prompt(
        &mut self,
        prompt: &str,
    ) -> Result<tokio::sync::mpsc::Receiver<SystemEvent>, ViernesError>;

    /// Rotates the active conversation pointer and returns the archived UUID.
    async fn rotate_conversation(&mut self) -> Result<Uuid, ViernesError>;

    /// Returns the currently bound active conversation UUID.
    fn active_conversation_id(&self) -> Uuid;
}

/// Contract for Text-to-Speech synthesis engines (ElevenLabs / Edge-TTS).
#[async_trait]
pub trait TextToSpeech: Send + Sync + std::fmt::Debug {
    /// Synthesizes text into raw MP3 audio bytes.
    async fn synthesize(&self, text: &str) -> Result<Vec<u8>, ViernesError>;

    /// Human-readable provider identifier for telemetry and logging.
    fn provider_name(&self) -> &'static str;
}

/// Contract for non-blocking streaming audio playback with Barge-in capability.
#[async_trait]
pub trait AudioPlayback: Send + Sync + std::fmt::Debug {
    /// Enqueues audio bytes for immediate streaming playback.
    async fn enqueue_chunk(&mut self, audio_bytes: Vec<u8>) -> Result<(), ViernesError>;

    /// Instantly halts playback and clears pending queued buffers (Barge-in).
    fn stop_immediately(&mut self) -> Result<(), ViernesError>;

    /// Indicates whether audio is currently sounding through output speakers.
    fn is_playing(&self) -> bool;
}

// ----------------------------------------------------------------------------
// Test Doubles (Hermetic Fakes for CI and A/B Parity)
// ----------------------------------------------------------------------------

/// In-memory fake STT provider for deterministic testing.
#[derive(Debug, Default, Clone)]
pub struct FakeSpeechToText {
    /// Fixed transcript returned on calls to transcribe.
    pub default_transcript: String,
}

#[async_trait]
impl SpeechToText for FakeSpeechToText {
    async fn transcribe(
        &self,
        _samples: &[f32],
        _sample_rate: u32,
    ) -> Result<String, ViernesError> {
        Ok(self.default_transcript.clone())
    }
}

/// In-memory fake TTS provider producing synthetic dummy bytes.
#[derive(Debug, Default, Clone)]
pub struct FakeTextToSpeech {
    /// Provider identification tag.
    pub name: &'static str,
}

#[async_trait]
impl TextToSpeech for FakeTextToSpeech {
    async fn synthesize(&self, text: &str) -> Result<Vec<u8>, ViernesError> {
        if text.trim().is_empty() {
            return Err(ViernesError::Tts("Cannot synthesize empty text".into()));
        }
        // Emit dummy MP3 header + payload bytes
        let mut dummy = vec![0xFF, 0xFB, 0x90, 0x64];
        dummy.extend_from_slice(text.as_bytes());
        Ok(dummy)
    }

    fn provider_name(&self) -> &'static str {
        self.name
    }
}
