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

    /// Obtains a clone of the system events broadcast sender.
    #[must_use]
    pub fn event_sender(&self) -> broadcast::Sender<SystemEvent> {
        self.event_tx.clone()
    }

    /// Process state transitions under lock and then broadcast the external event to subscribers.
    pub async fn dispatch(
        &self,
        event: SystemEvent,
    ) -> Result<Option<TransitionRecord>, ViernesError> {
        let record = self.process_transition(event.clone()).await?;
        let _ = self.event_tx.send(event);
        Ok(record)
    }

    /// Determine the deterministic next state given current state and incoming event.
    fn compute_next_state(current: SystemState, event: &SystemEvent) -> Option<SystemState> {
        match (current, event) {
            // Idle transitions
            (SystemState::Idle, SystemEvent::VoiceStarted | SystemEvent::PttPressed) => {
                Some(SystemState::Listening)
            }
            // Resilient recovery: if synthesis starts while in Idle, transition to Speaking
            (SystemState::Idle, SystemEvent::SpeakingStarted | SystemEvent::SentenceReady(_)) => {
                Some(SystemState::Speaking)
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
            (SystemState::Thinking, SystemEvent::AwaitingConfirmationRequested { .. }) => {
                Some(SystemState::AwaitingConfirmation)
            }
            // Filler playback finishes while thinking: remain in Thinking
            (SystemState::Thinking, SystemEvent::FillerPlaybackFinished) => {
                Some(SystemState::Thinking)
            }

            // Speaking transitions: full assistant response completed -> return to Idle
            (SystemState::Speaking, SystemEvent::PlaybackFinished) => Some(SystemState::Idle),
            // Filler playback finishes while speaking: return to Thinking awaiting real response
            (SystemState::Speaking, SystemEvent::FillerPlaybackFinished) => {
                Some(SystemState::Thinking)
            }
            (SystemState::Speaking, SystemEvent::AwaitingConfirmationRequested { .. }) => {
                Some(SystemState::AwaitingConfirmation)
            }

            // Speaking interruption (Barge-in): human speaks while assistant talks
            (
                SystemState::Speaking,
                SystemEvent::VoiceStarted | SystemEvent::BargeIn | SystemEvent::PttPressed,
            ) => Some(SystemState::Listening),

            // AwaitingConfirmation transitions
            (
                SystemState::AwaitingConfirmation,
                SystemEvent::ConfirmationReceived { confirmed: true },
            ) => Some(SystemState::Thinking),
            (
                SystemState::AwaitingConfirmation,
                SystemEvent::ConfirmationReceived { confirmed: false },
            ) => Some(SystemState::Idle),
            (
                SystemState::AwaitingConfirmation,
                SystemEvent::VoiceStarted | SystemEvent::BargeIn | SystemEvent::PttPressed,
            ) => Some(SystemState::Listening),
            (
                SystemState::AwaitingConfirmation,
                SystemEvent::SpeakingStarted | SystemEvent::SentenceReady(_),
            ) => Some(SystemState::Speaking),

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
                // If transitioning from Speaking to Listening (or explicit BargeIn), activate atomic barge-in flag
                if (prev_state == SystemState::Speaking || matches!(event, SystemEvent::BargeIn))
                    && next_state == SystemState::Listening
                {
                    self.barge_in_active.store(true, Ordering::Release);
                    warn!(
                        from = %prev_state,
                        to = %next_state,
                        trigger = %event,
                        "Barge-in detected: audio playback interruption signaled"
                    );
                }

                *state_guard = next_state;
                drop(state_guard);

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
    async fn test_filler_playback_retains_thinking_state() {
        let sm = StateMachine::new(16);
        sm.dispatch(SystemEvent::VoiceStarted).await.unwrap();
        sm.dispatch(SystemEvent::PttReleased).await.unwrap();
        assert_eq!(sm.state().await, SystemState::Thinking);

        // Filler starts speaking: Thinking -> Speaking
        sm.dispatch(SystemEvent::SpeakingStarted).await.unwrap();
        assert_eq!(sm.state().await, SystemState::Speaking);

        // Filler finishes: Speaking -> Thinking (NOT Idle!)
        sm.dispatch(SystemEvent::FillerPlaybackFinished)
            .await
            .unwrap();
        assert_eq!(sm.state().await, SystemState::Thinking);

        // Real AI response arrives: Thinking -> Speaking
        sm.dispatch(SystemEvent::SpeakingStarted).await.unwrap();
        assert_eq!(sm.state().await, SystemState::Speaking);

        // Real response finishes: Speaking -> Idle
        sm.dispatch(SystemEvent::PlaybackFinished).await.unwrap();
        assert_eq!(sm.state().await, SystemState::Idle);
    }

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

    #[tokio::test]
    async fn test_awaiting_confirmation_accepted_and_rejected() {
        let sm = StateMachine::new(16);
        sm.dispatch(SystemEvent::VoiceStarted).await.unwrap();
        sm.dispatch(SystemEvent::PttReleased).await.unwrap();
        assert_eq!(sm.state().await, SystemState::Thinking);

        // Thinking -> AwaitingConfirmation
        let rec = sm
            .dispatch(SystemEvent::AwaitingConfirmationRequested {
                action: "rm -rf /tmp/foo".into(),
            })
            .await
            .unwrap();
        assert!(rec.is_some());
        assert_eq!(sm.state().await, SystemState::AwaitingConfirmation);

        // Confirmed -> Thinking
        let rec = sm
            .dispatch(SystemEvent::ConfirmationReceived { confirmed: true })
            .await
            .unwrap();
        assert!(rec.is_some());
        assert_eq!(sm.state().await, SystemState::Thinking);

        // Request again, then reject -> Idle
        sm.dispatch(SystemEvent::AwaitingConfirmationRequested {
            action: "drop table".into(),
        })
        .await
        .unwrap();
        assert_eq!(sm.state().await, SystemState::AwaitingConfirmation);

        let rec = sm
            .dispatch(SystemEvent::ConfirmationReceived { confirmed: false })
            .await
            .unwrap();
        assert!(rec.is_some());
        assert_eq!(sm.state().await, SystemState::Idle);
    }

    #[tokio::test]
    async fn test_awaiting_confirmation_from_speaking_and_barge_in() {
        let sm = StateMachine::new(16);
        sm.dispatch(SystemEvent::VoiceStarted).await.unwrap();
        sm.dispatch(SystemEvent::PttReleased).await.unwrap();
        sm.dispatch(SystemEvent::SpeakingStarted).await.unwrap();
        assert_eq!(sm.state().await, SystemState::Speaking);

        // Speaking -> AwaitingConfirmation
        sm.dispatch(SystemEvent::AwaitingConfirmationRequested {
            action: "restart service".into(),
        })
        .await
        .unwrap();
        assert_eq!(sm.state().await, SystemState::AwaitingConfirmation);

        // Barge-in during AwaitingConfirmation -> Listening & sets barge_in_active
        sm.dispatch(SystemEvent::BargeIn).await.unwrap();
        assert_eq!(sm.state().await, SystemState::Listening);
        assert!(sm.is_barge_in_active());
        sm.reset_barge_in();

        // VoiceStarted or PttPressed during AwaitingConfirmation -> Listening without barge_in_active
        sm.dispatch(SystemEvent::PttReleased).await.unwrap();
        sm.dispatch(SystemEvent::AwaitingConfirmationRequested {
            action: "delete file".into(),
        })
        .await
        .unwrap();
        sm.dispatch(SystemEvent::VoiceStarted).await.unwrap();
        assert_eq!(sm.state().await, SystemState::Listening);
        assert!(!sm.is_barge_in_active());

        sm.dispatch(SystemEvent::PttReleased).await.unwrap();
        sm.dispatch(SystemEvent::AwaitingConfirmationRequested {
            action: "delete file".into(),
        })
        .await
        .unwrap();
        sm.dispatch(SystemEvent::PttPressed).await.unwrap();
        assert_eq!(sm.state().await, SystemState::Listening);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_dispatch_updates_state_before_event_broadcast() {
        let sm = Arc::new(StateMachine::new(16));
        let mut event_rx = sm.subscribe_events();
        let (ack_tx, mut ack_rx) = tokio::sync::mpsc::channel::<()>(1);

        let sm_reader = sm.clone();
        let observer = tokio::spawn(async move {
            let mut observed = Vec::new();
            for _ in 0..4 {
                let ev = event_rx.recv().await.unwrap();
                let st = sm_reader.state().await;
                let barge_in = sm_reader.is_barge_in_active();
                observed.push((ev, st, barge_in));
                ack_tx.send(()).await.unwrap();
            }
            observed
        });

        // Give observer task a chance to start awaiting recv()
        tokio::task::yield_now().await;

        for event in [
            SystemEvent::VoiceStarted,
            SystemEvent::PttReleased,
            SystemEvent::SpeakingStarted,
            SystemEvent::BargeIn,
        ] {
            sm.dispatch(event).await.unwrap();
            ack_rx.recv().await.unwrap();
        }

        let observed = observer.await.unwrap();
        assert_eq!(observed[0].1, SystemState::Listening);
        assert!(!observed[0].2);
        assert_eq!(observed[1].1, SystemState::Thinking);
        assert!(!observed[1].2);
        assert_eq!(observed[2].1, SystemState::Speaking);
        assert!(!observed[2].2);
        assert_eq!(observed[3].1, SystemState::Listening);
        assert!(observed[3].2);
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
            Just(SystemEvent::FillerPlaybackFinished),
            Just(SystemEvent::BargeIn),
            Just(SystemEvent::ResetToIdle),
            Just(SystemEvent::SystemError("arbitrary".into())),
            Just(SystemEvent::ToolExecutionRequested {
                tool_name: "test".into()
            }),
            Just(SystemEvent::AwaitingConfirmationRequested {
                action: "rm".into()
            }),
            Just(SystemEvent::ConfirmationReceived { confirmed: true }),
            Just(SystemEvent::ConfirmationReceived { confirmed: false }),
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
                    // State must strictly be one of the 5 defined enum variants
                    prop_assert!(matches!(
                        st,
                        SystemState::Idle
                            | SystemState::Listening
                            | SystemState::Thinking
                            | SystemState::Speaking
                            | SystemState::AwaitingConfirmation
                    ));
                }
                Ok(())
            }).unwrap();
        }
    }
}
