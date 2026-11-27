//! Central State Machine for Viernes Voice Assistant.

use serde::{Deserialize, Serialize};

use crate::config::{default_status_file_path, ensure_private_dir};
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

    /// Waiting for explicit user confirmation before executing a destructive action.
    AwaitingConfirmation,
}

impl std::fmt::Display for SystemState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Idle => write!(f, "Idle"),
            Self::Listening => write!(f, "Listening"),
            Self::Thinking => write!(f, "Thinking"),
            Self::Speaking => write!(f, "Speaking"),
            Self::AwaitingConfirmation => write!(f, "AwaitingConfirmation"),
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

            // Thinking transitions to Speaking when synthesis starts, AwaitingConfirmation for risky actions, or Idle on empty/error
            (Self::Thinking, Self::Speaking | Self::Idle | Self::AwaitingConfirmation) => true,

            // Speaking transitions to Idle on completion, Thinking after filler playback,
            // AwaitingConfirmation for risky actions, or Listening on Barge-in
            (
                Self::Speaking,
                Self::Idle | Self::Listening | Self::Thinking | Self::AwaitingConfirmation,
            ) => true,

            // AwaitingConfirmation transitions to Thinking on confirmation, Speaking, Listening on voice/barge-in, or Idle on cancel/reset
            (
                Self::AwaitingConfirmation,
                Self::Thinking | Self::Speaking | Self::Listening | Self::Idle,
            ) => true,

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

/// Legacy default file path where the daemon wrote its live status.
#[deprecated(note = "use viernes_core::config::default_status_file_path() instead")]
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
    /// Reads the current live status snapshot from `default_status_file_path()`.
    #[must_use]
    pub fn read_from_disk() -> Option<Self> {
        let path = default_status_file_path();
        let content = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str(&content).ok()
    }

    /// Atomically writes this status snapshot to disk via a private per-write temporary file rename.
    pub fn write_to_disk(&self) -> Result<(), std::io::Error> {
        let target_path = default_status_file_path();
        if let Some(parent) = target_path.parent() {
            if !parent.as_os_str().is_empty() {
                ensure_private_dir(parent)?;
            }
        }

        let file_name = target_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("status.json");
        let tmp_path =
            target_path.with_file_name(format!(".{file_name}.tmp.{}", uuid::Uuid::new_v4()));

        let content = serde_json::to_string(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let mut open_opts = std::fs::OpenOptions::new();
        open_opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            open_opts.mode(0o600);
        }

        let mut file = open_opts.open(&tmp_path)?;

        let write_result = (|| -> std::io::Result<()> {
            use std::io::Write;
            file.write_all(content.as_bytes())?;
            file.sync_all()?;
            drop(file);
            std::fs::rename(&tmp_path, &target_path)?;
            Ok(())
        })();

        if let Err(err) = write_result {
            let _ = std::fs::remove_file(&tmp_path);
            return Err(err);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::tests::{EnvGuard, ENV_LOCK};

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

    #[test]
    fn test_awaiting_confirmation_and_filler_transitions() {
        assert_eq!(
            SystemState::AwaitingConfirmation.to_string(),
            "AwaitingConfirmation"
        );

        // Allowed transitions into AwaitingConfirmation
        assert!(SystemState::Thinking.can_transition_to(SystemState::AwaitingConfirmation));
        assert!(SystemState::Speaking.can_transition_to(SystemState::AwaitingConfirmation));
        assert!(!SystemState::Idle.can_transition_to(SystemState::AwaitingConfirmation));
        assert!(!SystemState::Listening.can_transition_to(SystemState::AwaitingConfirmation));

        // Allowed transitions out of AwaitingConfirmation
        for target in [
            SystemState::Thinking,
            SystemState::Speaking,
            SystemState::Listening,
            SystemState::Idle,
            SystemState::AwaitingConfirmation,
        ] {
            assert!(SystemState::AwaitingConfirmation.can_transition_to(target));
        }

        // FillerPlaybackFinished: Speaking -> Thinking
        assert!(SystemState::Speaking.can_transition_to(SystemState::Thinking));
    }

    #[test]
    fn test_daemon_live_status_concurrent_writes_and_permissions() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _guard = EnvGuard::new(&["VIERNES_STATUS_FILE"]);

        let base_dir =
            std::env::temp_dir().join(format!("viernes-status-test-{}", uuid::Uuid::new_v4()));
        let status_path = base_dir.join("run/status.json");
        std::env::set_var("VIERNES_STATUS_FILE", &status_path);

        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .enable_all()
            .build()
            .unwrap();

        rt.block_on(async {
            let mut handles = Vec::new();
            for task_id in 0..16u64 {
                handles.push(tokio::spawn(async move {
                    for iter in 0..5u64 {
                        let snapshot = DaemonLiveStatus {
                            state: SystemState::Speaking,
                            last_user_prompt: format!("prompt-{task_id}-{iter}"),
                            last_assistant_text: format!("reply-{task_id}-{iter}"),
                            turn_counter: task_id * 10 + iter,
                            timestamp_ms: 1_700_000_000_000 + iter,
                        };
                        snapshot.write_to_disk().unwrap();
                    }
                }));
            }

            for h in handles {
                h.await.unwrap();
            }
        });

        let loaded = DaemonLiveStatus::read_from_disk().expect("status should be readable");
        assert_eq!(loaded.state, SystemState::Speaking);

        // No leftover temp files in parent directory
        let parent = status_path.parent().unwrap();
        let entries: Vec<_> = std::fs::read_dir(parent)
            .unwrap()
            .filter_map(Result::ok)
            .collect();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].file_name(), "status.json");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let dir_mode = std::fs::metadata(parent).unwrap().permissions().mode() & 0o777;
            assert_eq!(dir_mode, 0o700);
            let file_mode = std::fs::metadata(&status_path)
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(file_mode, 0o600);
        }

        let _ = std::fs::remove_dir_all(&base_dir);
    }

    #[test]
    fn test_daemon_live_status_cleans_up_temp_file_on_rename_failure() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _guard = EnvGuard::new(&["VIERNES_STATUS_FILE"]);

        let base_dir =
            std::env::temp_dir().join(format!("viernes-status-fail-{}", uuid::Uuid::new_v4()));
        let target_as_dir = base_dir.join("status.json");
        std::fs::create_dir_all(&target_as_dir).unwrap();
        std::env::set_var("VIERNES_STATUS_FILE", &target_as_dir);

        let status = DaemonLiveStatus::default();
        let err = status.write_to_disk().unwrap_err();
        assert!(err.kind() == std::io::ErrorKind::IsADirectory || err.raw_os_error().is_some());

        // Only the pre-existing `status.json` directory should remain; no `.tmp.*` files leaked
        let entries: Vec<_> = std::fs::read_dir(&base_dir)
            .unwrap()
            .filter_map(Result::ok)
            .collect();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].file_name(), "status.json");

        let _ = std::fs::remove_dir_all(&base_dir);
    }
}
