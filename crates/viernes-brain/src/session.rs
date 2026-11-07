//! Dynamic Session Pointer and conversational memory rotation manager.

use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::fs::{create_dir_all, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;
use uuid::Uuid;

use crate::error::BrainError;

/// Stored session pointer state containing current session ID and archived history.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionState {
    /// Active session UUID.
    pub current_session_id: String,
    /// Timestamp when current session was created.
    pub created_at: DateTime<Utc>,
    /// History of past session UUIDs rotated out.
    pub archived_sessions: Vec<String>,
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            current_session_id: Uuid::new_v4().to_string(),
            created_at: Utc::now(),
            archived_sessions: Vec::new(),
        }
    }
}

/// Thread-safe manager for dynamic session pointers and atomic disk persistence.
pub struct SessionPointerManager {
    file_path: PathBuf,
    state: Arc<RwLock<SessionState>>,
    rotation_regex: Regex,
}

impl SessionPointerManager {
    /// Initialize with custom path or default `~/.config/viernes/session.json`.
    pub fn new<P: AsRef<Path>>(path: Option<P>) -> Result<Self, BrainError> {
        let file_path = match path {
            Some(p) => p.as_ref().to_path_buf(),
            None => {
                let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
                PathBuf::from(home).join(".config/viernes/session.json")
            }
        };

        // Regex pattern detecting rotation commands in Spanish
        let pattern = r"(?i)\b(abre|inicia|comienza|empezar|abrir|iniciar)\s+(otra|una\s+nueva)\s+(conversaci[oó]n|charla|sesi[oó]n)\b|\b(nueva\s+conversaci[oó]n|resetear\s+sesi[oó]n)\b";
        let rotation_regex = Regex::new(pattern)
            .map_err(|e| BrainError::Internal(format!("invalid regex pattern: {e}")))?;

        let state = if file_path.exists() {
            match std::fs::read_to_string(&file_path) {
                Ok(content) => serde_json::from_str::<SessionState>(&content).unwrap_or_default(),
                Err(_) => SessionState::default(),
            }
        } else {
            SessionState::default()
        };

        let manager = Self {
            file_path,
            state: Arc::new(RwLock::new(state)),
            rotation_regex,
        };

        manager.save_atomic_sync()?;
        Ok(manager)
    }

    /// Check if user spoken input intends to rotate the conversation session.
    #[must_use]
    pub fn detects_rotation_intent(&self, input: &str) -> bool {
        self.rotation_regex.is_match(input.trim())
    }

    /// Retrieve the currently active session ID.
    pub async fn current_session_id(&self) -> String {
        self.state.read().await.current_session_id.clone()
    }

    /// Rotate session: generate new UUIDv4, archive old ID, and atomically persist.
    pub async fn rotate_session(&self) -> Result<String, BrainError> {
        let mut guard = self.state.write().await;
        let old_id = guard.current_session_id.clone();
        let new_id = Uuid::new_v4().to_string();

        info!(
            old_session = %old_id,
            new_session = %new_id,
            "Rotating conversation session pointer"
        );

        guard.archived_sessions.push(old_id);
        guard.current_session_id = new_id.clone();
        guard.created_at = Utc::now();

        self.save_atomic(&guard)?;
        Ok(new_id)
    }

    /// Atomically persist current state using temporary file + rename.
    fn save_atomic(&self, state: &SessionState) -> Result<(), BrainError> {
        if let Some(parent) = self.file_path.parent() {
            create_dir_all(parent).map_err(|e| BrainError::SessionError(e.to_string()))?;
        }

        let temp_path = self.file_path.with_extension("tmp");
        let serialized = serde_json::to_string_pretty(state)?;

        {
            let mut file =
                File::create(&temp_path).map_err(|e| BrainError::SessionError(e.to_string()))?;
            file.write_all(serialized.as_bytes())
                .map_err(|e| BrainError::SessionError(e.to_string()))?;
            file.sync_all()
                .map_err(|e| BrainError::SessionError(e.to_string()))?;
        }

        std::fs::rename(&temp_path, &self.file_path)
            .map_err(|e| BrainError::SessionError(e.to_string()))?;

        Ok(())
    }

    fn save_atomic_sync(&self) -> Result<(), BrainError> {
        if let Ok(guard) = self.state.try_read() {
            self.save_atomic(&guard)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_rotation_intent_regex_detection() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("session.json");
        let mgr = SessionPointerManager::new(Some(path)).unwrap();

        assert!(mgr.detects_rotation_intent("Viernes, abre otra conversación por favor"));
        assert!(mgr.detects_rotation_intent("inicia una nueva sesión"));
        assert!(mgr.detects_rotation_intent("quiero una nueva conversación"));
        assert!(!mgr.detects_rotation_intent("¿Cómo está el clima hoy?"));
        assert!(!mgr.detects_rotation_intent("Abre el navegador"));
    }

    #[tokio::test]
    async fn test_atomic_persistence_and_rotation() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("session.json");
        let mgr = SessionPointerManager::new(Some(&path)).unwrap();

        let initial_id = mgr.current_session_id().await;
        assert!(!initial_id.is_empty());

        let new_id = mgr.rotate_session().await.unwrap();
        assert_ne!(initial_id, new_id);

        // Verify state reloaded from disk matches rotated state
        let mgr_reloaded = SessionPointerManager::new(Some(&path)).unwrap();
        assert_eq!(mgr_reloaded.current_session_id().await, new_id);
    }

    #[tokio::test]
    async fn test_concurrent_access() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("session.json");
        let mgr = Arc::new(SessionPointerManager::new(Some(&path)).unwrap());

        let mut handles = Vec::new();
        for _ in 0..10 {
            let m = Arc::clone(&mgr);
            handles.push(tokio::spawn(async move {
                let _ = m.current_session_id().await;
                let _ = m.rotate_session().await;
            }));
        }

        for h in handles {
            h.await.unwrap();
        }

        // Must still read clean valid JSON
        let final_mgr = SessionPointerManager::new(Some(&path)).unwrap();
        assert!(!final_mgr.current_session_id().await.is_empty());
    }
}
