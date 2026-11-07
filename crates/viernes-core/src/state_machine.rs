//! Deterministic central state machine for Viernes.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, info, warn};

use crate::error::ViernesError;
use crate::events::{SystemEvent, TransitionRecord};
use crate::state::SystemState;

/// Default broadcast channel capacity for high-throughput event dispatch.
pub const DEFAULT_EVENT_CHANNEL_CAPACITY: usize = 256;

/// State machine coordinator managing central assistant lifecycle and event bus.
pub struct StateMachine {
    current_state: Arc<RwLock<SystemState>>,
    event_tx: broadcast::Sender<SystemEvent>,
    transition_tx: broadcast::Sender<TransitionRecord>,
    barge_in_active: Arc<AtomicBool>,
}

impl Default for StateMachine {
    fn default() -> Self {
        Self::new(DEFAULT_EVENT_CHANNEL_CAPACITY)
    }
}

impl StateMachine {
    /// Create a new StateMachine instance with the given broadcast capacity.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        let (event_tx, _) = broadcast::channel(capacity);
        let (transition_tx, _) = broadcast::channel(capacity);

        Self {
            current_state: Arc::new(RwLock::new(SystemState::Idle)),
            event_tx,
            transition_tx,
            barge_in_active: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Read the current state atomically.
    pub async fn state(&self) -> SystemState {
        *self.current_state.read().await
    }

    /// Subscribe to system events broadcast channel.
    #[must_use]
    pub fn subscribe_events(&self) -> broadcast::Receiver<SystemEvent> {
        self.event_tx.subscribe()
    }

    /// Subscribe to state machine transition records.
    #[must_use]
    pub fn subscribe_transitions(&self) -> broadcast::Receiver<TransitionRecord> {
        self.transition_tx.subscribe()
    }

    /// Check if barge-in interruption is currently requested.
    #[must_use]
    pub fn is_barge_in_active(&self) -> bool {
        self.barge_in_active.load(Ordering::Acquire)
    }

    /// Clear the barge-in flag once playback cancellation is fulfilled.
    pub fn reset_barge_in(&self) {
        self.barge_in_active.store(false, Ordering::Release);
    }

    /// Broadcast an external event to all subscribers and process state transitions.
    pub async fn dispatch(
        &self,
        event: SystemEvent,
    ) -> Result<Option<TransitionRecord>, ViernesError> {
        let _ = self.event_tx.send(event.clone());
        self.process_transition(event).await
    }

    /// Determine the deterministic next state given current state and incoming event.
    fn compute_next_state(current: SystemState, event: &SystemEvent) -> Option<SystemState> {
        match (current, event) {
            // Idle transitions
            (SystemState::Idle, SystemEvent::VoiceStarted | SystemEvent::PttPressed) => {
                Some(SystemState::Listening)
            }

            // Listening transitions
            (
                SystemState::Listening,
                SystemEvent::VoiceFinished { .. }
                | SystemEvent::PttReleased
                | SystemEvent::TranscriptReady(_),
            ) => Some(SystemState::Thinking),

            // Thinking transitions
            (
                SystemState::Thinking,
                SystemEvent::SpeakingStarted | SystemEvent::SentenceReady(_),
            ) => Some(SystemState::Speaking),

            // Speaking transitions: playback completed -> return to Idle
            (SystemState::Speaking, SystemEvent::PlaybackFinished) => Some(SystemState::Idle),

            // Speaking interruption (Barge-in): human speaks while assistant talks
            (
                SystemState::Speaking,
                SystemEvent::VoiceStarted | SystemEvent::BargeIn | SystemEvent::PttPressed,
            ) => Some(SystemState::Listening),

            // Global reset triggers
            (_, SystemEvent::ResetToIdle | SystemEvent::SystemError(_)) => Some(SystemState::Idle),

            // Discard unexpected events safely without transitioning or corrupting state
            _ => None,
        }
    }

    /// Execute state transition logic deterministically with structured telemetry logging.
    async fn process_transition(
        &self,
        event: SystemEvent,
    ) -> Result<Option<TransitionRecord>, ViernesError> {
        let mut state_guard = self.current_state.write().await;
        let prev_state = *state_guard;

        if let Some(next_state) = Self::compute_next_state(prev_state, &event) {
            if prev_state != next_state {
                // If transitioning from Speaking to Listening, activate atomic barge-in flag
                if prev_state == SystemState::Speaking && next_state == SystemState::Listening {
                    self.barge_in_active.store(true, Ordering::Release);
                    warn!(
                        from = %prev_state,
                        to = %next_state,
                        trigger = %event,
                        "Barge-in detected: audio playback interruption signaled"
                    );
                }

                *state_guard = next_state;

                let record = TransitionRecord {
                    timestamp: chrono::Utc::now(),
                    from_state: prev_state,
                    to_state: next_state,
                    trigger_event: event.to_string(),
                };

                info!(
                    timestamp = %record.timestamp,
                    from = %record.from_state,
                    to = %record.to_state,
                    trigger = %record.trigger_event,
                    "StateMachine transition committed"
                );

                let _ = self.transition_tx.send(record.clone());
                return Ok(Some(record));
            }
        } else {
            debug!(
                current = %prev_state,
                event = %event,
                "StateMachine received unhandled or idempotent event; maintaining current state"
            );
        }

        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[tokio::test]
    async fn test_happy_path_lifecycle() {
        let sm = StateMachine::new(16);
        let mut rx = sm.subscribe_transitions();

        assert_eq!(sm.state().await, SystemState::Idle);

        // Idle -> Listening
        let rec = sm.dispatch(SystemEvent::VoiceStarted).await.unwrap();
        assert!(rec.is_some());
        assert_eq!(sm.state().await, SystemState::Listening);

        // Listening -> Thinking
        let rec = sm
            .dispatch(SystemEvent::VoiceFinished {
                samples: vec![0.0; 160],
                sample_rate: 16000,
            })
            .await
            .unwrap();
        assert!(rec.is_some());
        assert_eq!(sm.state().await, SystemState::Thinking);

        // Thinking -> Speaking
        let rec = sm.dispatch(SystemEvent::SpeakingStarted).await.unwrap();
        assert!(rec.is_some());
        assert_eq!(sm.state().await, SystemState::Speaking);

        // Speaking -> Idle
        let rec = sm.dispatch(SystemEvent::PlaybackFinished).await.unwrap();
        assert!(rec.is_some());
        assert_eq!(sm.state().await, SystemState::Idle);

        // Verify broadcast events
        let mut count = 0;
        while let Ok(_transition) = rx.try_recv() {
            count += 1;
        }
        assert_eq!(count, 4);
    }

    #[tokio::test]
    async fn test_barge_in_interruption() {
        let sm = StateMachine::new(16);
        sm.dispatch(SystemEvent::VoiceStarted).await.unwrap();
        sm.dispatch(SystemEvent::TranscriptReady("test".into()))
            .await
            .unwrap();
        sm.dispatch(SystemEvent::SpeakingStarted).await.unwrap();
        assert_eq!(sm.state().await, SystemState::Speaking);
        assert!(!sm.is_barge_in_active());

        // Human talks while Speaking -> triggers barge-in flag & transitions to Listening
        let rec = sm.dispatch(SystemEvent::VoiceStarted).await.unwrap();
        assert!(rec.is_some());
        assert_eq!(sm.state().await, SystemState::Listening);
        assert!(sm.is_barge_in_active());

        sm.reset_barge_in();
        assert!(!sm.is_barge_in_active());
    }

    #[tokio::test]
    async fn test_system_error_recovers_to_idle() {
        let sm = StateMachine::new(16);
        sm.dispatch(SystemEvent::VoiceStarted).await.unwrap();
        assert_eq!(sm.state().await, SystemState::Listening);

        sm.dispatch(SystemEvent::SystemError("Network drop".to_string()))
            .await
            .unwrap();

        assert_eq!(sm.state().await, SystemState::Idle);
    }

    // Property-based test verifying random sequences never deadlock or leave undefined states
    fn arb_system_event() -> impl Strategy<Value = SystemEvent> {
        prop_oneof![
            Just(SystemEvent::VoiceStarted),
            Just(SystemEvent::VoiceFinished {
                samples: vec![0.0; 10],
                sample_rate: 16000
            }),
            Just(SystemEvent::PttPressed),
            Just(SystemEvent::PttReleased),
            Just(SystemEvent::TranscriptReady("hello".into())),
            Just(SystemEvent::SpeakingStarted),
            Just(SystemEvent::PlaybackFinished),
            Just(SystemEvent::BargeIn),
            Just(SystemEvent::ResetToIdle),
            Just(SystemEvent::SystemError("arbitrary".into())),
            Just(SystemEvent::ToolExecutionRequested {
                tool_name: "test".into()
            }),
            Just(SystemEvent::ConfirmationReceived { confirmed: true }),
        ]
    }

    proptest! {
        #[test]
        fn test_state_machine_proptest_invariants(events in prop::collection::vec(arb_system_event(), 1..60)) {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();

            rt.block_on(async {
                let sm = StateMachine::new(128);
                for event in events {
                    let _ = sm.dispatch(event).await;
                    let st = sm.state().await;
                    // State must strictly be one of the 4 defined enum variants
                    prop_assert!(matches!(st, SystemState::Idle | SystemState::Listening | SystemState::Thinking | SystemState::Speaking));
                }
                Ok(())
            }).unwrap();
        }
    }
}
