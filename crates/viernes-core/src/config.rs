//! Application configuration loaded from environment and .env files.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::ViernesError;

/// Runtime settings governing all Viernes subsystem components.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ViernesConfig {
    /// Primary or active ElevenLabs API key.
    pub elevenlabs_api_key: String,

    /// Multi-account ElevenLabs key pool for automated quota failover.
    pub elevenlabs_api_keys: Vec<String>,

    /// Configured ElevenLabs Voice ID.
    pub elevenlabs_voice_id: String,

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
}

impl Default for ViernesConfig {
    fn default() -> Self {
        Self {
            elevenlabs_api_key: String::new(),
            elevenlabs_api_keys: Vec::new(),
            elevenlabs_voice_id: "cgSgspJ2msm6clMCkdW9".into(), // Jessica
            vad_threshold: 0.010,
            silence_timeout_ms: 800,
            audio_input_device: None,
            audio_output_device: None,
            temp_dir: PathBuf::from("/tmp/viernes"),
        }
    }
}

impl ViernesConfig {
    /// Loads configuration values from an optional .env file path and process environment.
    pub fn load_from_env(env_path: Option<&Path>) -> Result<Self, ViernesError> {
        if let Some(p) = env_path {
            if p.is_file() {
                dotenvy::from_path(p).map_err(|e| ViernesError::Config(e.to_string()))?;
            }
        } else {
            let _ = dotenvy::dotenv();
        }

        let mut config = Self::default();

        if let Ok(val) = std::env::var("ELEVENLABS_VOICE_ID") {
            if !val.trim().is_empty() {
                config.elevenlabs_voice_id = val.trim().to_string();
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
            if let Ok(parsed) = vad_str.trim().parse::<f32>() {
                config.vad_threshold = parsed;
            }
        }

        if let Ok(silence_str) = std::env::var("SILENCE_TIMEOUT_MS") {
            if let Ok(parsed) = silence_str.trim().parse::<u64>() {
                config.silence_timeout_ms = parsed;
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

        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_values() {
        let cfg = ViernesConfig::default();
        assert_eq!(cfg.elevenlabs_voice_id, "cgSgspJ2msm6clMCkdW9");
        assert!((cfg.vad_threshold - 0.010).abs() < f32::EPSILON);
        assert_eq!(cfg.silence_timeout_ms, 800);
        assert_eq!(cfg.temp_dir, PathBuf::from("/tmp/viernes"));
    }
}
