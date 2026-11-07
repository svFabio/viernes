//! Audio device enumeration, inspection, and safe resolution with fallback.
//!
//! Provides support for ALSA, PipeWire, and PulseAudio via `cpal`,
//! ensuring safe resolution by index, exact name, or partial match,
//! with automatic fallback to system defaults.

use cpal::traits::{DeviceTrait, HostTrait};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};
use viernes_core::error::ViernesError;

/// Structured descriptor representing an audio input or output endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioDeviceInfo {
    /// Zero-based sequential enumeration index.
    pub index: usize,
    /// Human-readable device name reported by the host audio subsystem.
    pub name: String,
    /// Indicates whether this device is currently configured as the host system default.
    pub is_default: bool,
    /// Maximum supported channels (e.g. 1 for mono, 2 for stereo).
    pub max_channels: u16,
    /// Sample rates supported by this device.
    pub sample_rates: Vec<u32>,
}

impl std::fmt::Display for AudioDeviceInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[{}] {} ({} ch{}){}",
            self.index,
            self.name,
            self.max_channels,
            if self.max_channels == 1 { "" } else { "s" },
            if self.is_default { " [DEFAULT]" } else { "" }
        )
    }
}

/// Abstraction allowing device enumeration to be mocked hermetically in CI.
pub trait DeviceEnumerator: Send + Sync {
    /// Enumerates all accessible audio input endpoints (microphones).
    fn list_input_devices(&self) -> Result<Vec<AudioDeviceInfo>, ViernesError>;

    /// Enumerates all accessible audio output endpoints (speakers/headphones).
    fn list_output_devices(&self) -> Result<Vec<AudioDeviceInfo>, ViernesError>;

    /// Obtains the default input device, if available.
    fn default_input_device(&self) -> Result<Option<AudioDeviceInfo>, ViernesError>;

    /// Obtains the default output device, if available.
    fn default_output_device(&self) -> Result<Option<AudioDeviceInfo>, ViernesError>;
}

/// Production implementation of [`DeviceEnumerator`] utilizing native `cpal` host APIs.
#[derive(Debug, Default)]
pub struct CpalDeviceEnumerator;

impl CpalDeviceEnumerator {
    /// Creates a new native `cpal` device enumerator.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl DeviceEnumerator for CpalDeviceEnumerator {
    fn list_input_devices(&self) -> Result<Vec<AudioDeviceInfo>, ViernesError> {
        let host = cpal::default_host();
        let default_name = host.default_input_device().and_then(|d| d.name().ok());

        let devices = host
            .input_devices()
            .map_err(|e| ViernesError::Audio(format!("Failed to query input devices: {e}")))?;

        let mut list = Vec::new();
        for (index, dev) in devices.enumerate() {
            let name = dev.name().unwrap_or_else(|_| format!("Input Device #{index}"));
            let is_default = default_name.as_deref() == Some(&name);

            let mut max_channels = 0;
            let mut sample_rates = Vec::new();
            if let Ok(configs) = dev.supported_input_configs() {
                for c in configs {
                    if c.channels() > max_channels {
                        max_channels = c.channels();
                    }
                    sample_rates.push(c.min_sample_rate().0);
                    sample_rates.push(c.max_sample_rate().0);
                }
            }
            sample_rates.sort_unstable();
            sample_rates.dedup();

            list.push(AudioDeviceInfo {
                index,
                name,
                is_default,
                max_channels,
                sample_rates,
            });
        }

        Ok(list)
    }

    fn list_output_devices(&self) -> Result<Vec<AudioDeviceInfo>, ViernesError> {
        let host = cpal::default_host();
        let default_name = host.default_output_device().and_then(|d| d.name().ok());

        let devices = host
            .output_devices()
            .map_err(|e| ViernesError::Audio(format!("Failed to query output devices: {e}")))?;

        let mut list = Vec::new();
        for (index, dev) in devices.enumerate() {
            let name = dev.name().unwrap_or_else(|_| format!("Output Device #{index}"));
            let is_default = default_name.as_deref() == Some(&name);

            let mut max_channels = 0;
            let mut sample_rates = Vec::new();
            if let Ok(configs) = dev.supported_output_configs() {
                for c in configs {
                    if c.channels() > max_channels {
                        max_channels = c.channels();
                    }
                    sample_rates.push(c.min_sample_rate().0);
                    sample_rates.push(c.max_sample_rate().0);
                }
            }
            sample_rates.sort_unstable();
            sample_rates.dedup();

            list.push(AudioDeviceInfo {
                index,
                name,
                is_default,
                max_channels,
                sample_rates,
            });
        }

        Ok(list)
    }

    fn default_input_device(&self) -> Result<Option<AudioDeviceInfo>, ViernesError> {
        let inputs = self.list_input_devices()?;
        Ok(inputs.into_iter().find(|d| d.is_default))
    }

    fn default_output_device(&self) -> Result<Option<AudioDeviceInfo>, ViernesError> {
        let outputs = self.list_output_devices()?;
        Ok(outputs.into_iter().find(|d| d.is_default))
    }
}

/// Resolves an audio device from a collection using an identifier query.
///
/// Query matching precedence:
/// 1. If `identifier` is empty or `None`, returns the default device.
/// 2. If `identifier` parses as an integer, matches by numeric `index`.
/// 3. Exact case-insensitive match on device `name`.
/// 4. Substring (contains) case-insensitive match on device `name`.
/// 5. Fallback to `default_dev`, or first available device if no default exists.
#[must_use]
pub fn resolve_device<'a>(
    identifier: Option<&str>,
    devices: &'a [AudioDeviceInfo],
    default_dev: Option<&'a AudioDeviceInfo>,
) -> Option<&'a AudioDeviceInfo> {
    let query = identifier.map(str::trim).filter(|s| !s.is_empty());

    let Some(query) = query else {
        debug!("No device identifier requested; utilizing default device");
        return default_dev.or_else(|| devices.first());
    };

    // 1. Numeric index match
    if let Ok(idx) = query.parse::<usize>() {
        if let Some(matched) = devices.iter().find(|d| d.index == idx) {
            info!("Matched device by numeric index {}: '{}'", idx, matched.name);
            return Some(matched);
        }
        warn!("Device index {} was requested but not found in available endpoints", idx);
    }

    // 2. Exact case-insensitive name match
    let query_lower = query.to_lowercase();
    if let Some(matched) = devices.iter().find(|d| d.name.to_lowercase() == query_lower) {
        info!("Matched device by exact name: '{}'", matched.name);
        return Some(matched);
    }

    // 3. Partial case-insensitive substring match
    if let Some(matched) = devices
        .iter()
        .find(|d| d.name.to_lowercase().contains(&query_lower))
    {
        info!(
            "Matched device by partial substring '{}': '{}'",
            query, matched.name
        );
        return Some(matched);
    }

    warn!(
        "Audio device '{}' not found among {} devices; falling back to default",
        query,
        devices.len()
    );
    default_dev.or_else(|| devices.first())
}

/// Resolves the input audio device respecting the `AUDIO_INPUT_DEVICE` environment variable.
pub fn resolve_input_device_from_env(
    enumerator: &dyn DeviceEnumerator,
) -> Result<Option<AudioDeviceInfo>, ViernesError> {
    let devices = enumerator.list_input_devices()?;
    let default_dev = enumerator.default_input_device()?;
    let env_query = std::env::var("AUDIO_INPUT_DEVICE").ok();

    Ok(resolve_device(env_query.as_deref(), &devices, default_dev.as_ref()).cloned())
}

/// Resolves the output audio device respecting the `AUDIO_OUTPUT_DEVICE` environment variable.
pub fn resolve_output_device_from_env(
    enumerator: &dyn DeviceEnumerator,
) -> Result<Option<AudioDeviceInfo>, ViernesError> {
    let devices = enumerator.list_output_devices()?;
    let default_dev = enumerator.default_output_device()?;
    let env_query = std::env::var("AUDIO_OUTPUT_DEVICE").ok();

    Ok(resolve_device(env_query.as_deref(), &devices, default_dev.as_ref()).cloned())
}

/// Hermetic fake device enumerator for unit tests and CI runners.
#[derive(Debug, Default, Clone)]
pub struct MockDeviceEnumerator {
    /// Injected mock input devices.
    pub inputs: Vec<AudioDeviceInfo>,
    /// Injected mock output devices.
    pub outputs: Vec<AudioDeviceInfo>,
}

impl MockDeviceEnumerator {
    /// Creates a mock enumerator with predefined test fixtures.
    #[must_use]
    pub fn new(inputs: Vec<AudioDeviceInfo>, outputs: Vec<AudioDeviceInfo>) -> Self {
        Self { inputs, outputs }
    }
}

impl DeviceEnumerator for MockDeviceEnumerator {
    fn list_input_devices(&self) -> Result<Vec<AudioDeviceInfo>, ViernesError> {
        Ok(self.inputs.clone())
    }

    fn list_output_devices(&self) -> Result<Vec<AudioDeviceInfo>, ViernesError> {
        Ok(self.outputs.clone())
    }

    fn default_input_device(&self) -> Result<Option<AudioDeviceInfo>, ViernesError> {
        Ok(self.inputs.iter().find(|d| d.is_default).cloned())
    }

    fn default_output_device(&self) -> Result<Option<AudioDeviceInfo>, ViernesError> {
        Ok(self.outputs.iter().find(|d| d.is_default).cloned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_devices() -> Vec<AudioDeviceInfo> {
        vec![
            AudioDeviceInfo {
                index: 0,
                name: "Built-in Audio Analog Stereo".into(),
                is_default: true,
                max_channels: 2,
                sample_rates: vec![44100, 48000],
            },
            AudioDeviceInfo {
                index: 1,
                name: "HyperX SoloCast USB Microphone".into(),
                is_default: false,
                max_channels: 1,
                sample_rates: vec![48000, 96000],
            },
            AudioDeviceInfo {
                index: 2,
                name: "PipeWire Virtual Sink".into(),
                is_default: false,
                max_channels: 2,
                sample_rates: vec![16000, 44100, 48000],
            },
        ]
    }

    #[test]
    fn test_resolve_default_when_no_identifier() {
        let devs = sample_devices();
        let default_dev = devs.iter().find(|d| d.is_default);

        let resolved = resolve_device(None, &devs, default_dev);
        assert_eq!(resolved.unwrap().name, "Built-in Audio Analog Stereo");

        let resolved_empty = resolve_device(Some("  "), &devs, default_dev);
        assert_eq!(resolved_empty.unwrap().name, "Built-in Audio Analog Stereo");
    }

    #[test]
    fn test_resolve_by_numeric_index() {
        let devs = sample_devices();
        let default_dev = devs.iter().find(|d| d.is_default);

        let resolved = resolve_device(Some("1"), &devs, default_dev);
        assert_eq!(resolved.unwrap().name, "HyperX SoloCast USB Microphone");

        // Index out of range falls back to default
        let resolved_oob = resolve_device(Some("99"), &devs, default_dev);
        assert_eq!(resolved_oob.unwrap().name, "Built-in Audio Analog Stereo");
    }

    #[test]
    fn test_resolve_by_exact_name_case_insensitive() {
        let devs = sample_devices();
        let default_dev = devs.iter().find(|d| d.is_default);

        let resolved = resolve_device(
            Some("hyperx solocast usb microphone"),
            &devs,
            default_dev,
        );
        assert_eq!(resolved.unwrap().name, "HyperX SoloCast USB Microphone");
    }

    #[test]
    fn test_resolve_by_partial_match() {
        let devs = sample_devices();
        let default_dev = devs.iter().find(|d| d.is_default);

        let resolved = resolve_device(Some("solocast"), &devs, default_dev);
        assert_eq!(resolved.unwrap().name, "HyperX SoloCast USB Microphone");

        let resolved_pipewire = resolve_device(Some("pipewire"), &devs, default_dev);
        assert_eq!(resolved_pipewire.unwrap().name, "PipeWire Virtual Sink");
    }

    #[test]
    fn test_resolve_fallback_when_not_found() {
        let devs = sample_devices();
        let default_dev = devs.iter().find(|d| d.is_default);

        let resolved = resolve_device(Some("NonExistentDeviceXYZ"), &devs, default_dev);
        assert_eq!(resolved.unwrap().name, "Built-in Audio Analog Stereo");
    }

    #[test]
    fn test_display_formatting() {
        let dev = AudioDeviceInfo {
            index: 3,
            name: "Focusrite Scarlett Solo".into(),
            is_default: true,
            max_channels: 2,
            sample_rates: vec![44100, 48000, 96000],
        };
        let formatted = format!("{dev}");
        assert_eq!(formatted, "[3] Focusrite Scarlett Solo (2 chs) [DEFAULT]");
    }

    #[test]
    fn test_mock_device_enumerator() {
        let devs = sample_devices();
        let mock = MockDeviceEnumerator::new(devs.clone(), vec![]);

        let inputs = mock.list_input_devices().unwrap();
        assert_eq!(inputs.len(), 3);

        let default_in = mock.default_input_device().unwrap();
        assert!(default_in.unwrap().is_default);
    }
}
