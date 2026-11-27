//! Temporary storage cache manager with retention policies, quota eviction, and shutdown cleanup.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use tracing::{debug, info, warn};
use uuid::Uuid;
use viernes_core::error::ViernesError;

const DEFAULT_TTL: Duration = Duration::from_secs(2 * 60 * 60); // 2 hours
const DEFAULT_MAX_QUOTA_BYTES: u64 = 100 * 1024 * 1024; // 100 MB
const DEFAULT_TARGET_QUOTA_BYTES: u64 = 70 * 1024 * 1024; // 70 MB target after purge

/// Metadata for a managed temporary audio file.
struct FileMeta {
    path: PathBuf,
    size: u64,
    modified: SystemTime,
}

/// Sanitizes a temporary audio filename prefix to prevent path traversal.
fn sanitize_prefix(prefix: &str) -> String {
    let clean: String = prefix
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect();
    if clean.is_empty() {
        "audio".to_string()
    } else {
        clean
    }
}

/// Thread-safe temporary storage manager ensuring safe disk usage and user privacy.
#[derive(Debug)]
pub struct TempStorageManager {
    dir: PathBuf,
    ttl: Duration,
    max_quota_bytes: u64,
    target_quota_bytes: u64,
    lock: Mutex<()>,
}

impl Default for TempStorageManager {
    fn default() -> Self {
        Self::new(viernes_core::default_temp_dir_path())
    }
}

impl TempStorageManager {
    /// Constructs a `TempStorageManager` for the specified directory path.
    pub fn new(dir: PathBuf) -> Self {
        Self::with_policies(
            dir,
            DEFAULT_TTL,
            DEFAULT_MAX_QUOTA_BYTES,
            DEFAULT_TARGET_QUOTA_BYTES,
        )
    }

    /// Constructs a manager with explicit TTL and disk quota limits.
    pub fn with_policies(
        dir: PathBuf,
        ttl: Duration,
        max_quota_bytes: u64,
        target_quota_bytes: u64,
    ) -> Self {
        let manager = Self {
            dir,
            ttl,
            max_quota_bytes,
            target_quota_bytes,
            lock: Mutex::new(()),
        };
        if let Err(err) = manager.ensure_directory_secure() {
            warn!(
                path = %manager.dir.display(),
                error = %err,
                "Failed to initialize private temporary audio directory"
            );
        }
        manager
    }

    /// Ensures the temporary storage directory exists with restricted `0700` user permissions.
    pub fn ensure_directory_secure(&self) -> Result<(), ViernesError> {
        viernes_core::ensure_private_dir(&self.dir).map_err(ViernesError::Io)?;
        debug!(path = %self.dir.display(), "Temporary directory secured with 0700 permissions");
        Ok(())
    }

    /// Writes temporary audio bytes to a securely generated file (`0600` on Unix) inside the temp folder.
    pub fn write_temp_audio(&self, prefix: &str, bytes: &[u8]) -> Result<PathBuf, ViernesError> {
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        self.ensure_directory_secure()?;

        let safe_prefix = sanitize_prefix(prefix);
        let file_name = format!(
            "{}_{}_{}.mp3",
            safe_prefix,
            std::process::id(),
            Uuid::new_v4().simple()
        );
        let file_path = self.dir.join(file_name);

        #[cfg(unix)]
        {
            use std::io::Write;
            use std::os::unix::fs::OpenOptionsExt;
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&file_path)
                .map_err(ViernesError::Io)?;
            file.write_all(bytes).map_err(ViernesError::Io)?;
        }
        #[cfg(not(unix))]
        {
            fs::write(&file_path, bytes).map_err(ViernesError::Io)?;
        }

        // Run quota check after write while holding lock
        if let Err(err) = self.purge_excess_and_expired_locked() {
            warn!(
                path = %self.dir.display(),
                error = %err,
                "Failed to purge expired or excess temporary audio files after write"
            );
        }

        Ok(file_path)
    }

    /// Scans the directory and purges files older than the configured TTL or exceeding quota.
    pub fn purge_excess_and_expired(&self) -> Result<usize, ViernesError> {
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        self.purge_excess_and_expired_locked()
    }

    fn purge_excess_and_expired_locked(&self) -> Result<usize, ViernesError> {
        if !self.dir.exists() {
            return Ok(0);
        }

        let now = SystemTime::now();
        let mut files: Vec<FileMeta> = Vec::new();
        let mut total_size: u64 = 0;
        let mut purged_count = 0;

        let entries = fs::read_dir(&self.dir).map_err(ViernesError::Io)?;

        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = fs::symlink_metadata(&path) else {
                continue;
            };
            if meta.file_type().is_symlink() || !meta.is_file() {
                continue;
            }

            let size = meta.len();
            let modified = meta.modified().unwrap_or(now);

            // Check TTL expiration
            if let Ok(age) = now.duration_since(modified) {
                if age > self.ttl {
                    debug!(path = %path.display(), age_secs = age.as_secs(), "Purging expired audio file");
                    if let Err(err) = fs::remove_file(&path) {
                        warn!(path = %path.display(), error = %err, "Failed to remove expired audio file");
                    } else {
                        purged_count += 1;
                    }
                    continue;
                }
            }

            total_size += size;
            files.push(FileMeta {
                path,
                size,
                modified,
            });
        }

        // Check disk quota threshold
        if total_size > self.max_quota_bytes {
            warn!(
                total_mb = total_size / (1024 * 1024),
                max_mb = self.max_quota_bytes / (1024 * 1024),
                "Temporary storage quota exceeded; evicting oldest files"
            );

            // Sort files oldest first
            files.sort_by_key(|f| f.modified);

            for file in files {
                if total_size <= self.target_quota_bytes {
                    break;
                }
                debug!(path = %file.path.display(), size = file.size, "Purging LRU audio file for quota");
                if fs::remove_file(&file.path).is_ok() {
                    total_size = total_size.saturating_sub(file.size);
                    purged_count += 1;
                }
            }
        }

        Ok(purged_count)
    }

    /// Cleans all temporary audio fragments upon system shutdown.
    pub fn clean_all(&self) -> Result<usize, ViernesError> {
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        self.clean_all_locked()
    }

    fn clean_all_locked(&self) -> Result<usize, ViernesError> {
        if !self.dir.exists() {
            return Ok(0);
        }

        let mut count = 0;
        let entries = fs::read_dir(&self.dir).map_err(ViernesError::Io)?;
        for entry in entries.flatten() {
            let p = entry.path();
            let Ok(meta) = fs::symlink_metadata(&p) else {
                continue;
            };
            if meta.file_type().is_symlink() || !meta.is_file() {
                continue;
            }
            if fs::remove_file(&p).is_ok() {
                count += 1;
            }
        }

        info!(
            count = count,
            "Shutdown cleanup purged temporary audio files"
        );
        Ok(count)
    }

    /// Returns the directory path.
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }
}

impl Drop for TempStorageManager {
    fn drop(&mut self) {
        if let Err(err) = self.clean_all() {
            warn!(
                path = %self.dir.display(),
                error = %err,
                "Failed to clean temporary audio files on drop"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    use tempfile::tempdir;

    #[test]
    fn test_default_uses_core_temp_dir_path() {
        assert_eq!(
            TempStorageManager::default().dir(),
            viernes_core::default_temp_dir_path()
        );
    }

    #[test]
    fn test_secure_directory_creation_and_write() {
        let tmp = tempdir().unwrap();
        let target_dir = tmp.path().join("viernes_test");

        let manager = TempStorageManager::new(target_dir.clone());
        assert!(target_dir.exists());

        let path = manager
            .write_temp_audio("test_audio", b"mock mp3 bytes")
            .unwrap();
        assert!(path.exists());
        assert!(fs::read(&path).unwrap() == b"mock mp3 bytes");

        #[cfg(unix)]
        {
            let dir_perms = fs::metadata(&target_dir).unwrap().permissions();
            assert_eq!(dir_perms.mode() & 0o777, 0o700);
            let file_perms = fs::metadata(&path).unwrap().permissions();
            assert_eq!(file_perms.mode() & 0o777, 0o600);
        }
    }

    #[test]
    fn test_prefix_sanitization_prevents_path_traversal() {
        let tmp = tempdir().unwrap();
        let target_dir = tmp.path().join("viernes_sanitize");
        let manager = TempStorageManager::new(target_dir.clone());

        let p1 = manager.write_temp_audio("../../etc/passwd", b"x").unwrap();
        assert_eq!(p1.parent().unwrap(), target_dir.as_path());
        assert!(p1
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("etcpasswd_"));

        let p2 = manager.write_temp_audio("../..", b"y").unwrap();
        assert_eq!(p2.parent().unwrap(), target_dir.as_path());
        assert!(p2
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("audio_"));
    }

    #[cfg(unix)]
    #[test]
    fn test_symlinks_are_skipped_during_purge_and_clean_all() {
        let tmp = tempdir().unwrap();
        let outside_file = tmp.path().join("outside.txt");
        fs::write(&outside_file, b"keep me").unwrap();

        let target_dir = tmp.path().join("viernes_symlink_test");
        let manager =
            TempStorageManager::with_policies(target_dir.clone(), Duration::from_secs(0), 10, 5);

        let link_path = target_dir.join("sneaky.mp3");
        std::os::unix::fs::symlink(&outside_file, &link_path).unwrap();

        let real_file = manager.write_temp_audio("real", b"123456789012").unwrap();
        assert!(!real_file.exists() || manager.purge_excess_and_expired().is_ok());

        let removed = manager.clean_all().unwrap();
        assert_eq!(removed, 0);
        assert!(fs::symlink_metadata(&link_path).is_ok());
        assert_eq!(fs::read(&outside_file).unwrap(), b"keep me");
    }

    #[test]
    fn test_drop_cleans_all_files() {
        let tmp = tempdir().unwrap();
        let target_dir = tmp.path().join("viernes_drop_test");
        let file_path = {
            let manager = TempStorageManager::new(target_dir);
            let p = manager.write_temp_audio("f1", b"123").unwrap();
            assert!(p.exists());
            p
        };
        assert!(!file_path.exists());
    }

    #[test]
    fn test_quota_eviction() {
        let tmp = tempdir().unwrap();
        let target_dir = tmp.path().join("viernes_quota_test");

        // Small 1KB max quota, 700 bytes target (so deleting 1 600-byte file brings total to 600 <= 700)
        let manager = TempStorageManager::with_policies(
            target_dir.clone(),
            Duration::from_secs(3600),
            1000,
            700,
        );

        let p1 = manager.write_temp_audio("f1", &[0u8; 600]).unwrap();
        std::thread::sleep(Duration::from_millis(10));
        let p2 = manager.write_temp_audio("f2", &[0u8; 600]).unwrap();

        // One file must have been evicted to respect target quota
        let purged = manager.purge_excess_and_expired().unwrap();
        assert!(purged > 0 || !p1.exists());
        assert!(p2.exists());
    }

    #[test]
    fn test_clean_all_on_shutdown() {
        let tmp = tempdir().unwrap();
        let target_dir = tmp.path().join("viernes_clean_test");
        let manager = TempStorageManager::new(target_dir);

        let _ = manager.write_temp_audio("f1", b"123").unwrap();
        let _ = manager.write_temp_audio("f2", b"456").unwrap();

        let removed = manager.clean_all().unwrap();
        assert_eq!(removed, 2);
    }
}
