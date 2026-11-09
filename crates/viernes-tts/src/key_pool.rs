//! Multi-account API key pool with atomic lock-free rotation for ElevenLabs.

use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::SystemTime;

use tracing::{info, warn};

use crate::error::TtsError;

/// Default cooldown duration (60 seconds) before an exhausted key is automatically rehabilitated.
pub const KEY_COOLDOWN_SECS: u64 = 60;

/// An individual API key entry with an exhausted marker and cooldown timestamp.
struct KeyEntry {
    key: String,
    exhausted: AtomicBool,
    exhausted_at: AtomicU64,
}

impl KeyEntry {
    fn new(key: String) -> Self {
        Self {
            key,
            exhausted: AtomicBool::new(false),
            exhausted_at: AtomicU64::new(0),
        }
    }

    fn is_exhausted(&self) -> bool {
        if !self.exhausted.load(Ordering::Relaxed) {
            return false;
        }

        let ts = self.exhausted_at.load(Ordering::Relaxed);
        if ts > 0 {
            let now = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();

            if now.saturating_sub(ts) >= KEY_COOLDOWN_SECS {
                // Cooldown period elapsed: rehabilitate key automatically
                self.exhausted.store(false, Ordering::SeqCst);
                self.exhausted_at.store(0, Ordering::SeqCst);
                info!(
                    key = %redact_key(&self.key),
                    "ElevenLabs API key cooldown period expired ({}s); key automatically rehabilitated",
                    KEY_COOLDOWN_SECS
                );
                return false;
            }
        }

        true
    }

    fn mark_exhausted(&self) {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.exhausted_at.store(now, Ordering::SeqCst);
        self.exhausted.store(true, Ordering::SeqCst);
    }

    fn reset(&self) {
        self.exhausted.store(false, Ordering::SeqCst);
        self.exhausted_at.store(0, Ordering::SeqCst);
    }
}

/// Thread-safe multi-account API key pool managed via `AtomicUsize`.
pub struct KeyPool {
    keys: Arc<Vec<KeyEntry>>,
    current_index: AtomicUsize,
}

impl KeyPool {
    /// Constructs a `KeyPool` from a slice or vector of API keys.
    ///
    /// Empty strings or strings with only whitespace are discarded.
    #[must_use]
    pub fn new(raw_keys: Vec<String>) -> Self {
        let clean_keys: Vec<KeyEntry> = raw_keys
            .into_iter()
            .map(|k| k.trim().to_string())
            .filter(|k| !k.is_empty())
            .map(KeyEntry::new)
            .collect();

        Self {
            keys: Arc::new(clean_keys),
            current_index: AtomicUsize::new(0),
        }
    }

    /// Parses keys from a comma-separated string (e.g., from `ELEVENLABS_API_KEYS`).
    #[must_use]
    pub fn from_comma_separated(raw: &str) -> Self {
        let keys: Vec<String> = raw
            .split(',')
            .map(str::trim)
            .filter(|k| !k.is_empty())
            .map(ToString::to_string)
            .collect();
        Self::new(keys)
    }

    /// Loads the key pool from environment variables (`ELEVENLABS_API_KEYS` or `ELEVENLABS_API_KEY`).
    #[must_use]
    pub fn from_env() -> Self {
        if let Ok(keys_str) = std::env::var("ELEVENLABS_API_KEYS") {
            let pool = Self::from_comma_separated(&keys_str);
            if !pool.is_empty() {
                return pool;
            }
        }

        if let Ok(single_key) = std::env::var("ELEVENLABS_API_KEY") {
            let trimmed = single_key.trim();
            if !trimmed.is_empty() {
                return Self::new(vec![trimmed.to_string()]);
            }
        }

        Self::new(Vec::new())
    }

    /// Returns the total number of keys in the pool.
    #[must_use]
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    /// Indicates whether the key pool contains zero keys.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// Obtains the currently active API key, if available and not exhausted.
    pub fn get_active_key(&self) -> Result<String, TtsError> {
        let total = self.keys.len();
        if total == 0 {
            return Err(TtsError::NoKeysConfigured);
        }

        let start = self.current_index.load(Ordering::SeqCst) % total;
        for offset in 0..total {
            let idx = (start + offset) % total;
            if !self.keys[idx].is_exhausted() {
                return Ok(self.keys[idx].key.clone());
            }
        }

        Err(TtsError::AllKeysExhausted(total))
    }

    /// Atomically rotates to the next available API key in the pool.
    ///
    /// The currently active key is marked as exhausted before rotating.
    pub fn mark_current_exhausted_and_rotate(&self) -> Result<String, TtsError> {
        let total = self.keys.len();
        if total == 0 {
            return Err(TtsError::NoKeysConfigured);
        }

        let old_idx = self.current_index.fetch_add(1, Ordering::SeqCst) % total;
        self.keys[old_idx].mark_exhausted();

        // Find the next available non-exhausted key
        for offset in 0..total {
            let idx = (old_idx + 1 + offset) % total;
            if !self.keys[idx].is_exhausted() {
                self.current_index.store(idx, Ordering::SeqCst);
                info!(
                    exhausted_key_idx = old_idx + 1,
                    new_key_idx = idx + 1,
                    total_keys = total,
                    "Rotated ElevenLabs key pool to next non-exhausted key"
                );
                return Ok(self.keys[idx].key.clone());
            }
        }

        warn!(
            total_keys = total,
            "All ElevenLabs API keys in pool have been exhausted"
        );
        Err(TtsError::AllKeysExhausted(total))
    }

    /// Resets all exhausted markers across the pool (e.g. after a cooldown period).
    pub fn reset_all(&self) {
        for entry in self.keys.iter() {
            entry.reset();
        }
        self.current_index.store(0, Ordering::SeqCst);
    }
}

/// Redacted string helper to guarantee no API keys are ever printed in plain text.
fn redact_key(key: &str) -> String {
    if key.len() <= 8 {
        "***REDACTED***".to_string()
    } else {
        format!("{}...{}", &key[..4], &key[key.len() - 4..])
    }
}

impl fmt::Debug for KeyPool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let total = self.keys.len();
        let active_idx = self.current_index.load(Ordering::Relaxed) % total.max(1);
        let redacted_keys: Vec<String> = self.keys.iter().map(|k| redact_key(&k.key)).collect();

        f.debug_struct("KeyPool")
            .field("total_keys", &total)
            .field("active_index", &active_idx)
            .field("keys", &redacted_keys)
            .finish()
    }
}

impl fmt::Display for KeyPool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "KeyPool(total: {}, active_index: {})",
            self.keys.len(),
            self.current_index.load(Ordering::Relaxed) % self.keys.len().max(1)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_comma_separated() {
        let pool = KeyPool::from_comma_separated(" key1,  key2 , key3 , ");
        assert_eq!(pool.len(), 3);
        assert_eq!(pool.get_active_key().unwrap(), "key1");
    }

    #[test]
    fn test_empty_pool() {
        let pool = KeyPool::new(vec![]);
        assert!(pool.is_empty());
        assert!(matches!(
            pool.get_active_key(),
            Err(TtsError::NoKeysConfigured)
        ));
    }

    #[test]
    fn test_atomic_rotation_and_exhaustion() {
        let pool = KeyPool::new(vec!["key_alpha".into(), "key_beta".into()]);
        assert_eq!(pool.get_active_key().unwrap(), "key_alpha");

        // Rotate once: alpha exhausted, beta active
        let next = pool.mark_current_exhausted_and_rotate().unwrap();
        assert_eq!(next, "key_beta");
        assert_eq!(pool.get_active_key().unwrap(), "key_beta");

        // Rotate second time: beta exhausted -> AllKeysExhausted
        let result = pool.mark_current_exhausted_and_rotate();
        assert!(matches!(result, Err(TtsError::AllKeysExhausted(2))));

        // Reset restores availability
        pool.reset_all();
        assert_eq!(pool.get_active_key().unwrap(), "key_alpha");
    }

    #[test]
    fn test_key_cooldown_rehabilitation() {
        let pool = KeyPool::new(vec!["key_temp_limited".into()]);
        assert_eq!(pool.get_active_key().unwrap(), "key_temp_limited");

        // Exhaust the key
        let _ = pool.mark_current_exhausted_and_rotate();
        assert!(matches!(
            pool.get_active_key(),
            Err(TtsError::AllKeysExhausted(1))
        ));

        // Simulate cooldown elapsed by setting exhausted_at to 65 seconds in the past
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        pool.keys[0]
            .exhausted_at
            .store(now.saturating_sub(65), Ordering::SeqCst);

        // Cooldown has expired: key should automatically rehabilitate!
        assert_eq!(pool.get_active_key().unwrap(), "key_temp_limited");
    }

    #[test]
    fn test_credential_redaction() {
        let pool = KeyPool::new(vec!["sk_live_123456789abcdef".into()]);
        let debug_str = format!("{pool:?}");
        assert!(!debug_str.contains("123456789"));
        assert!(debug_str.contains("sk_l...cdef"));

        let display_str = format!("{pool}");
        assert!(!display_str.contains("sk_live"));
    }
}
