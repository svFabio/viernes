//! Central State Machine for Viernes Voice Assistant.

use serde::{Deserialize, Serialize};

use crate::error::ViernesError;

/// Discrete observable runtime states of the voice assistant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum SystemState {
    /// System is quiescent, waiting for speech or Push-to-Talk activation.
    #[default]
    Idle,

    /// Microphone is capturing audio; VAD is tracking voice energy.
    Listening,

    /// Speech segment is being transcribed and processed by Antigravity Brain.
    Thinking,

    /// TTS audio is actively playing back through the output speakers.
    Speaking,
}

impl std::fmt::Display for SystemState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Idle => write!(f, "Idle"),
            Self::Listening => write!(f, "Listening"),
            Self::Thinking => write!(f, "Thinking"),
            Self::Speaking => write!(f, "Speaking"),
        }
    }
}

impl SystemState {
    /// Evaluates whether a proposed transition from `self` to `target` is valid.
    #[must_use]
    pub const fn can_transition_to(self, target: Self) -> bool {
        match (self, target) {
            // Identity transitions are idempotent no-ops
            (s1, s2) if s1 as usize == s2 as usize => true,

            // Idle can transition into Listening (VAD trigger or PTT)
            (Self::Idle, Self::Listening) => true,

            // Listening transitions to Thinking when voice finishes, or back to Idle if silence/empty
            (Self::Listening, Self::Thinking | Self::Idle) => true,

            // Thinking transitions to Speaking when synthesis starts, or Idle on empty/error
            (Self::Thinking, Self::Speaking | Self::Idle) => true,

            // Speaking transitions to Idle on completion, or directly to Listening on Barge-in!
            (Self::Speaking, Self::Idle | Self::Listening) => true,

            // Any other transitions are disallowed
            _ => false,
        }
    }

    /// Attempts to mutate the state to `target`, returning a diagnostic error if invalid.
    pub fn transition_to(&mut self, target: Self) -> Result<(), ViernesError> {
        if self.can_transition_to(target) {
            *self = target;
            Ok(())
        } else {
            Err(ViernesError::StateTransition {
                from: self.to_string(),
                to: target.to_string(),
                reason: format!("Illegal transition from {self} to {target}"),
            })
        }
    }
}

/// Canonical file path where the daemon writes its live status for CLI and TUI.
pub const DAEMON_STATUS_FILE: &str = "/tmp/viernes_status.json";

/// Real-time live status snapshot written by the daemon for external tools, CLI, and TUI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DaemonLiveStatus {
    /// Current state of the daemon.
    pub state: SystemState,
    /// Last recognized user speech transcript.
    pub last_user_prompt: String,
    /// Last synthesized assistant response text.
    pub last_assistant_text: String,
    /// Monotonically increasing counter of conversation turns.
    pub turn_counter: u64,
    /// Unix timestamp in milliseconds when this status was emitted.
    pub timestamp_ms: u64,
}

impl Default for DaemonLiveStatus {
    fn default() -> Self {
        Self {
            state: SystemState::Idle,
            last_user_prompt: String::new(),
            last_assistant_text: String::new(),
            turn_counter: 0,
            timestamp_ms: 0,
        }
    }
}

impl DaemonLiveStatus {
    /// Reads the current live status snapshot from `/tmp/viernes_status.json` or `$VIERNES_STATUS_FILE`.
    #[must_use]
    pub fn read_from_disk() -> Option<Self> {
        let path_str =
            std::env::var("VIERNES_STATUS_FILE").unwrap_or_else(|_| DAEMON_STATUS_FILE.to_string());
        let path = std::path::Path::new(&path_str);
        if !path.exists() {
            return None;
        }
        let content = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&content).ok()
    }

    /// Atomically writes this status snapshot to disk via temporary file rename.
    pub fn write_to_disk(&self) -> Result<(), std::io::Error> {
        let target_path =
            std::env::var("VIERNES_STATUS_FILE").unwrap_or_else(|_| DAEMON_STATUS_FILE.to_string());
        let tmp_path = format!("{target_path}.tmp.{}", std::process::id());
        let content = serde_json::to_string(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(&tmp_path, content)?;
        std::fs::rename(&tmp_path, target_path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_lifecycle_transitions() {
        let mut state = SystemState::Idle;

        assert!(state.transition_to(SystemState::Listening).is_ok());
        assert_eq!(state, SystemState::Listening);

        assert!(state.transition_to(SystemState::Thinking).is_ok());
        assert_eq!(state, SystemState::Thinking);

        assert!(state.transition_to(SystemState::Speaking).is_ok());
        assert_eq!(state, SystemState::Speaking);

        assert!(state.transition_to(SystemState::Idle).is_ok());
        assert_eq!(state, SystemState::Idle);
    }

    #[test]
    fn test_barge_in_direct_transition() {
        let mut state = SystemState::Speaking;
        // User interrupts Viernes while speaking
        assert!(state.transition_to(SystemState::Listening).is_ok());
        assert_eq!(state, SystemState::Listening);
    }

    #[test]
    fn test_invalid_transition_rejected() {
        let mut state = SystemState::Idle;
        // Idle cannot jump directly to Speaking without Thinking
        let err = state.transition_to(SystemState::Speaking);
        assert!(err.is_err());
        assert_eq!(state, SystemState::Idle);
    }
}
