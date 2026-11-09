//! Conversational fillers manager and ultra-low latency dispatcher (<150ms).

use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{create_dir_all, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, RwLock};
use tokio::time::sleep;
use tracing::{debug, info, warn};

use crate::error::BrainError;

/// Patience delay threshold before firing waiting filler (6.5 seconds).
pub const PATIENCE_TIMEOUT: Duration = Duration::from_millis(6500);

/// Categorized collection of conversational fillers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FillerCollection {
    /// Immediate phrases fired upon speech termination.
    #[serde(alias = "generic", default)]
    pub immediate: Vec<String>,
    /// Tool category mapped phrases (e.g. "terminal", "filesystem", "web", "files", "search").
    #[serde(default)]
    pub tools: HashMap<String, Vec<String>>,
    /// Patience phrases fired when reasoning exceeds 6.5s.
    #[serde(default)]
    pub patience: Vec<String>,
}

impl Default for FillerCollection {
    fn default() -> Self {
        let mut tools = HashMap::new();
        tools.insert(
            "terminal".to_string(),
            vec![
                "Usaré la terminal para verificar, Señor, un momento...".into(),
                "Consultando la consola del sistema, un segundo...".into(),
            ],
        );
        tools.insert(
            "files".to_string(),
            vec![
                "Permítame revisar los archivos, Señor, un momento...".into(),
                "Revisando los ficheros locales del proyecto...".into(),
            ],
        );
        tools.insert(
            "filesystem".to_string(),
            vec![
                "Permítame revisar los archivos, Señor, un momento...".into(),
                "Revisando los ficheros locales del proyecto...".into(),
            ],
        );
        tools.insert(
            "search".to_string(),
            vec![
                "Buscando información actualizada en la red, un instante...".into(),
                "Consultando fuentes web, un momento...".into(),
            ],
        );
        tools.insert(
            "web".to_string(),
            vec![
                "Buscando información actualizada en la red, un instante...".into(),
                "Consultando fuentes web, un momento...".into(),
            ],
        );

        Self {
            immediate: vec![
                "Deme un segundo, Señor, ya lo reviso.".into(),
                "Enseguida, Señor, estoy consultando información.".into(),
                "Un momento, Señor.".into(),
                "Revisando sus registros, deme un segundo.".into(),
                "Lo escucho, Señor, un segundo.".into(),
            ],
            tools,
            patience: vec![
                "Sigo procesando su consulta, Señor...".into(),
                "El razonamiento está tomando unos momentos adicionales...".into(),
            ],
        }
    }
}

/// Thread-safe manager for pre-synthesized fillers and configuration loading.
pub struct FillerCacheManager {
    file_path: PathBuf,
    fillers: Arc<RwLock<FillerCollection>>,
}

impl FillerCacheManager {
    /// Initialize with custom path or default `~/.config/viernes/fillers.json`.
    pub fn new<P: AsRef<Path>>(path: Option<P>) -> Result<Self, BrainError> {
        let file_path = match path {
            Some(p) => p.as_ref().to_path_buf(),
            None => {
                let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
                PathBuf::from(home).join(".config/viernes/fillers.json")
            }
        };

        let fillers = if file_path.exists() {
            match std::fs::read_to_string(&file_path) {
                Ok(content) => {
                    serde_json::from_str::<FillerCollection>(&content).unwrap_or_default()
                }
                Err(_) => FillerCollection::default(),
            }
        } else {
            let defaults = FillerCollection::default();
            if let Some(parent) = file_path.parent() {
                let _ = create_dir_all(parent);
            }
            if let Ok(serialized) = serde_json::to_string_pretty(&defaults) {
                if let Ok(mut f) = File::create(&file_path) {
                    let _ = f.write_all(serialized.as_bytes());
                }
            }
            defaults
        };

        Ok(Self {
            file_path,
            fillers: Arc::new(RwLock::new(fillers)),
        })
    }

    /// Select an immediate filler phrase at random in < 150ms.
    pub async fn select_immediate(&self) -> Option<String> {
        let guard = self.fillers.read().await;
        let mut rng = rand::thread_rng();
        guard.immediate.choose(&mut rng).cloned()
    }

    /// Select a tool-specific filler phrase for the given category or tool name.
    pub async fn select_tool_filler(&self, tool_or_category: &str) -> Option<String> {
        let guard = self.fillers.read().await;
        let mut rng = rand::thread_rng();

        let category = match tool_or_category {
            "read_file" | "view_file" | "grep_search" | "find_by_name" | "list_dir"
            | "filesystem" | "files" => "files",
            "run_command" | "send_command_input" | "command_status" | "terminal" => "terminal",
            "search_web" | "search_marketplace" | "web" | "search" => "search",
            other => other,
        };

        if let Some(list) = guard
            .tools
            .get(category)
            .or_else(|| guard.tools.get(tool_or_category))
        {
            list.choose(&mut rng).cloned()
        } else {
            guard.immediate.choose(&mut rng).cloned()
        }
    }

    /// Select a patience filler phrase at random.
    pub async fn select_patience(&self) -> Option<String> {
        let guard = self.fillers.read().await;
        let mut rng = rand::thread_rng();
        guard.patience.choose(&mut rng).cloned()
    }

    /// Reload configuration dynamically from disk.
    pub async fn reload(&self) -> Result<(), BrainError> {
        if self.file_path.exists() {
            let content = std::fs::read_to_string(&self.file_path)
                .map_err(|e| BrainError::FillersError(e.to_string()))?;
            let loaded: FillerCollection = serde_json::from_str(&content)?;
            *self.fillers.write().await = loaded;
            info!("Reloaded conversational fillers from disk");
        }
        Ok(())
    }
}

/// Asynchronous dispatcher coordinating immediate and patience filler execution.
pub struct FillerDispatcher {
    cache_manager: Arc<FillerCacheManager>,
    output_tx: mpsc::Sender<String>,
}

impl FillerDispatcher {
    /// Create a new dispatcher sending selected filler phrases to the playback channel.
    #[must_use]
    pub fn new(cache_manager: Arc<FillerCacheManager>, output_tx: mpsc::Sender<String>) -> Self {
        Self {
            cache_manager,
            output_tx,
        }
    }

    /// Dispatch immediate filler and measure execution latency.
    pub async fn dispatch_immediate(&self) -> Result<Duration, BrainError> {
        let start = Instant::now();
        if let Some(phrase) = self.cache_manager.select_immediate().await {
            let elapsed = start.elapsed();
            debug!(elapsed_us = elapsed.as_micros(), phrase = %phrase, "Selected immediate filler");
            self.output_tx
                .send(phrase)
                .await
                .map_err(|e| BrainError::Internal(e.to_string()))?;
            Ok(elapsed)
        } else {
            Ok(start.elapsed())
        }
    }

    /// Spawn a patience monitor that automatically fires if not cancelled within 6.5 seconds.
    pub fn spawn_patience_timer(
        &self,
        mut cancel_rx: tokio::sync::oneshot::Receiver<()>,
    ) -> tokio::task::JoinHandle<()> {
        let cache = Arc::clone(&self.cache_manager);
        let tx = self.output_tx.clone();

        tokio::spawn(async move {
            tokio::select! {
                _ = sleep(PATIENCE_TIMEOUT) => {
                    if let Some(patience_phrase) = cache.select_patience().await {
                        warn!("Inactivity threshold reached (6.5s); firing patience filler");
                        let _ = tx.send(patience_phrase).await;
                    }
                }
                _ = &mut cancel_rx => {
                    debug!("Patience timer cancelled; model responded before 6.5s");
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_filler_dispatch_latency_under_150ms() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("fillers.json");
        let cache = Arc::new(FillerCacheManager::new(Some(path)).unwrap());

        let (tx, mut rx) = mpsc::channel(8);
        let dispatcher = FillerDispatcher::new(cache, tx);

        let latency = dispatcher.dispatch_immediate().await.unwrap();
        assert!(
            latency < Duration::from_millis(150),
            "Latency was {:?}, which exceeded 150ms limit",
            latency
        );

        let received = rx.recv().await;
        assert!(received.is_some());
        assert!(!received.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_tool_categorization() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("fillers.json");
        let cache = FillerCacheManager::new(Some(path)).unwrap();

        let term_filler = cache.select_tool_filler("terminal").await.unwrap();
        assert!(term_filler.contains("terminal") || term_filler.contains("consola"));

        let web_filler = cache.select_tool_filler("web").await.unwrap();
        assert!(web_filler.contains("red") || web_filler.contains("web"));
    }

    #[tokio::test]
    async fn test_patience_timer_cancellation() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("fillers.json");
        let cache = Arc::new(FillerCacheManager::new(Some(path)).unwrap());

        let (tx, mut rx) = mpsc::channel(8);
        let dispatcher = FillerDispatcher::new(cache, tx);

        let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel();
        let handle = dispatcher.spawn_patience_timer(cancel_rx);

        // Cancel quickly before 6.5s
        let _ = cancel_tx.send(());
        handle.await.unwrap();

        // Channel should not have received patience filler
        assert!(rx.try_recv().is_err());
    }
}
