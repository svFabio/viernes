//! Multi-account API key pool with atomic lock-free rotation for ElevenLabs.

use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::SystemTime;

use tracing::{info, warn};

use crate::error::TtsError;

/// Default cooldown duration (60 seconds) before an exhausted key is automatically rehabilitated.
pub const KEY_COOLDOWN_SECS: u64 = 60;

/// An active API key paired with its slot index in the pool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveKey {
    /// Zero-based index of this key inside [`KeyPool`].
    pub index: usize,
    /// Raw API key string.
    pub key: String,
}

/// An individual API key entry with an exhausted marker and cooldown timestamp.
struct KeyEntry {
    key: String,
    exhausted: AtomicBool,
    permanently_exhausted: AtomicBool,
    exhausted_at: AtomicU64,
}

impl KeyEntry {
    fn new(key: String) -> Self {
        Self {
            key,
            exhausted: AtomicBool::new(false),
            permanently_exhausted: AtomicBool::new(false),
            exhausted_at: AtomicU64::new(0),
        }
    }

    fn is_exhausted(&self) -> bool {
        if self.permanently_exhausted.load(Ordering::SeqCst) {
            return true;
        }

        if !self.exhausted.load(Ordering::Relaxed) {
            return false;
        }

        let ts = self.exhausted_at.load(Ordering::Relaxed);
        if ts > 0 {
            let now = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();

            if now.saturating_sub(ts) >= KEY_COOLDOWN_SECS
                && !self.permanently_exhausted.load(Ordering::SeqCst)
            {
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

    fn mark_exhausted(&self, permanent: bool) {
        if permanent {
            self.permanently_exhausted.store(true, Ordering::SeqCst);
            self.exhausted_at.store(0, Ordering::SeqCst);
            self.exhausted.store(true, Ordering::SeqCst);
        } else {
            let now = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            self.exhausted_at.store(now, Ordering::SeqCst);
            self.exhausted.store(true, Ordering::SeqCst);
        }
    }

    fn reset(&self) {
        self.permanently_exhausted.store(false, Ordering::SeqCst);
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

    /// Obtains the currently active API key and its index, if available and not exhausted.
    #[must_use]
    pub fn get_active_key_with_index(&self) -> Option<ActiveKey> {
        let total = self.keys.len();
        if total == 0 {
            return None;
        }

        let start = self.current_index.load(Ordering::SeqCst) % total;
        for offset in 0..total {
            let idx = (start + offset) % total;
            if !self.keys[idx].is_exhausted() {
                return Some(ActiveKey {
                    index: idx,
                    key: self.keys[idx].key.clone(),
                });
            }
        }

        None
    }

    /// Marks the key at `index` as exhausted (permanently on `401`, or with cooldown on `429`),
    /// advancing the pool pointer only if it still points at `index`.
    pub fn mark_exhausted_at(&self, index: usize, permanent: bool) {
        let total = self.keys.len();
        if index >= total {
            return;
        }

        self.keys[index].mark_exhausted(permanent);
        if permanent {
            warn!(
                key_idx = index + 1,
                key = %redact_key(&self.keys[index].key),
                "ElevenLabs API key permanently disabled (unauthorized)"
            );
        } else {
            warn!(
                key_idx = index + 1,
                key = %redact_key(&self.keys[index].key),
                cooldown_secs = KEY_COOLDOWN_SECS,
                "ElevenLabs API key placed on temporary cooldown"
            );
        }

        let mut next_idx = (index + 1) % total;
        for offset in 0..total {
            let candidate = (index + 1 + offset) % total;
            if !self.keys[candidate].is_exhausted() {
                next_idx = candidate;
                break;
            }
        }

        let current = self.current_index.load(Ordering::SeqCst);
        if current % total == index {
            let _ = self.current_index.compare_exchange(
                current,
                next_idx,
                Ordering::SeqCst,
                Ordering::Relaxed,
            );
        }
    }

    /// Obtains the currently active API key, if available and not exhausted.
    pub fn get_active_key(&self) -> Result<String, TtsError> {
        let total = self.keys.len();
        if total == 0 {
            return Err(TtsError::NoKeysConfigured);
        }

        self.get_active_key_with_index()
            .map(|active| active.key)
            .ok_or(TtsError::AllKeysExhausted(total))
    }

    /// Atomically rotates to the next available API key in the pool.
    ///
    /// The currently active key is marked as temporarily exhausted before rotating.
    pub fn mark_current_exhausted_and_rotate(&self) -> Result<String, TtsError> {
        let total = self.keys.len();
        if total == 0 {
            return Err(TtsError::NoKeysConfigured);
        }

        let old_idx = self.current_index.fetch_add(1, Ordering::SeqCst) % total;
        self.keys[old_idx].mark_exhausted(false);

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
    let char_count = key.chars().count();
    if char_count <= 8 {
        "***REDACTED***".to_string()
    } else {
        let prefix: String = key.chars().take(4).collect();
        let suffix: String = key.chars().skip(char_count - 4).collect();
        format!("{prefix}...{suffix}")
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

    #[test]
    fn test_utf8_redaction_does_not_panic() {
        assert_eq!(redact_key("áéíóúñ"), "***REDACTED***");
        assert_eq!(redact_key("áéíóúñ123456"), "áéíó...3456");
    }

    #[test]
    fn test_mark_exhausted_at_avoids_wrong_key_race() {
        let pool = KeyPool::new(vec!["key_zero".into(), "key_one".into(), "key_two".into()]);
        let first = pool.get_active_key_with_index().unwrap();
        let concurrent = pool.get_active_key_with_index().unwrap();
        assert_eq!(first.index, 0);
        assert_eq!(concurrent.index, 0);

        // First caller exhausts slot 0 -> advances pointer to slot 1
        pool.mark_exhausted_at(first.index, false);
        let active_now = pool.get_active_key_with_index().unwrap();
        assert_eq!(active_now.index, 1);
        assert_eq!(active_now.key, "key_one");

        // Second concurrent caller finishes late for slot 0 -> must NOT exhaust slot 1
        pool.mark_exhausted_at(concurrent.index, false);
        let still_active = pool.get_active_key_with_index().unwrap();
        assert_eq!(still_active.index, 1);
        assert_eq!(still_active.key, "key_one");
    }

    #[test]
    fn test_permanent_exhaustion_never_rehabilitates() {
        let pool = KeyPool::new(vec!["key_invalid_401".into()]);
        let active = pool.get_active_key_with_index().unwrap();
        pool.mark_exhausted_at(active.index, true);

        // Even if exhausted_at looks old, a permanently exhausted key stays disabled
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        pool.keys[0]
            .exhausted_at
            .store(now.saturating_sub(120), Ordering::SeqCst);

        assert!(pool.get_active_key_with_index().is_none());
        assert!(matches!(
            pool.get_active_key(),
            Err(TtsError::AllKeysExhausted(1))
        ));

        // Explicit reset_all clears permanent exhaustion
        pool.reset_all();
        assert_eq!(pool.get_active_key().unwrap(), "key_invalid_401");
    }
}
