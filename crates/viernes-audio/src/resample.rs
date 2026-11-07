//! Defensive software audio resampling and multichannel downmixing.
//!
//! Provides deterministic downsampling from arbitrary hardware sample rates
//! (e.g. 44.1 kHz, 48 kHz, 96 kHz) to 16.0 kHz mono Float32 PCM required by Whisper STT.

/// Downmixes interleaved multi-channel PCM Float32 audio into mono Float32 PCM.
///
/// # Panics
/// Does not panic. If `channels == 0`, returns empty vector.
#[must_use]
pub fn downmix_to_mono(interleaved: &[f32], channels: u16) -> Vec<f32> {
    if channels <= 1 || interleaved.is_empty() {
        return interleaved.to_vec();
    }

    let ch = channels as usize;
    let num_frames = interleaved.len() / ch;
    let mut mono = Vec::with_capacity(num_frames);

    let norm_factor = 1.0 / (ch as f32);

    for frame in interleaved.chunks_exact(ch) {
        let sum: f32 = frame.iter().sum();
        mono.push(sum * norm_factor);
    }

    mono
}

/// Software linear interpolation resampler.
///
/// Converts a mono Float32 audio buffer from `from_rate` to `to_rate`.
/// Clamps output samples defensively to `[-1.0, 1.0]`.
#[must_use]
pub fn resample_linear(samples: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if samples.is_empty() || from_rate == 0 || to_rate == 0 {
        return Vec::new();
    }

    if from_rate == to_rate {
        return samples.to_vec();
    }

    let ratio = from_rate as f64 / to_rate as f64;
    let target_len = ((samples.len() as f64) / ratio).round() as usize;
    let mut output = Vec::with_capacity(target_len);

    for i in 0..target_len {
        let src_pos = i as f64 * ratio;
        let idx = src_pos.floor() as usize;
        let frac = (src_pos - idx as f64) as f32;

        if idx + 1 < samples.len() {
            let s0 = samples[idx];
            let s1 = samples[idx + 1];
            let interp = s0 + frac * (s1 - s0);
            output.push(interp.clamp(-1.0, 1.0));
        } else if idx < samples.len() {
            output.push(samples[idx].clamp(-1.0, 1.0));
        } else {
            break;
        }
    }

    output
}

/// Full processing helper: downmixes multichannel audio and resamples to target rate (typically 16kHz).
#[must_use]
pub fn process_to_16k_mono(samples: &[f32], channels: u16, sample_rate: u32) -> Vec<f32> {
    let mono = downmix_to_mono(samples, channels);
    resample_linear(&mono, sample_rate, 16_000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_downmix_mono_identity() {
        let input = vec![0.1, -0.2, 0.5];
        let res = downmix_to_mono(&input, 1);
        assert_eq!(res, input);
    }

    #[test]
    fn test_downmix_stereo_to_mono() {
        // L = 1.0, R = 0.0 -> mono = 0.5
        // L = -0.5, R = -0.5 -> mono = -0.5
        let stereo = vec![1.0, 0.0, -0.5, -0.5];
        let mono = downmix_to_mono(&stereo, 2);
        assert_eq!(mono.len(), 2);
        assert!((mono[0] - 0.5).abs() < 1e-6);
        assert!((mono[1] - (-0.5)).abs() < 1e-6);
    }

    #[test]
    fn test_resample_linear_identity() {
        let input = vec![0.0, 0.5, 1.0, 0.5, 0.0];
        let output = resample_linear(&input, 16000, 16000);
        assert_eq!(output, input);
    }

    #[test]
    fn test_resample_linear_downsample_48k_to_16k() {
        // 48kHz down to 16kHz is 3:1 ratio
        let input: Vec<f32> = (0..480).map(|i| i as f32 / 480.0).collect();
        let output = resample_linear(&input, 48000, 16000);

        assert_eq!(output.len(), 160);
        // Verify monotonicity and boundary preservation
        assert!((output[0] - input[0]).abs() < 1e-5);
        assert!((output[159] - input[477]).abs() < 0.05);
    }

    #[test]
    fn test_resample_linear_downsample_44k_to_16k() {
        // 44.1kHz to 16kHz
        let input = vec![0.0; 4410]; // 100ms
        let output = resample_linear(&input, 44100, 16000);
        assert_eq!(output.len(), 1600); // 100ms at 16kHz
    }

    #[test]
    fn test_process_to_16k_mono_stereo_48k() {
        // 2 channels @ 48kHz, 960 total samples = 480 frames = 10ms
        let stereo = vec![0.2; 960];
        let processed = process_to_16k_mono(&stereo, 2, 48000);
        assert_eq!(processed.len(), 160); // 10ms at 16kHz
        assert!((processed[0] - 0.2).abs() < 1e-6);
    }
}
