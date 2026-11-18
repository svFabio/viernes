//! Automatic audio ducking controller for media playback during voice interactions.
//!
//! Queries PipeWire / PulseAudio via `pactl -f json list sink-inputs` to identify external
//! media streams (e.g. Spotify, browser playback, MPD, VLC), lowers their volume during active
//! voice turns (PTT speech capture, filler playback, and assistant speaking), and deterministically
//! restores them to their exact original levels upon returning to Idle.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

use serde::Deserialize;
use tokio::sync::Mutex;
use tracing::{debug, info};
use viernes_core::config::ViernesConfig;

/// PulseAudio nominal 100% volume integer reference (0x10000 = 65536).
pub const PULSE_VOLUME_NORM: u32 = 65536;

/// Record of an actively ducked audio sink input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuckedStream {
    /// Sink input numerical identifier (e.g. 1428).
    pub index: u32,
    /// Original raw volume integer before ducking was applied.
    pub original_value: u32,
}

#[derive(Debug, Deserialize)]
struct SinkInputJson {
    index: u32,
    volume: Option<VolumeMapJson>,
    properties: Option<PropertiesJson>,
}

#[derive(Debug, Deserialize)]
struct VolumeMapJson {
    #[serde(rename = "front-left")]
    front_left: Option<ChannelVolumeJson>,
}

#[derive(Debug, Deserialize)]
struct ChannelVolumeJson {
    value: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct PropertiesJson {
    #[serde(rename = "application.name")]
    application_name: Option<String>,
    #[serde(rename = "node.name")]
    node_name: Option<String>,
}

/// Identifies whether a sink input stream belongs to Viernes itself.
#[must_use]
pub fn is_viernes_stream(app_name: Option<&str>, node_name: Option<&str>) -> bool {
    let check = |s: &str| s.to_lowercase().contains("viernes");
    app_name.is_some_and(check) || node_name.is_some_and(check)
}

/// Parses sink-inputs JSON output and extracts eligible non-Viernes streams for ducking.
#[must_use]
pub fn parse_duckable_streams(json_str: &str) -> Vec<DuckedStream> {
    let parsed: Vec<SinkInputJson> = match serde_json::from_str(json_str) {
        Ok(list) => list,
        Err(e) => {
            debug!(error = %e, "Failed parsing pactl sink-inputs JSON or empty stream list");
            return Vec::new();
        }
    };

    parsed
        .into_iter()
        .filter_map(|input| {
            let app_name = input
                .properties
                .as_ref()
                .and_then(|p| p.application_name.as_deref());
            let node_name = input
                .properties
                .as_ref()
                .and_then(|p| p.node_name.as_deref());

            if is_viernes_stream(app_name, node_name) {
                return None;
            }

            let val = input
                .volume
                .and_then(|v| v.front_left)
                .and_then(|c| c.value)
                .unwrap_or(PULSE_VOLUME_NORM);

            Some(DuckedStream {
                index: input.index,
                original_value: val,
            })
        })
        .collect()
}

/// Manages reactive background media attenuation (Audio Ducking) for Viernes.
#[derive(Debug)]
pub struct AudioDucker {
    enabled: AtomicBool,
    target_volume_percent: AtomicU32,
    ducked_streams: Arc<Mutex<Vec<DuckedStream>>>,
    is_ducked: Arc<AtomicBool>,
}

impl Default for AudioDucker {
    fn default() -> Self {
        Self::new(true, 20)
    }
}

impl AudioDucker {
    /// Creates a new `AudioDucker` with specified enabled toggle and target percentage (e.g. 20%).
    #[must_use]
    pub fn new(enabled: bool, target_volume_percent: u32) -> Self {
        Self {
            enabled: AtomicBool::new(enabled),
            target_volume_percent: AtomicU32::new(target_volume_percent.min(100)),
            ducked_streams: Arc::new(Mutex::new(Vec::new())),
            is_ducked: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Initializes `AudioDucker` from the active [`ViernesConfig`].
    #[must_use]
    pub fn from_config(config: &ViernesConfig) -> Self {
        Self::new(
            config.audio_ducking_enabled,
            config.audio_ducking_volume_percent,
        )
    }

    /// Returns whether ducking is actively enabled.
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::SeqCst)
    }

    /// Toggles ducking enabled state.
    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::SeqCst);
    }

    /// Returns configured target ducking volume percentage.
    #[must_use]
    pub fn target_volume_percent(&self) -> u32 {
        self.target_volume_percent.load(Ordering::SeqCst)
    }

    /// Sets target ducking volume percentage (clamped 0..=100).
    pub fn set_target_volume_percent(&self, percent: u32) {
        self.target_volume_percent
            .store(percent.min(100), Ordering::SeqCst);
    }

    /// Indicates whether background streams are currently in a ducked state.
    #[must_use]
    pub fn is_ducked(&self) -> bool {
        self.is_ducked.load(Ordering::SeqCst)
    }

    /// Lowers the volume of all active non-Viernes media streams to the configured ducking level.
    ///
    /// Idempotent: If already in a ducked state, subsequent calls return immediately
    /// without overwriting the recorded original volume levels.
    pub async fn duck(&self) -> usize {
        if !self.is_enabled() {
            return 0;
        }

        if self.is_ducked.swap(true, Ordering::SeqCst) {
            // Already ducked; protect original volume snapshot
            return 0;
        }

        let target_pct = self.target_volume_percent();
        let target_val = (PULSE_VOLUME_NORM * target_pct) / 100;

        let output = match tokio::process::Command::new("pactl")
            .args(["-f", "json", "list", "sink-inputs"])
            .output()
            .await
        {
            Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).to_string(),
            Ok(out) => {
                debug!(code = ?out.status.code(), "pactl list sink-inputs returned non-zero");
                self.is_ducked.store(false, Ordering::SeqCst);
                return 0;
            }
            Err(e) => {
                debug!(error = %e, "pactl not available or failed execution");
                self.is_ducked.store(false, Ordering::SeqCst);
                return 0;
            }
        };

        let streams = parse_duckable_streams(&output);
        if streams.is_empty() {
            debug!("No external audio streams found to duck");
            return 0;
        }

        let count = streams.len();
        info!(
            count,
            target_pct, "Ducking background media streams via PipeWire/PulseAudio"
        );

        for stream in &streams {
            let _ = tokio::process::Command::new("pactl")
                .args([
                    "set-sink-input-volume",
                    &stream.index.to_string(),
                    &target_val.to_string(),
                ])
                .status()
                .await;
        }

        let mut guard = self.ducked_streams.lock().await;
        *guard = streams;
        count
    }

    /// Restores all previously ducked background media streams to their original volume levels.
    ///
    /// Idempotent: Safe to call repeatedly or when no streams were ducked.
    pub async fn restore(&self) -> usize {
        if !self.is_ducked.swap(false, Ordering::SeqCst) {
            return 0;
        }

        let streams: Vec<DuckedStream> = {
            let mut guard = self.ducked_streams.lock().await;
            std::mem::take(&mut *guard)
        };

        if streams.is_empty() {
            return 0;
        }

        let count = streams.len();
        info!(
            count,
            "Restoring background media streams to original volumes"
        );

        for stream in &streams {
            let _ = tokio::process::Command::new("pactl")
                .args([
                    "set-sink-input-volume",
                    &stream.index.to_string(),
                    &stream.original_value.to_string(),
                ])
                .status()
                .await;
        }

        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_viernes_stream_filtering() {
        assert!(is_viernes_stream(
            Some("PipeWire ALSA [viernes]"),
            Some("alsa_playback.viernes")
        ));
        assert!(is_viernes_stream(Some("Viernes Voice Assistant"), None));
        assert!(is_viernes_stream(None, Some("node.viernes.playback")));
        assert!(!is_viernes_stream(Some("Brave"), Some("brave")));
        assert!(!is_viernes_stream(Some("Spotify"), Some("spotify")));
        assert!(!is_viernes_stream(None, None));
    }

    #[test]
    fn test_parse_duckable_streams_json() {
        let sample_json = r#"[
            {
                "index": 100,
                "volume": {
                    "front-left": { "value": 65536 }
                },
                "properties": {
                    "application.name": "Brave"
                }
            },
            {
                "index": 200,
                "volume": {
                    "front-left": { "value": 45000 }
                },
                "properties": {
                    "application.name": "PipeWire ALSA [viernes]"
                }
            },
            {
                "index": 300,
                "volume": {
                    "front-left": { "value": 52000 }
                },
                "properties": {
                    "application.name": "Spotify"
                }
            }
        ]"#;

        let streams = parse_duckable_streams(sample_json);
        assert_eq!(streams.len(), 2);
        assert_eq!(streams[0].index, 100);
        assert_eq!(streams[0].original_value, 65536);
        assert_eq!(streams[1].index, 300);
        assert_eq!(streams[1].original_value, 52000);
    }

    #[test]
    fn test_ducker_toggle_and_defaults() {
        let ducker = AudioDucker::new(true, 25);
        assert!(ducker.is_enabled());
        assert_eq!(ducker.target_volume_percent(), 25);
        assert!(!ducker.is_ducked());

        ducker.set_enabled(false);
        assert!(!ducker.is_enabled());

        ducker.set_target_volume_percent(150);
        assert_eq!(ducker.target_volume_percent(), 100); // clamped
    }

    #[tokio::test]
    async fn test_ducker_double_duck_safety() {
        let ducker = AudioDucker::new(true, 20);
        assert!(!ducker.is_ducked());

        let first_ducked = ducker.duck().await;
        // Second call while already ducked must be an idempotent no-op
        let second_ducked = ducker.duck().await;
        assert_eq!(second_ducked, 0);

        // Restore returns the exact count restored
        let restored = ducker.restore().await;
        assert_eq!(restored, first_ducked);

        // Second restore when already restored must be an idempotent no-op
        let second_restored = ducker.restore().await;
        assert_eq!(second_restored, 0);
    }
}
