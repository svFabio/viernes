//! Typed error definitions for the Viernes TTS subsystem.

use thiserror::Error;
use viernes_core::error::ViernesError;

/// Specific error types arising during TTS synthesis, key rotation, or playback.
#[derive(Debug, Error)]
pub enum TtsError {
    /// Text passed to synthesis was empty or whitespace.
    #[error("Cannot synthesize empty text")]
    EmptyText,

    /// All API keys in the ElevenLabs key pool have been exhausted (401/429).
    #[error("All ElevenLabs API keys in pool are exhausted ({0} keys attempted)")]
    AllKeysExhausted(usize),

    /// Key pool is completely empty (no keys configured).
    #[error("No API keys configured in pool")]
    NoKeysConfigured,

    /// HTTP client or network error when communicating with ElevenLabs.
    #[error("HTTP network error: {0}")]
    Http(#[from] reqwest::Error),

    /// Edge-TTS WebSocket error.
    #[error("Edge-TTS WebSocket error: {0}")]
    WebSocket(String),

    /// Protocol or SSML formatting error.
    #[error("TTS protocol error: {0}")]
    Protocol(String),

    /// Audio player or sink failure.
    #[error("Audio playback error: {0}")]
    Playback(String),

    /// Temporary storage or cache management error.
    #[error("Storage error: {0}")]
    Storage(String),

    /// General I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

impl From<TtsError> for ViernesError {
    fn from(err: TtsError) -> Self {
        ViernesError::Tts(err.to_string())
    }
}
