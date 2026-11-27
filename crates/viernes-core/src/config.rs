//! Application configuration loaded from environment and .env files.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::error::ViernesError;

/// Resolves the per-user private runtime directory for Viernes sockets, status files, and logs.
///
/// Resolution order:
/// 1. `$XDG_RUNTIME_DIR/viernes` when `XDG_RUNTIME_DIR` is non-empty
/// 2. `$HOME/.cache/viernes/run` when `HOME` is non-empty
/// 3. `<temp_dir>/viernes-$USER` (falling back to `"user"` if `USER` is unset/empty)
#[must_use]
pub fn runtime_dir() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_RUNTIME_DIR") {
        let trimmed = xdg.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed).join("viernes");
        }
    }

    if let Ok(home) = std::env::var("HOME") {
        let trimmed = home.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed).join(".cache/viernes/run");
        }
    }

    let user = std::env::var("USER")
        .ok()
        .map(|u| u.trim().to_string())
        .filter(|u| !u.is_empty())
        .unwrap_or_else(|| "user".into());
    std::env::temp_dir().join(format!("viernes-{user}"))
}

/// Ensures `path` exists as a private directory (`0o700` on Unix) and rejects symlinks.
pub fn ensure_private_dir(path: &Path) -> std::io::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) => {
            if meta.file_type().is_symlink() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!(
                        "refusing to use symlink as private directory: {}",
                        path.display()
                    ),
                ));
            }
            if !meta.is_dir() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    format!("path exists and is not a directory: {}", path.display()),
                ));
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir_all(path)?;
            let meta = std::fs::symlink_metadata(path)?;
            if meta.file_type().is_symlink() || !meta.is_dir() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!(
                        "created path is a symlink or not a directory: {}",
                        path.display()
                    ),
                ));
            }
        }
        Err(err) => return Err(err),
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    }

    Ok(())
}

/// Returns the Push-to-Talk UNIX domain socket path.
///
/// Honors `VIERNES_PTT_SOCK` if non-empty, otherwise defaults to `runtime_dir().join("ptt.sock")`.
#[must_use]
pub fn default_ptt_socket_path() -> PathBuf {
    if let Ok(sock) = std::env::var("VIERNES_PTT_SOCK") {
        let trimmed = sock.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    runtime_dir().join("ptt.sock")
}

/// Returns the live daemon status snapshot file path.
///
/// Honors `VIERNES_STATUS_FILE` if non-empty, otherwise defaults to `runtime_dir().join("status.json")`.
#[must_use]
pub fn default_status_file_path() -> PathBuf {
    if let Ok(status_file) = std::env::var("VIERNES_STATUS_FILE") {
        let trimmed = status_file.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    runtime_dir().join("status.json")
}

/// Returns the default temporary audio scratch directory (`runtime_dir().join("audio")`).
#[must_use]
pub fn default_temp_dir_path() -> PathBuf {
    runtime_dir().join("audio")
}

/// Returns the default ALSA diagnostic log file path (`runtime_dir().join("alsa.log")`).
#[must_use]
pub fn default_alsa_log_path() -> PathBuf {
    runtime_dir().join("alsa.log")
}

/// Runtime settings governing all Viernes subsystem components.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ViernesConfig {
    /// Primary or active ElevenLabs API key.
    pub elevenlabs_api_key: String,

    /// Multi-account ElevenLabs key pool for automated quota failover.
    pub elevenlabs_api_keys: Vec<String>,

    /// Configured ElevenLabs Voice ID.
    pub elevenlabs_voice_id: String,

    /// Optional ElevenLabs TTS model identifier override.
    pub elevenlabs_model_id: Option<String>,

    /// Voice Activity Detection energy threshold (RMS).
    pub vad_threshold: f32,

    /// Silence duration in milliseconds before auto-terminating speech.
    pub silence_timeout_ms: u64,

    /// Optional audio hardware input device identifier.
    pub audio_input_device: Option<String>,

    /// Optional audio hardware output device identifier.
    pub audio_output_device: Option<String>,

    /// Scratch directory for temporary synthesis chunks.
    pub temp_dir: PathBuf,

    /// Whether automatic audio ducking (lowering music volume while Viernes speaks) is enabled.
    pub audio_ducking_enabled: bool,

    /// Target volume percent for background media during ducking (e.g. 40%).
    pub audio_ducking_volume_percent: u32,

    /// Playback volume percentage for Viernes assistant voice output (e.g. 130% for high clarity over background).
    pub viernes_volume_percent: u32,
}

impl Default for ViernesConfig {
    fn default() -> Self {
        Self {
            elevenlabs_api_key: String::new(),
            elevenlabs_api_keys: Vec::new(),
            elevenlabs_voice_id: "cgSgspJ2msm6clMCkdW9".into(), // Jessica
            elevenlabs_model_id: None,
            vad_threshold: 0.010,
            silence_timeout_ms: 800,
            audio_input_device: None,
            audio_output_device: None,
            temp_dir: default_temp_dir_path(),
            audio_ducking_enabled: true,
            audio_ducking_volume_percent: 40,
            viernes_volume_percent: 130,
        }
    }
}

impl ViernesConfig {
    /// Loads configuration values from an optional .env file path and process environment.
    pub fn load_from_env(env_path: Option<&Path>) -> Result<Self, ViernesError> {
        if let Some(p) = env_path {
            if !p.is_file() {
                return Err(ViernesError::Config(format!(
                    "Environment file does not exist or is not a file: {}",
                    p.display()
                )));
            }
            dotenvy::from_path(p).map_err(|e| {
                ViernesError::Config(format!("Failed to load {}: {e}", p.display()))
            })?;
        } else {
            if let Err(err) = dotenvy::dotenv() {
                if !err.not_found() {
                    warn!(error = %err, "Failed to parse .env file");
                }
            }
            if std::env::var("ELEVENLABS_API_KEYS").is_err()
                && std::env::var("ELEVENLABS_API_KEY").is_err()
            {
                if let Ok(home) = std::env::var("HOME") {
                    let trimmed_home = home.trim();
                    if !trimmed_home.is_empty() {
                        let config_env = PathBuf::from(trimmed_home).join(".config/viernes/.env");
                        if config_env.is_file() {
                            if let Err(err) = dotenvy::from_path(&config_env) {
                                warn!(
                                    path = %config_env.display(),
                                    error = %err,
                                    "Failed to parse .env file"
                                );
                            }
                        }
                    }
                }
            }
        }

        let mut config = Self::default();

        if let Ok(val) = std::env::var("ELEVENLABS_VOICE_ID") {
            if !val.trim().is_empty() {
                config.elevenlabs_voice_id = val.trim().to_string();
            }
        }

        if let Ok(val) = std::env::var("ELEVENLABS_MODEL_ID") {
            let trimmed = val.trim();
            if !trimmed.is_empty() {
                config.elevenlabs_model_id = Some(trimmed.to_string());
            }
        }

        // Multi-key pool resolution
        if let Ok(pool_str) = std::env::var("ELEVENLABS_API_KEYS") {
            let keys: Vec<String> = pool_str
                .replace('\n', ",")
                .split(',')
                .map(|k| k.trim().to_string())
                .filter(|k| !k.is_empty())
                .collect();
            if !keys.is_empty() {
                config.elevenlabs_api_key = keys[0].clone();
                config.elevenlabs_api_keys = keys;
            }
        }

        // Fallback to single key if pool is empty
        if config.elevenlabs_api_keys.is_empty() {
            if let Ok(single_key) = std::env::var("ELEVENLABS_API_KEY") {
                let key = single_key.trim().to_string();
                if !key.is_empty() {
                    config.elevenlabs_api_key = key.clone();
                    config.elevenlabs_api_keys = vec![key];
                }
            }
        }

        if let Ok(vad_str) = std::env::var("VAD_THRESHOLD") {
            let trimmed = vad_str.trim();
            if !trimmed.is_empty() {
                match trimmed.parse::<f32>() {
                    Ok(parsed) if parsed.is_finite() && parsed > 0.0 && parsed <= 1.0 => {
                        config.vad_threshold = parsed;
                    }
                    _ => {
                        warn!(
                            value = %vad_str,
                            "Invalid VAD_THRESHOLD (expected finite float in (0.0, 1.0]); keeping default"
                        );
                    }
                }
            }
        }

        if let Ok(silence_str) = std::env::var("SILENCE_TIMEOUT_MS") {
            let trimmed = silence_str.trim();
            if !trimmed.is_empty() {
                match trimmed.parse::<u64>() {
                    Ok(parsed) if (100..=30_000).contains(&parsed) => {
                        config.silence_timeout_ms = parsed;
                    }
                    _ => {
                        warn!(
                            value = %silence_str,
                            "Invalid SILENCE_TIMEOUT_MS (expected integer in 100..=30000); keeping default"
                        );
                    }
                }
            }
        }

        if let Ok(in_dev) = std::env::var("AUDIO_INPUT_DEVICE") {
            let trimmed = in_dev.trim();
            if !trimmed.is_empty() {
                config.audio_input_device = Some(trimmed.to_string());
            }
        }

        if let Ok(out_dev) = std::env::var("AUDIO_OUTPUT_DEVICE") {
            let trimmed = out_dev.trim();
            if !trimmed.is_empty() {
                config.audio_output_device = Some(trimmed.to_string());
            }
        }

        if let Ok(dir_str) = std::env::var("VIERNES_TEMP_DIR") {
            if !dir_str.trim().is_empty() {
                config.temp_dir = PathBuf::from(dir_str.trim());
            }
        }

        if let Ok(duck_str) = std::env::var("AUDIO_DUCKING_ENABLED") {
            let s = duck_str.trim().to_lowercase();
            if !s.is_empty() {
                match s.as_str() {
                    "1" | "true" | "yes" | "on" => config.audio_ducking_enabled = true,
                    "0" | "false" | "no" | "off" => config.audio_ducking_enabled = false,
                    _ => {
                        warn!(
                            value = %duck_str,
                            "Unrecognized AUDIO_DUCKING_ENABLED value; keeping default"
                        );
                    }
                }
            }
        }

        if let Ok(pct_str) = std::env::var("AUDIO_DUCKING_VOLUME_PERCENT") {
            let trimmed = pct_str.trim();
            if !trimmed.is_empty() {
                match trimmed.parse::<u32>() {
                    Ok(parsed) if parsed <= 100 => {
                        config.audio_ducking_volume_percent = parsed;
                    }
                    _ => {
                        warn!(
                            value = %pct_str,
                            "Invalid AUDIO_DUCKING_VOLUME_PERCENT (expected 0..=100); keeping default"
                        );
                    }
                }
            }
        }

        if let Ok(vol_str) = std::env::var("VIERNES_VOLUME_PERCENT") {
            let trimmed = vol_str.trim();
            if !trimmed.is_empty() {
                match trimmed.parse::<u32>() {
                    Ok(parsed) if parsed <= 200 => {
                        config.viernes_volume_percent = parsed;
                    }
                    _ => {
                        warn!(
                            value = %vol_str,
                            "Invalid VIERNES_VOLUME_PERCENT (expected 0..=200); keeping default"
                        );
                    }
                }
            }
        }

        Ok(config)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::Mutex;

    pub(crate) static ENV_LOCK: Mutex<()> = Mutex::new(());

    pub(crate) struct EnvGuard {
        saved: Vec<(&'static str, Option<String>)>,
    }

    impl EnvGuard {
        pub(crate) fn new(keys: &[&'static str]) -> Self {
            let saved = keys.iter().map(|&k| (k, std::env::var(k).ok())).collect();
            for &k in keys {
                std::env::remove_var(k);
            }
            Self { saved }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for (k, v) in self.saved.drain(..) {
                match v {
                    Some(val) => std::env::set_var(k, val),
                    None => std::env::remove_var(k),
                }
            }
        }
    }

    const ALL_TEST_ENV_KEYS: &[&str] = &[
        "XDG_RUNTIME_DIR",
        "HOME",
        "USER",
        "VIERNES_PTT_SOCK",
        "VIERNES_STATUS_FILE",
        "ELEVENLABS_API_KEY",
        "ELEVENLABS_API_KEYS",
        "ELEVENLABS_VOICE_ID",
        "ELEVENLABS_MODEL_ID",
        "VAD_THRESHOLD",
        "SILENCE_TIMEOUT_MS",
        "AUDIO_INPUT_DEVICE",
        "AUDIO_OUTPUT_DEVICE",
        "VIERNES_TEMP_DIR",
        "AUDIO_DUCKING_ENABLED",
        "AUDIO_DUCKING_VOLUME_PERCENT",
        "VIERNES_VOLUME_PERCENT",
    ];

    #[test]
    fn test_default_config_values() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _guard = EnvGuard::new(ALL_TEST_ENV_KEYS);
        std::env::set_var("XDG_RUNTIME_DIR", "/tmp/viernes-test-xdg");

        let cfg = ViernesConfig::default();
        assert_eq!(cfg.elevenlabs_voice_id, "cgSgspJ2msm6clMCkdW9");
        assert_eq!(cfg.elevenlabs_model_id, None);
        assert!((cfg.vad_threshold - 0.010).abs() < f32::EPSILON);
        assert_eq!(cfg.silence_timeout_ms, 800);
        assert_eq!(
            cfg.temp_dir,
            PathBuf::from("/tmp/viernes-test-xdg/viernes/audio")
        );
        assert!(cfg.audio_ducking_enabled);
        assert_eq!(cfg.audio_ducking_volume_percent, 40);
        assert_eq!(cfg.viernes_volume_percent, 130);
    }

    #[test]
    fn test_runtime_dir_and_path_helpers() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _guard = EnvGuard::new(ALL_TEST_ENV_KEYS);

        // 1. XDG_RUNTIME_DIR takes precedence
        std::env::set_var("XDG_RUNTIME_DIR", "/run/user/1000");
        std::env::set_var("HOME", "/home/alice");
        assert_eq!(runtime_dir(), PathBuf::from("/run/user/1000/viernes"));
        assert_eq!(
            default_ptt_socket_path(),
            PathBuf::from("/run/user/1000/viernes/ptt.sock")
        );
        assert_eq!(
            default_status_file_path(),
            PathBuf::from("/run/user/1000/viernes/status.json")
        );
        assert_eq!(
            default_temp_dir_path(),
            PathBuf::from("/run/user/1000/viernes/audio")
        );
        assert_eq!(
            default_alsa_log_path(),
            PathBuf::from("/run/user/1000/viernes/alsa.log")
        );

        // 2. Empty XDG_RUNTIME_DIR falls back to HOME/.cache/viernes/run
        std::env::set_var("XDG_RUNTIME_DIR", "   ");
        assert_eq!(
            runtime_dir(),
            PathBuf::from("/home/alice/.cache/viernes/run")
        );

        // 3. Unset HOME falls back to temp_dir()/viernes-$USER
        std::env::remove_var("XDG_RUNTIME_DIR");
        std::env::remove_var("HOME");
        std::env::set_var("USER", "bob");
        assert_eq!(runtime_dir(), std::env::temp_dir().join("viernes-bob"));

        // 4. VIERNES_* env overrides
        std::env::set_var("VIERNES_PTT_SOCK", "/custom/ptt.sock");
        std::env::set_var("VIERNES_STATUS_FILE", "/custom/status.json");
        assert_eq!(default_ptt_socket_path(), PathBuf::from("/custom/ptt.sock"));
        assert_eq!(
            default_status_file_path(),
            PathBuf::from("/custom/status.json")
        );
    }

    #[test]
    fn test_load_from_env_missing_explicit_path_errors() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _guard = EnvGuard::new(ALL_TEST_ENV_KEYS);

        let missing =
            std::env::temp_dir().join(format!("viernes-missing-env-{}", uuid::Uuid::new_v4()));
        let err = ViernesConfig::load_from_env(Some(&missing)).unwrap_err();
        assert!(matches!(err, ViernesError::Config(_)));
    }

    #[test]
    fn test_load_from_env_validates_numeric_and_model_overrides() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _guard = EnvGuard::new(ALL_TEST_ENV_KEYS);

        // Invalid overrides should fall back to defaults
        std::env::set_var("VAD_THRESHOLD", "NaN");
        std::env::set_var("SILENCE_TIMEOUT_MS", "50");
        std::env::set_var("AUDIO_DUCKING_VOLUME_PERCENT", "150");
        std::env::set_var("VIERNES_VOLUME_PERCENT", "250");
        std::env::set_var("AUDIO_DUCKING_ENABLED", "maybe");
        std::env::set_var("ELEVENLABS_MODEL_ID", "   ");

        let cfg = ViernesConfig::load_from_env(None).unwrap();
        assert!((cfg.vad_threshold - 0.010).abs() < f32::EPSILON);
        assert_eq!(cfg.silence_timeout_ms, 800);
        assert_eq!(cfg.audio_ducking_volume_percent, 40);
        assert_eq!(cfg.viernes_volume_percent, 130);
        assert!(cfg.audio_ducking_enabled);
        assert_eq!(cfg.elevenlabs_model_id, None);

        // Out-of-range VAD values (0.0, -0.2, 1.5, inf)
        for bad in ["0.0", "-0.2", "1.5", "inf"] {
            std::env::set_var("VAD_THRESHOLD", bad);
            let c = ViernesConfig::load_from_env(None).unwrap();
            assert!((c.vad_threshold - 0.010).abs() < f32::EPSILON);
        }

        // Valid overrides
        std::env::set_var("VAD_THRESHOLD", "0.25");
        std::env::set_var("SILENCE_TIMEOUT_MS", "1500");
        std::env::set_var("AUDIO_DUCKING_VOLUME_PERCENT", "25");
        std::env::set_var("VIERNES_VOLUME_PERCENT", "180");
        std::env::set_var("AUDIO_DUCKING_ENABLED", "false");
        std::env::set_var("ELEVENLABS_MODEL_ID", "  eleven_turbo_v2_5 ");

        let valid_cfg = ViernesConfig::load_from_env(None).unwrap();
        assert!((valid_cfg.vad_threshold - 0.25).abs() < f32::EPSILON);
        assert_eq!(valid_cfg.silence_timeout_ms, 1500);
        assert_eq!(valid_cfg.audio_ducking_volume_percent, 25);
        assert_eq!(valid_cfg.viernes_volume_percent, 180);
        assert!(!valid_cfg.audio_ducking_enabled);
        assert_eq!(
            valid_cfg.elevenlabs_model_id.as_deref(),
            Some("eleven_turbo_v2_5")
        );
    }

    #[test]
    fn test_load_from_env_explicit_file() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _guard = EnvGuard::new(ALL_TEST_ENV_KEYS);

        let dir = std::env::temp_dir().join(format!("viernes-env-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let env_file = dir.join("test.env");
        std::fs::write(
            &env_file,
            "ELEVENLABS_MODEL_ID=eleven_flash_v2_5\nVAD_THRESHOLD=0.05\n",
        )
        .unwrap();

        let cfg = ViernesConfig::load_from_env(Some(&env_file)).unwrap();
        assert_eq!(
            cfg.elevenlabs_model_id.as_deref(),
            Some("eleven_flash_v2_5")
        );
        assert!((cfg.vad_threshold - 0.05).abs() < f32::EPSILON);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn test_ensure_private_dir_permissions_and_symlink_rejection() {
        use std::os::unix::fs::PermissionsExt;

        let base = std::env::temp_dir().join(format!("viernes-priv-test-{}", uuid::Uuid::new_v4()));
        let target_dir = base.join("private");
        ensure_private_dir(&target_dir).unwrap();

        let mode = std::fs::metadata(&target_dir).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);

        // Symlink must be rejected
        let link_path = base.join("symlink_dir");
        std::os::unix::fs::symlink(&target_dir, &link_path).unwrap();
        let err = ensure_private_dir(&link_path).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);

        let _ = std::fs::remove_dir_all(&base);
    }
}
