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
