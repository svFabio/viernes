//! Automatic audio ducking controller for media playback during voice interactions.
//!
//! Queries PipeWire / PulseAudio via `pactl -f json list sink-inputs` to identify external
//! media streams (e.g. Spotify, browser playback, MPD, VLC), lowers their volume during active
//! voice turns (PTT speech capture, filler playback, and assistant speaking), and deterministically
//! restores them to their exact original levels upon returning to Idle.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tracing::{debug, info, warn};
use viernes_core::config::{ensure_private_dir, runtime_dir, ViernesConfig};

/// PulseAudio nominal 100% volume integer reference (0x10000 = 65536).
pub const PULSE_VOLUME_NORM: u32 = 65536;

const PACTL_TIMEOUT: Duration = Duration::from_secs(2);
const DUCKED_STATE_FILENAME: &str = "ducked_streams.json";

/// Record of an actively ducked audio sink input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DuckedStream {
    /// Sink input numerical identifier (e.g. 1428).
    pub index: u32,
    /// Original raw volume integer before ducking was applied.
    pub original_value: u32,
}

#[derive(Debug, Deserialize)]
struct SinkInputJson {
    index: u32,
    mute: Option<bool>,
    corked: Option<bool>,
    volume: Option<HashMap<String, ChannelVolumeJson>>,
    properties: Option<PropertiesJson>,
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

            if input.mute == Some(true) {
                debug!(index = input.index, "Skipping muted sink input stream");
                return None;
            }

            if input.corked == Some(true) {
                debug!(index = input.index, "Observed corked sink input stream");
            }

            let volume_map = input.volume?;
            let mut sum: u64 = 0;
            let mut count: u64 = 0;
            for ch in volume_map.values() {
                if let Some(val) = ch.value {
                    sum += u64::from(val);
                    count += 1;
                }
            }
            if count == 0 {
                return None;
            }

            let avg_val = (sum / count) as u32;
            Some(DuckedStream {
                index: input.index,
                original_value: avg_val,
            })
        })
        .collect()
}

/// Filters candidate streams so streams already at or below `target_val` are never turned up.
#[must_use]
pub fn filter_duckable_above_target(
    streams: Vec<DuckedStream>,
    target_val: u32,
) -> Vec<DuckedStream> {
    streams
        .into_iter()
        .filter(|s| s.original_value > target_val)
        .collect()
}

async fn run_pactl(args: &[&str]) -> Option<std::process::Output> {
    let mut cmd = tokio::process::Command::new("pactl");
    cmd.args(args).kill_on_drop(true);
    match tokio::time::timeout(PACTL_TIMEOUT, cmd.output()).await {
        Ok(Ok(out)) if out.status.success() => Some(out),
        Ok(Ok(out)) => {
            warn!(
                args = ?args,
                code = ?out.status.code(),
                stderr = %String::from_utf8_lossy(&out.stderr).trim(),
                "pactl returned non-zero exit status"
            );
            None
        }
        Ok(Err(e)) => {
            debug!(args = ?args, error = %e, "pactl not available or failed execution");
            None
        }
        Err(_) => {
            warn!(
                args = ?args,
                timeout = ?PACTL_TIMEOUT,
                "pactl command timed out"
            );
            None
        }
    }
}

fn persist_ducked_streams(path: &Path, streams: &[DuckedStream]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            ensure_private_dir(parent)?;
        }
    }
    let payload = serde_json::to_vec(streams)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    let tmp_path = path.with_extension("json.tmp");
    if let Ok(meta) = std::fs::symlink_metadata(&tmp_path) {
        if meta.file_type().is_symlink() {
            let _ = std::fs::remove_file(&tmp_path);
        }
    }
    if let Ok(meta) = std::fs::symlink_metadata(path) {
        if meta.file_type().is_symlink() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("refusing to overwrite symlink: {}", path.display()),
            ));
        }
    }

    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp_path)?;
        file.write_all(&payload)?;
        file.sync_all()?;
        std::fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(0o600))?;
    }
    #[cfg(not(unix))]
    {
        std::fs::write(&tmp_path, &payload)?;
    }

    std::fs::rename(&tmp_path, path)?;
    Ok(())
}

fn remove_state_file(path: &Path) {
    match std::fs::remove_file(path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            warn!(path = %path.display(), error = %e, "Failed to remove ducked_streams state file");
        }
    }
}

/// Manages reactive background media attenuation (Audio Ducking) for Viernes.
#[derive(Debug)]
pub struct AudioDucker {
    enabled: AtomicBool,
    target_volume_percent: AtomicU32,
    ducked_streams: Arc<Mutex<Vec<DuckedStream>>>,
    is_ducked: Arc<AtomicBool>,
    op_lock: Arc<Mutex<()>>,
    state_file_override: Option<PathBuf>,
    dry_run: bool,
    #[cfg(test)]
    mock_list_json: Option<String>,
}

impl Default for AudioDucker {
    fn default() -> Self {
        Self::new(true, 40)
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
            op_lock: Arc::new(Mutex::new(())),
            state_file_override: None,
            dry_run: false,
            #[cfg(test)]
            mock_list_json: None,
        }
    }

    /// Creates a hermetic `AudioDucker` that tracks duck/restore state without invoking `pactl` or touching disk.
    #[must_use]
    pub fn new_dry_run(enabled: bool, target_volume_percent: u32) -> Self {
        Self {
            enabled: AtomicBool::new(enabled),
            target_volume_percent: AtomicU32::new(target_volume_percent.min(100)),
            ducked_streams: Arc::new(Mutex::new(Vec::new())),
            is_ducked: Arc::new(AtomicBool::new(false)),
            op_lock: Arc::new(Mutex::new(())),
            state_file_override: None,
            dry_run: true,
            #[cfg(test)]
            mock_list_json: None,
        }
    }

    #[cfg(test)]
    fn with_state_file(enabled: bool, target_volume_percent: u32, state_file: PathBuf) -> Self {
        Self {
            enabled: AtomicBool::new(enabled),
            target_volume_percent: AtomicU32::new(target_volume_percent.min(100)),
            ducked_streams: Arc::new(Mutex::new(Vec::new())),
            is_ducked: Arc::new(AtomicBool::new(false)),
            op_lock: Arc::new(Mutex::new(())),
            state_file_override: Some(state_file),
            dry_run: false,
            mock_list_json: None,
        }
    }

    fn state_file_path(&self) -> PathBuf {
        self.state_file_override
            .clone()
            .unwrap_or_else(|| runtime_dir().join(DUCKED_STATE_FILENAME))
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
        let _op_guard = self.op_lock.lock().await;

        if !self.is_enabled() {
            return 0;
        }

        if self.is_ducked.load(Ordering::SeqCst) {
            // Already ducked; protect original volume snapshot
            return 0;
        }

        if self.dry_run {
            self.is_ducked.store(true, Ordering::SeqCst);
            return 0;
        }

        let target_pct = self.target_volume_percent();
        let target_val = (PULSE_VOLUME_NORM * target_pct) / 100;

        #[cfg(test)]
        let mocked_output = self.mock_list_json.clone();
        #[cfg(not(test))]
        let mocked_output: Option<String> = None;

        let output = if let Some(json) = mocked_output {
            json
        } else {
            let Some(out) = run_pactl(&["-f", "json", "list", "sink-inputs"]).await else {
                self.is_ducked.store(false, Ordering::SeqCst);
                return 0;
            };
            String::from_utf8_lossy(&out.stdout).to_string()
        };

        let streams = filter_duckable_above_target(parse_duckable_streams(&output), target_val);
        if streams.is_empty() {
            debug!("No external audio streams above target volume found to duck");
            self.is_ducked.store(false, Ordering::SeqCst);
            return 0;
        }

        info!(
            count = streams.len(),
            target_pct, "Ducking background media streams via PipeWire/PulseAudio"
        );

        let mut actually_ducked = Vec::with_capacity(streams.len());
        for stream in streams {
            let idx_str = stream.index.to_string();
            let val_str = target_val.to_string();
            if run_pactl(&["set-sink-input-volume", &idx_str, &val_str])
                .await
                .is_some()
            {
                actually_ducked.push(stream);
            }
        }

        if actually_ducked.is_empty() {
            self.is_ducked.store(false, Ordering::SeqCst);
            return 0;
        }

        let count = actually_ducked.len();
        let state_path = self.state_file_path();
        if let Err(e) = persist_ducked_streams(&state_path, &actually_ducked) {
            warn!(path = %state_path.display(), error = %e, "Failed to persist ducked_streams.json");
        }

        {
            let mut guard = self.ducked_streams.lock().await;
            *guard = actually_ducked;
        }
        self.is_ducked.store(true, Ordering::SeqCst);
        count
    }

    /// Restores all previously ducked background media streams to their original volume levels.
    ///
    /// Idempotent: Safe to call repeatedly or when no streams were ducked.
    pub async fn restore(&self) -> usize {
        let _op_guard = self.op_lock.lock().await;

        if !self.is_ducked.swap(false, Ordering::SeqCst) {
            return 0;
        }

        if self.dry_run {
            return 0;
        }

        let streams: Vec<DuckedStream> = {
            let mut guard = self.ducked_streams.lock().await;
            std::mem::take(&mut *guard)
        };

        remove_state_file(&self.state_file_path());

        if streams.is_empty() {
            return 0;
        }

        let count = streams.len();
        info!(
            count,
            "Restoring background media streams to original volumes"
        );

        for stream in &streams {
            let idx_str = stream.index.to_string();
            let val_str = stream.original_value.to_string();
            let _ = run_pactl(&["set-sink-input-volume", &idx_str, &val_str]).await;
        }

        count
    }

    /// Recovers and restores any leftover ducked streams persisted before an unexpected crash (`SIGKILL`).
    ///
    /// Skips all disk and `pactl` operations when constructed in `dry_run` mode.
    pub async fn recover_orphaned_ducking(&self) -> usize {
        if self.dry_run {
            return 0;
        }

        let _op_guard = self.op_lock.lock().await;
        let state_path = self.state_file_path();

        let content = match std::fs::read(&state_path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return 0,
            Err(e) => {
                warn!(path = %state_path.display(), error = %e, "Failed to read orphaned ducked_streams.json");
                return 0;
            }
        };

        let streams: Vec<DuckedStream> = match serde_json::from_slice(&content) {
            Ok(parsed) => parsed,
            Err(e) => {
                warn!(path = %state_path.display(), error = %e, "Corrupt orphaned ducked_streams.json; removing");
                remove_state_file(&state_path);
                return 0;
            }
        };

        remove_state_file(&state_path);

        if streams.is_empty() {
            return 0;
        }

        info!(
            count = streams.len(),
            "Recovering orphaned ducked media streams after previous crash"
        );

        let mut restored = 0;
        for stream in &streams {
            let idx_str = stream.index.to_string();
            let val_str = stream.original_value.to_string();
            if run_pactl(&["set-sink-input-volume", &idx_str, &val_str])
                .await
                .is_some()
            {
                restored += 1;
            }
        }

        restored
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

    #[test]
    fn test_parse_duckable_streams_mono_multichannel_muted_and_missing_volume() {
        let sample_json = r#"[
            {
                "index": 10,
                "mute": false,
                "corked": false,
                "volume": {
                    "mono": { "value": 50000 }
                },
                "properties": {
                    "application.name": "Mono Podcast"
                }
            },
            {
                "index": 20,
                "mute": true,
                "volume": {
                    "front-left": { "value": 65536 },
                    "front-right": { "value": 65536 }
                },
                "properties": {
                    "application.name": "Muted Browser"
                }
            },
            {
                "index": 30,
                "mute": false,
                "volume": {
                    "front-left": { "value": 60000 },
                    "front-right": { "value": 40000 }
                },
                "properties": {
                    "application.name": "Stereo Player"
                }
            },
            {
                "index": 40,
                "properties": {
                    "application.name": "Missing Volume"
                }
            },
            {
                "index": 50,
                "volume": {
                    "front-left": {}
                },
                "properties": {
                    "application.name": "Empty Channel Value"
                }
            }
        ]"#;

        let streams = parse_duckable_streams(sample_json);
        assert_eq!(streams.len(), 2);
        assert_eq!(streams[0].index, 10);
        assert_eq!(streams[0].original_value, 50_000);
        assert_eq!(streams[1].index, 30);
        assert_eq!(streams[1].original_value, 50_000);
    }

    #[test]
    fn test_filter_duckable_above_target_skips_quieter_streams() {
        let target_val = (PULSE_VOLUME_NORM * 40) / 100; // 26214
        let streams = vec![
            DuckedStream {
                index: 1,
                original_value: 10_000, // quieter than target: must be skipped
            },
            DuckedStream {
                index: 2,
                original_value: target_val, // equal to target: must be skipped
            },
            DuckedStream {
                index: 3,
                original_value: 60_000, // louder than target: must be kept
            },
        ];

        let filtered = filter_duckable_above_target(streams, target_val);
        assert_eq!(
            filtered,
            vec![DuckedStream {
                index: 3,
                original_value: 60_000
            }]
        );
    }

    #[tokio::test]
    async fn test_ducker_double_duck_safety() {
        let ducker = AudioDucker::new_dry_run(true, 20);
        assert!(!ducker.is_ducked());

        let first_ducked = ducker.duck().await;
        assert!(ducker.is_ducked());
        // Second call while already ducked must be an idempotent no-op
        let second_ducked = ducker.duck().await;
        assert_eq!(second_ducked, 0);

        // Restore returns the exact count restored
        let restored = ducker.restore().await;
        assert_eq!(restored, first_ducked);
        assert!(!ducker.is_ducked());

        // Second restore when already restored must be an idempotent no-op
        let second_restored = ducker.restore().await;
        assert_eq!(second_restored, 0);
    }

    #[tokio::test]
    async fn test_empty_streams_resets_is_ducked_flag() {
        let mut ducker = AudioDucker::new(true, 30);
        ducker.mock_list_json = Some("[]".to_string());

        let count = ducker.duck().await;
        assert_eq!(count, 0);
        assert!(
            !ducker.is_ducked(),
            "duck() with empty stream list must reset is_ducked to false"
        );

        // Also verify streams all quieter than target reset is_ducked to false
        ducker.mock_list_json = Some(
            r#"[{"index": 7, "volume": {"mono": {"value": 5000}}, "properties": {"application.name": "Quiet"}}]"#
                .to_string(),
        );
        let count2 = ducker.duck().await;
        assert_eq!(count2, 0);
        assert!(!ducker.is_ducked());
    }

    #[tokio::test]
    async fn test_orphan_state_persist_permissions_and_cleanup() {
        let temp_dir = tempfile::tempdir().unwrap();
        let state_path = temp_dir.path().join("run").join("ducked_streams.json");

        let orphaned = vec![DuckedStream {
            index: 999_999,
            original_value: 55_000,
        }];
        persist_ducked_streams(&state_path, &orphaned).unwrap();
        assert!(state_path.exists());

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let file_mode = std::fs::metadata(&state_path).unwrap().permissions().mode() & 0o777;
            assert_eq!(file_mode, 0o600);
        }

        // dry_run must NOT touch disk or delete orphan file
        let dry_ducker = AudioDucker::new_dry_run(true, 40);
        assert_eq!(dry_ducker.recover_orphaned_ducking().await, 0);
        assert!(state_path.exists());

        // Real recovery reads and deletes the orphan file
        let ducker = AudioDucker::with_state_file(true, 40, state_path.clone());
        let _ = ducker.recover_orphaned_ducking().await;
        assert!(
            !state_path.exists(),
            "Orphaned ducked_streams.json must be removed after recovery"
        );
    }
}
