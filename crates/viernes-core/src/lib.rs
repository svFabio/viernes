//! Core domain library for the Viernes Voice Assistant.
//!
//! Provides architectural contracts, discrete event models,
//! the central deterministic state machine, and configuration loading.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod config;
pub mod error;
pub mod events;
pub mod state;
pub mod state_machine;
pub mod traits;

pub use config::ViernesConfig;
pub use error::{CoreError, ViernesError};
pub use events::{State, SystemEvent, TransitionRecord};
pub use state::SystemState;
pub use state_machine::StateMachine;
pub use traits::{AudioCapture, AudioPlayback, BrainService, SpeechToText, TextToSpeech};

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn prop_state_transitions_never_panic(
            s1 in prop_oneof![
                Just(SystemState::Idle),
                Just(SystemState::Listening),
                Just(SystemState::Thinking),
                Just(SystemState::Speaking),
            ],
            s2 in prop_oneof![
                Just(SystemState::Idle),
                Just(SystemState::Listening),
                Just(SystemState::Thinking),
                Just(SystemState::Speaking),
            ]
        ) {
            let mut state = s1;
            let res = state.transition_to(s2);
            if s1.can_transition_to(s2) {
                prop_assert!(res.is_ok());
                prop_assert_eq!(state, s2);
            } else {
                prop_assert!(res.is_err());
                prop_assert_eq!(state, s1);
            }
        }
    }
}
