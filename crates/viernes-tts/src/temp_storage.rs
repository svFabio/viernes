//! Temporary storage cache manager with retention policies, quota eviction, and shutdown cleanup.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use tracing::{debug, info, warn};
use uuid::Uuid;
use viernes_core::error::ViernesError;

const DEFAULT_TEMP_DIR: &str = "/tmp/viernes";
const DEFAULT_TTL: Duration = Duration::from_secs(2 * 60 * 60); // 2 hours
const DEFAULT_MAX_QUOTA_BYTES: u64 = 100 * 1024 * 1024; // 100 MB
const DEFAULT_TARGET_QUOTA_BYTES: u64 = 70 * 1024 * 1024; // 70 MB target after purge

/// Metadata for a managed temporary audio file.
struct FileMeta {
    path: PathBuf,
    size: u64,
    modified: SystemTime,
}

/// Thread-safe temporary storage manager ensuring safe disk usage and user privacy.
#[derive(Debug)]
pub struct TempStorageManager {
    dir: PathBuf,
    ttl: Duration,
    max_quota_bytes: u64,
    target_quota_bytes: u64,
}

impl Default for TempStorageManager {
    fn default() -> Self {
        Self::new(PathBuf::from(DEFAULT_TEMP_DIR))
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
        };
        let _ = manager.ensure_directory_secure();
        manager
    }

    /// Ensures the temporary storage directory exists with restricted `0700` user permissions.
    pub fn ensure_directory_secure(&self) -> Result<(), ViernesError> {
        if !self.dir.exists() {
            fs::create_dir_all(&self.dir).map_err(ViernesError::Io)?;
        }

        // Set POSIX 0700 permissions (read/write/exec only for owner)
        let mut permissions = fs::metadata(&self.dir)
            .map_err(ViernesError::Io)?
            .permissions();

        permissions.set_mode(0o700);
        fs::set_permissions(&self.dir, permissions).map_err(ViernesError::Io)?;

        debug!(path = %self.dir.display(), "Temporary directory secured with 0700 permissions");
        Ok(())
    }

    /// Writes temporary audio bytes to a securely generated file inside the temp folder.
    pub fn write_temp_audio(&self, prefix: &str, bytes: &[u8]) -> Result<PathBuf, ViernesError> {
        self.ensure_directory_secure()?;

        let file_name = format!(
            "{}_{}_{}.mp3",
            prefix,
            std::process::id(),
            Uuid::new_v4().simple()
        );
        let file_path = self.dir.join(file_name);

        fs::write(&file_path, bytes).map_err(ViernesError::Io)?;

        // Run quota check after write
        let _ = self.purge_excess_and_expired();

        Ok(file_path)
    }

    /// Scans the directory and purges files older than the configured TTL or exceeding quota.
    pub fn purge_excess_and_expired(&self) -> Result<usize, ViernesError> {
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
            if path.is_file() {
                if let Ok(meta) = entry.metadata() {
                    let size = meta.len();
                    let modified = meta.modified().unwrap_or(now);

                    // Check TTL expiration
                    if let Ok(age) = now.duration_since(modified) {
                        if age > self.ttl {
                            debug!(path = %path.display(), age_secs = age.as_secs(), "Purging expired audio file");
                            let _ = fs::remove_file(&path);
                            purged_count += 1;
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
            }
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
        if !self.dir.exists() {
            return Ok(0);
        }

        let mut count = 0;
        let entries = fs::read_dir(&self.dir).map_err(ViernesError::Io)?;
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_file() && fs::remove_file(&p).is_ok() {
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
        let _ = self.purge_excess_and_expired();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

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

        let perms = fs::metadata(&target_dir).unwrap().permissions();
        assert_eq!(perms.mode() & 0o777, 0o700);
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
