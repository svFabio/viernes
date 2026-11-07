//! Event models dispatched across the Viernes reactive pipeline.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::state::SystemState;

/// Type alias for backward compatibility.
pub type State = SystemState;

/// Discrete event payload broadcasted across subsystem actors.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SystemEvent {
    /// VAD detected user voice activity beginning.
    VoiceStarted,

    /// VAD detected speech termination with captured audio buffer.
    VoiceFinished {
        /// Linear Float32 PCM audio samples normalized to [-1.0, 1.0].
        samples: Vec<f32>,
        /// Audio sampling frequency in Hz (typically 16000).
        sample_rate: u32,
    },

    /// Push-to-Talk hotkey was depressed.
    PttPressed,

    /// Push-to-Talk hotkey was released.
    PttReleased,

    /// Local STT completed transcription of spoken input.
    TranscriptReady(String),

    /// Antigravity Brain internal reasoning thought chunk.
    BrainThought(String),

    /// Incremental token chunk received from Brain.
    BrainToken(String),

    /// Tokenizer accumulated a complete sentence ready for synthesis.
    SentenceReady(String),

    /// Audio output sink started playback.
    SpeakingStarted,

    /// Audio output sink completed playing all queued buffers.
    PlaybackFinished,

    /// User interrupted active playback by starting to speak.
    BargeIn,

    /// Central state machine transitioned between states.
    StateChanged {
        /// Origin state.
        from: SystemState,
        /// New state.
        to: SystemState,
    },

    /// Tool execution was requested by Brain.
    ToolExecutionRequested {
        /// Name of requested tool.
        tool_name: String,
    },

    /// Tool execution completed.
    ToolExecutionCompleted {
        /// Name of tool.
        tool_name: String,
        /// Execution status.
        success: bool,
    },

    /// Destructive command confirmation required.
    AwaitingConfirmationRequested {
        /// Description of action.
        action: String,
    },

    /// User answered confirmation prompt.
    ConfirmationReceived {
        /// Confirmed status.
        confirmed: bool,
    },

    /// Subsystem diagnostic or recoverable error.
    SystemError(String),

    /// Reset state back to Idle.
    ResetToIdle,
}

impl std::fmt::Display for SystemEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::VoiceStarted => write!(f, "VoiceStarted"),
            Self::VoiceFinished {
                samples,
                sample_rate,
            } => {
                write!(
                    f,
                    "VoiceFinished({} samples @ {}Hz)",
                    samples.len(),
                    sample_rate
                )
            }
            Self::PttPressed => write!(f, "PttPressed"),
            Self::PttReleased => write!(f, "PttReleased"),
            Self::TranscriptReady(t) => write!(f, "TranscriptReady({t})"),
            Self::BrainThought(t) => write!(f, "BrainThought({t})"),
            Self::BrainToken(t) => write!(f, "BrainToken({t})"),
            Self::SentenceReady(s) => write!(f, "SentenceReady({s})"),
            Self::SpeakingStarted => write!(f, "SpeakingStarted"),
            Self::PlaybackFinished => write!(f, "PlaybackFinished"),
            Self::BargeIn => write!(f, "BargeIn"),
            Self::StateChanged { from, to } => write!(f, "StateChanged({from} -> {to})"),
            Self::ToolExecutionRequested { tool_name } => {
                write!(f, "ToolExecutionRequested({tool_name})")
            }
            Self::ToolExecutionCompleted { tool_name, success } => {
                write!(f, "ToolExecutionCompleted({tool_name}, success={success})")
            }
            Self::AwaitingConfirmationRequested { action } => {
                write!(f, "AwaitingConfirmationRequested({action})")
            }
            Self::ConfirmationReceived { confirmed } => {
                write!(f, "ConfirmationReceived(confirmed={confirmed})")
            }
            Self::SystemError(msg) => write!(f, "SystemError({msg})"),
            Self::ResetToIdle => write!(f, "ResetToIdle"),
        }
    }
}

/// Telemetry record emitted on every state machine transition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransitionRecord {
    /// UTC timestamp of transition event.
    pub timestamp: DateTime<Utc>,
    /// State before transition.
    pub from_state: SystemState,
    /// State after transition.
    pub to_state: SystemState,
    /// Event causing the transition.
    pub trigger_event: String,
}
