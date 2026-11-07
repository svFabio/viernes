//! Core error definitions for Viernes.

use thiserror::Error;

/// Core domain errors across Viernes subsystems.
#[derive(Debug, Error)]
pub enum ViernesError {
    /// Configuration loading or parsing failure.
    #[error("configuration error: {0}")]
    Config(String),

    /// Illegal state transition requested.
    #[error("illegal state transition from '{from}' to '{to}': {reason}")]
    StateTransition {
        /// Origin state.
        from: String,
        /// Intended target state.
        to: String,
        /// Diagnostic reasoning.
        reason: String,
    },

    /// Broadcast event channel communication failure.
    #[error("broadcast channel error: {0}")]
    Broadcast(String),

    /// Text-to-speech provider failure.
    #[error("TTS error: {0}")]
    Tts(String),

    /// Speech-to-text inference failure.
    #[error("STT error: {0}")]
    Stt(String),

    /// Audio hardware capture or playback failure.
    #[error("audio error: {0}")]
    Audio(String),

    /// Audio hardware device enumeration or resolution failure.
    #[error("device error: {0}")]
    Device(String),

    /// Brain reasoning or subprocess streaming failure.
    #[error("brain error: {0}")]
    Brain(String),

    /// Standard I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Serialization or deserialization error.
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    /// Generic internal system failure.
    #[error("internal core error: {0}")]
    Internal(String),
}

/// Backward-compatible alias for ViernesError.
pub type CoreError = ViernesError;
