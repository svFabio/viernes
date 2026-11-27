//! Dynamic Session Pointer and conversational memory rotation manager.

use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::fs::{create_dir_all, File};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::sync::RwLock;
use tracing::{info, warn};
use uuid::Uuid;

use crate::error::BrainError;

/// Stored session pointer state containing current session ID and archived history.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionState {
    /// Active session UUID.
    #[serde(alias = "active_conversation_id")]
    pub current_session_id: String,
    /// Timestamp when current session was created.
    #[serde(alias = "updated_at", default = "chrono::Utc::now")]
    pub created_at: DateTime<Utc>,
    /// History of past session UUIDs rotated out.
    #[serde(alias = "history", default)]
    pub archived_sessions: Vec<String>,
    /// Total voice turns conducted in current session.
    #[serde(default)]
    pub turn_count: u32,
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            current_session_id: Uuid::new_v4().to_string(),
            created_at: Utc::now(),
            archived_sessions: Vec::new(),
            turn_count: 0,
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
        let pattern = r"(?i)\b(abre|inicia|comienza|empezar|abrir|iniciar|abrí|empezá)\s+(otra|una\s+nueva|de\s+cero)\s+(conversaci[oó]n|charla|sesi[oó]n)\b|\b(nueva\s+conversaci[oó]n|nueva\s+sesi[oó]n|resetear\s+sesi[oó]n|limpiar\s+contexto|empezar\s+de\s+cero)\b";
        let rotation_regex = Regex::new(pattern)
            .map_err(|e| BrainError::Internal(format!("invalid regex pattern: {e}")))?;

        let state = match std::fs::read_to_string(&file_path) {
            Ok(content) => match serde_json::from_str::<SessionState>(&content) {
                Ok(parsed) => parsed,
                Err(parse_err) => {
                    warn!(
                        path = %file_path.display(),
                        error = %parse_err,
                        "Corrupt session file detected; backing up before creating fresh state"
                    );
                    let file_name = file_path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "session.json".to_string());
                    let backup_name =
                        format!("{file_name}.corrupt.{}", Utc::now().timestamp_millis());
                    let backup_path = file_path.with_file_name(backup_name);
                    if let Err(rename_err) = std::fs::rename(&file_path, &backup_path) {
                        warn!(
                            src = %file_path.display(),
                            dst = %backup_path.display(),
                            error = %rename_err,
                            "Failed to back up corrupt session file"
                        );
                    }
                    SessionState::default()
                }
            },
            Err(e) if e.kind() == ErrorKind::NotFound => SessionState::default(),
            Err(e) => {
                return Err(BrainError::SessionError(format!(
                    "failed to read session file {}: {e}",
                    file_path.display()
                )));
            }
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

    /// Retrieve active turn count for this session.
    pub async fn turn_count(&self) -> u32 {
        self.state.read().await.turn_count
    }

    /// Increment turn count and atomically save.
    pub async fn increment_turn_count(&self) -> Result<u32, BrainError> {
        let mut guard = self.state.write().await;
        guard.turn_count = guard.turn_count.saturating_add(1);
        let count = guard.turn_count;
        self.save_atomic_async(&guard).await?;
        Ok(count)
    }

    /// Check if conversation context is getting long and we should proactively suggest rotating.
    pub async fn should_suggest_rotation(&self) -> bool {
        let guard = self.state.read().await;
        guard.turn_count == 25 || guard.turn_count == 35 || guard.turn_count == 50
    }

    /// Atomically update active conversation ID if agy assigned a real ID on init.
    pub async fn update_conversation_id(&self, new_id: &str) -> Result<(), BrainError> {
        let trimmed = new_id.trim();
        if trimmed.is_empty() {
            warn!("Rejecting empty or whitespace-only conversation ID update");
            return Err(BrainError::SessionError(
                "conversation ID must not be empty or whitespace-only".into(),
            ));
        }

        let mut guard = self.state.write().await;
        if guard.current_session_id != trimmed {
            guard.current_session_id = trimmed.to_string();
            self.save_atomic_async(&guard).await?;
        }
        Ok(())
    }

    /// Rotate session: generate new UUIDv4, archive old ID, reset turn count, and atomically persist.
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
        guard.turn_count = 0;

        self.save_atomic_async(&guard).await?;
        Ok(new_id)
    }

    fn temp_path(&self) -> PathBuf {
        let temp_filename = format!("session.{}.tmp", Uuid::new_v4());
        match self.file_path.parent() {
            Some(parent) => parent.join(temp_filename),
            None => PathBuf::from(temp_filename),
        }
    }

    /// Atomically persist current state asynchronously using temporary file + rename.
    async fn save_atomic_async(&self, state: &SessionState) -> Result<(), BrainError> {
        if let Some(parent) = self.file_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| BrainError::SessionError(e.to_string()))?;
        }

        let temp_path = self.temp_path();
        let serialized = serde_json::to_string_pretty(state)?;

        let write_res: Result<(), BrainError> = async {
            let mut file = tokio::fs::File::create(&temp_path)
                .await
                .map_err(|e| BrainError::SessionError(e.to_string()))?;
            file.write_all(serialized.as_bytes())
                .await
                .map_err(|e| BrainError::SessionError(e.to_string()))?;
            file.sync_all()
                .await
                .map_err(|e| BrainError::SessionError(e.to_string()))?;
            Ok(())
        }
        .await;

        if let Err(e) = write_res {
            if let Err(rm_err) = tokio::fs::remove_file(&temp_path).await {
                if rm_err.kind() != ErrorKind::NotFound {
                    warn!(path = %temp_path.display(), error = %rm_err, "Failed removing temp session file");
                }
            }
            return Err(e);
        }

        if let Err(e) = tokio::fs::rename(&temp_path, &self.file_path).await {
            if let Err(rm_err) = tokio::fs::remove_file(&temp_path).await {
                if rm_err.kind() != ErrorKind::NotFound {
                    warn!(path = %temp_path.display(), error = %rm_err, "Failed removing temp session file after rename failure");
                }
            }
            return Err(BrainError::SessionError(e.to_string()));
        }

        Ok(())
    }

    /// Atomically persist current state synchronously during initialization.
    fn save_atomic(&self, state: &SessionState) -> Result<(), BrainError> {
        if let Some(parent) = self.file_path.parent() {
            create_dir_all(parent).map_err(|e| BrainError::SessionError(e.to_string()))?;
        }

        let temp_path = self.temp_path();
        let serialized = serde_json::to_string_pretty(state)?;

        let write_res = (|| -> Result<(), BrainError> {
            let mut file =
                File::create(&temp_path).map_err(|e| BrainError::SessionError(e.to_string()))?;
            file.write_all(serialized.as_bytes())
                .map_err(|e| BrainError::SessionError(e.to_string()))?;
            file.sync_all()
                .map_err(|e| BrainError::SessionError(e.to_string()))?;
            Ok(())
        })();

        if let Err(e) = write_res {
            let _ = std::fs::remove_file(&temp_path);
            return Err(e);
        }

        if let Err(e) = std::fs::rename(&temp_path, &self.file_path) {
            let _ = std::fs::remove_file(&temp_path);
            return Err(BrainError::SessionError(e.to_string()));
        }

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
        assert!(mgr.detects_rotation_intent("empezar de cero"));
        assert!(mgr.detects_rotation_intent("limpiar contexto"));
        assert!(!mgr.detects_rotation_intent("¿Cómo está el clima hoy?"));
        assert!(!mgr.detects_rotation_intent("Abre el navegador"));
        assert!(!mgr.detects_rotation_intent("abre otra ventana"));
        assert!(!mgr.detects_rotation_intent("inicia otra aplicación"));
        assert!(!mgr.detects_rotation_intent("abrir una nueva pestaña"));
    }

    #[tokio::test]
    async fn test_corrupt_session_file_is_backed_up() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("session.json");
        let corrupt_payload = "{not valid json!!!";
        std::fs::write(&path, corrupt_payload).unwrap();

        let mgr = SessionPointerManager::new(Some(&path)).unwrap();
        assert!(!mgr.current_session_id().await.is_empty());

        let backups: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("session.json.corrupt.")
            })
            .collect();
        assert_eq!(backups.len(), 1);
        let backed_up = std::fs::read_to_string(backups[0].path()).unwrap();
        assert_eq!(backed_up, corrupt_payload);

        // Fresh valid JSON must now exist at session.json
        let reloaded = SessionPointerManager::new(Some(&path)).unwrap();
        assert_eq!(
            reloaded.current_session_id().await,
            mgr.current_session_id().await
        );
    }

    #[test]
    fn test_non_not_found_io_error_returns_err() {
        let dir = tempdir().unwrap();
        // Passing a directory path causes read_to_string to fail with non-NotFound IO error
        let res = SessionPointerManager::new(Some(dir.path()));
        assert!(matches!(res, Err(BrainError::SessionError(_))));
    }

    #[tokio::test]
    async fn test_update_conversation_id_rejects_empty_and_whitespace() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("session.json");
        let mgr = SessionPointerManager::new(Some(&path)).unwrap();
        let original = mgr.current_session_id().await;

        assert!(mgr.update_conversation_id("").await.is_err());
        assert_eq!(mgr.current_session_id().await, original);

        assert!(mgr.update_conversation_id("   \t\n ").await.is_err());
        assert_eq!(mgr.current_session_id().await, original);

        mgr.update_conversation_id("  conv-42  ").await.unwrap();
        assert_eq!(mgr.current_session_id().await, "conv-42");

        let reloaded = SessionPointerManager::new(Some(&path)).unwrap();
        assert_eq!(reloaded.current_session_id().await, "conv-42");
    }

    #[tokio::test]
    async fn test_increment_turn_count_saturates_without_overflow() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("session.json");
        let state = SessionState {
            current_session_id: "sess-max".into(),
            created_at: Utc::now(),
            archived_sessions: Vec::new(),
            turn_count: u32::MAX,
        };
        std::fs::write(&path, serde_json::to_string(&state).unwrap()).unwrap();

        let mgr = SessionPointerManager::new(Some(&path)).unwrap();
        let count = mgr.increment_turn_count().await.unwrap();
        assert_eq!(count, u32::MAX);
        assert_eq!(mgr.turn_count().await, u32::MAX);
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
