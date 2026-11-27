//! Conversational fillers manager and ultra-low latency dispatcher (<150ms).

use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{create_dir_all, File};
use std::io::{ErrorKind, Write};
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
    audio_cache: Arc<RwLock<HashMap<String, Vec<u8>>>>,
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

        let fillers = match std::fs::read_to_string(&file_path) {
            Ok(content) => match serde_json::from_str::<FillerCollection>(&content) {
                Ok(parsed) => parsed,
                Err(e) => {
                    warn!(
                        path = %file_path.display(),
                        error = %e,
                        "Failed to parse fillers configuration; using default fillers"
                    );
                    FillerCollection::default()
                }
            },
            Err(e) if e.kind() == ErrorKind::NotFound => {
                let defaults = FillerCollection::default();
                if let Some(parent) = file_path.parent() {
                    if let Err(dir_err) = create_dir_all(parent) {
                        warn!(
                            path = %parent.display(),
                            error = %dir_err,
                            "Failed to create fillers configuration directory"
                        );
                    }
                }
                match serde_json::to_string_pretty(&defaults) {
                    Ok(serialized) => match File::create(&file_path) {
                        Ok(mut f) => {
                            if let Err(write_err) = f.write_all(serialized.as_bytes()) {
                                warn!(
                                    path = %file_path.display(),
                                    error = %write_err,
                                    "Failed to write default fillers configuration"
                                );
                            }
                        }
                        Err(create_err) => {
                            warn!(
                                path = %file_path.display(),
                                error = %create_err,
                                "Failed to create default fillers configuration file"
                            );
                        }
                    },
                    Err(ser_err) => {
                        warn!(error = %ser_err, "Failed to serialize default fillers configuration");
                    }
                }
                defaults
            }
            Err(e) => {
                warn!(
                    path = %file_path.display(),
                    error = %e,
                    "Failed to read fillers configuration; using default fillers"
                );
                FillerCollection::default()
            }
        };

        Ok(Self {
            file_path,
            fillers: Arc::new(RwLock::new(fillers)),
            audio_cache: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Converts a conversational filler phrase into a canonical filesystem cache key.
    #[must_use]
    pub fn phrase_to_cache_key(phrase: &str) -> String {
        let normalized = phrase
            .to_lowercase()
            .replace('á', "a")
            .replace('é', "e")
            .replace('í', "i")
            .replace('ó', "o")
            .replace(['ú', 'ü'], "u")
            .replace('ñ', "n");

        let mut key = String::new();
        for c in normalized.chars() {
            if c.is_ascii_alphanumeric() {
                key.push(c);
            } else if c == ' ' && !key.ends_with('_') {
                key.push('_');
            }
        }
        key.trim_matches('_').to_string()
    }

    /// Pre-warm and index all `.mp3` audio files found in `cache_dir` into RAM.
    pub async fn prewarm_audio_cache<P: AsRef<Path>>(&self, cache_dir: P) -> usize {
        let dir = cache_dir.as_ref();
        match tokio::fs::try_exists(dir).await {
            Ok(false) => {
                if let Err(e) = tokio::fs::create_dir_all(dir).await {
                    warn!(
                        path = %dir.display(),
                        error = %e,
                        "Failed to create filler audio cache directory"
                    );
                }
                return 0;
            }
            Err(e) => {
                warn!(
                    path = %dir.display(),
                    error = %e,
                    "Failed to check existence of filler audio cache directory"
                );
                return 0;
            }
            Ok(true) => {}
        }

        let mut entries = match tokio::fs::read_dir(dir).await {
            Ok(entries) => entries,
            Err(e) => {
                warn!(
                    path = %dir.display(),
                    error = %e,
                    "Failed to read filler audio cache directory"
                );
                return 0;
            }
        };

        let mut loaded = Vec::new();
        loop {
            match entries.next_entry().await {
                Ok(Some(entry)) => {
                    let path = entry.path();
                    let is_file = tokio::fs::metadata(&path)
                        .await
                        .map(|m| m.is_file())
                        .unwrap_or(false);
                    if is_file && path.extension().is_some_and(|ext| ext == "mp3") {
                        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                            match tokio::fs::read(&path).await {
                                Ok(bytes) if !bytes.is_empty() => {
                                    loaded.push((stem.to_string(), bytes));
                                }
                                Ok(_) => {
                                    warn!(
                                        path = %path.display(),
                                        "Skipping empty filler audio file"
                                    );
                                }
                                Err(e) => {
                                    warn!(
                                        path = %path.display(),
                                        error = %e,
                                        "Failed to read filler audio file"
                                    );
                                }
                            }
                        }
                    }
                }
                Ok(None) => break,
                Err(e) => {
                    warn!(
                        path = %dir.display(),
                        error = %e,
                        "Failed reading directory entry in filler audio cache"
                    );
                    break;
                }
            }
        }

        let loaded_count = loaded.len();
        if loaded_count > 0 {
            let mut guard = self.audio_cache.write().await;
            for (stem, bytes) in loaded {
                guard.insert(stem, bytes);
            }
            info!(
                loaded_count,
                "Pre-warmed conversational filler audio clips into RAM successfully"
            );
        }
        loaded_count
    }

    /// Retrieve pre-warmed audio bytes for a phrase, matching against canonical cache key.
    pub async fn get_cached_audio(&self, phrase: &str) -> Option<Vec<u8>> {
        let key = Self::phrase_to_cache_key(phrase);
        let guard = self.audio_cache.read().await;
        guard.get(&key).cloned()
    }

    /// Manually cache synthesized audio for a phrase.
    pub async fn store_cached_audio(&self, phrase: &str, audio: Vec<u8>) {
        let key = Self::phrase_to_cache_key(phrase);
        let mut guard = self.audio_cache.write().await;
        guard.insert(key, audio);
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
        match tokio::fs::read_to_string(&self.file_path).await {
            Ok(content) => {
                let loaded: FillerCollection = serde_json::from_str(&content)?;
                *self.fillers.write().await = loaded;
                info!("Reloaded conversational fillers from disk");
                Ok(())
            }
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
            Err(e) => {
                warn!(
                    path = %self.file_path.display(),
                    error = %e,
                    "Failed to read fillers configuration during reload"
                );
                Err(BrainError::FillersError(e.to_string()))
            }
        }
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

    #[tokio::test]
    async fn test_get_cached_audio_returns_none_on_cache_miss() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("fillers.json");
        let cache = FillerCacheManager::new(Some(path)).unwrap();

        cache
            .store_cached_audio("Un momento, Señor.", vec![1, 2, 3, 4])
            .await;

        assert_eq!(
            cache.get_cached_audio("Un momento, Señor.").await,
            Some(vec![1, 2, 3, 4])
        );
        // Must return None rather than returning unrelated cached audio
        assert_eq!(
            cache.get_cached_audio("Frase que no está en caché").await,
            None
        );
    }

    #[test]
    fn test_phrase_to_cache_key_normalizes_diaeresis_and_accents() {
        assert_eq!(
            FillerCacheManager::phrase_to_cache_key("Pingüino en cigüeña"),
            "pinguino_en_ciguena"
        );
        assert_eq!(
            FillerCacheManager::phrase_to_cache_key("Übungen ÁÉÍÓÚÑ"),
            "ubungen_aeioun"
        );
    }

    #[tokio::test]
    async fn test_prewarm_audio_cache_async_loads_mp3s_and_skips_empty() {
        let dir = tempdir().unwrap();
        let cfg_path = dir.path().join("fillers.json");
        let audio_dir = dir.path().join("audio");
        std::fs::create_dir_all(&audio_dir).unwrap();

        let phrase = "Deme un segundo, Señor.";
        let key = FillerCacheManager::phrase_to_cache_key(phrase);
        std::fs::write(audio_dir.join(format!("{key}.mp3")), b"ID3AUDIO").unwrap();
        std::fs::write(audio_dir.join("empty.mp3"), b"").unwrap();
        std::fs::write(audio_dir.join("ignored.txt"), b"hello").unwrap();

        let cache = FillerCacheManager::new(Some(cfg_path)).unwrap();
        let count = cache.prewarm_audio_cache(&audio_dir).await;
        assert_eq!(count, 1);

        assert_eq!(
            cache.get_cached_audio(phrase).await,
            Some(b"ID3AUDIO".to_vec())
        );
        assert_eq!(cache.get_cached_audio("empty").await, None);
        assert_eq!(cache.get_cached_audio("ignored").await, None);
    }

    #[tokio::test]
    async fn test_reload_updates_fillers_and_errors_on_invalid_json() {
        let dir = tempdir().unwrap();
        let cfg_path = dir.path().join("fillers.json");
        let cache = FillerCacheManager::new(Some(&cfg_path)).unwrap();

        let updated = FillerCollection {
            immediate: vec!["Solo esta frase.".into()],
            tools: HashMap::new(),
            patience: vec!["Esperando...".into()],
        };
        std::fs::write(&cfg_path, serde_json::to_string(&updated).unwrap()).unwrap();
        cache.reload().await.unwrap();

        assert_eq!(
            cache.select_immediate().await,
            Some("Solo esta frase.".into())
        );

        std::fs::write(&cfg_path, "{corrupt json").unwrap();
        assert!(cache.reload().await.is_err());
    }
}
