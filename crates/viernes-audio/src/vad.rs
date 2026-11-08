//! Real-time Voice Activity Detection (VAD) with sliding window RMS energy estimation.
//!
//! Provides deterministic speech segment boundary detection, spurious click rejection (< 250ms),
//! silence timeout accumulation (default 800ms), and 20-second safety cutoff protection.

use serde::{Deserialize, Serialize};
use tracing::{debug, info};

/// Events emitted by the Voice Activity Detector.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum VadEvent {
    /// Human speech onset detected (RMS exceeded threshold for >= 2 consecutive frames).
    VoiceStarted,
    /// Continuous speech completed or cut off by safety limit, returning the accumulated 16kHz Float32 PCM buffer.
    VoiceFinished(Vec<f32>),
}

/// Internal operational state of the VAD state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VadState {
    /// Quiescent environment; waiting for sustained energy.
    #[default]
    Silence,
    /// Actively accumulating voiced speech samples.
    Speech,
}

/// Computes the Root Mean Square (RMS) energy across a slice of PCM Float32 audio samples.
#[must_use]
pub fn compute_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_squares: f32 = samples.iter().map(|&s| s * s).sum();
    (sum_squares / (samples.len() as f32)).sqrt()
}

/// Real-time Voice Activity Detector.
pub struct VoiceActivityDetector {
    sample_rate: u32,
    threshold: f32,
    silence_timeout_ms: u64,
    state: VadState,
    consecutive_voice_frames: usize,
    accumulated_silence_samples: usize,
    speech_buffer: Vec<f32>,
    pre_roll_buffer: Vec<f32>,
    pre_roll_capacity: usize,
    min_speech_samples: usize,
    max_speech_samples: usize,
    silence_timeout_enabled: bool,
}

impl VoiceActivityDetector {
    /// Minimum speech duration threshold (250 ms) to filter out spurious clicks.
    pub const MIN_SPEECH_DURATION_MS: u64 = 250;
    /// Maximum allowed speech duration (20 seconds) before enforcing a safety cutoff.
    pub const MAX_SPEECH_DURATION_SECS: u64 = 20;
    /// Required consecutive high-energy frames to transition from silence to speech.
    pub const CONSECUTIVE_FRAMES_TRIGGER: usize = 2;

    /// Creates a new `VoiceActivityDetector` instance.
    #[must_use]
    pub fn new(sample_rate: u32, threshold: f32, silence_timeout_ms: u64) -> Self {
        let min_speech_samples =
            ((sample_rate as f64) * (Self::MIN_SPEECH_DURATION_MS as f64 / 1000.0)) as usize;
        let max_speech_samples = (sample_rate as usize) * (Self::MAX_SPEECH_DURATION_SECS as usize);
        // Retain 100ms of pre-roll to avoid clipping leading consonants
        let pre_roll_capacity = ((sample_rate as f64) * 0.100) as usize;

        Self {
            sample_rate,
            threshold,
            silence_timeout_ms,
            state: VadState::Silence,
            consecutive_voice_frames: 0,
            accumulated_silence_samples: 0,
            speech_buffer: Vec::new(),
            pre_roll_buffer: Vec::with_capacity(pre_roll_capacity),
            pre_roll_capacity,
            min_speech_samples,
            max_speech_samples,
            silence_timeout_enabled: true,
        }
    }

    /// Enables or disables the silence timeout cutoff.
    ///
    /// When disabled (e.g. during Push-to-Talk holding), audio continues accumulating
    /// across silence intervals without purging the speech buffer.
    pub fn set_silence_timeout_enabled(&mut self, enabled: bool) {
        self.silence_timeout_enabled = enabled;
    }

    /// Indicates whether the silence timeout cutoff is currently active.
    #[must_use]
    pub fn is_silence_timeout_enabled(&self) -> bool {
        self.silence_timeout_enabled
    }

    /// Dynamically reconfigures the RMS sensitivity threshold on the fly.
    pub fn set_threshold(&mut self, new_threshold: f32) {
        info!(
            "Updated VAD threshold from {} to {}",
            self.threshold, new_threshold
        );
        self.threshold = new_threshold;
    }

    /// Obtains the active RMS threshold.
    #[must_use]
    pub fn threshold(&self) -> f32 {
        self.threshold
    }

    /// Resets detector to initial quiescent state, clearing all internal buffers.
    pub fn reset(&mut self) {
        self.state = VadState::Silence;
        self.consecutive_voice_frames = 0;
        self.accumulated_silence_samples = 0;
        self.speech_buffer.clear();
        self.pre_roll_buffer.clear();
    }

    /// Force finish the current speech segment immediately (e.g. on PTT release),
    /// returning the accumulated PCM buffer if it meets minimum speech length.
    pub fn force_finish(&mut self) -> Option<Vec<f32>> {
        if self.speech_buffer.is_empty() {
            return None;
        }
        let buffer = std::mem::take(&mut self.speech_buffer);
        self.reset();
        if buffer.len() >= self.min_speech_samples {
            Some(buffer)
        } else {
            None
        }
    }

    /// Explicitly transitions VAD into speech accumulation mode (e.g. on PTT press).
    pub fn start_speech_capture(&mut self) {
        self.reset();
        self.state = VadState::Speech;
    }

    /// Returns current state of the VAD machine.
    #[must_use]
    pub fn state(&self) -> VadState {
        self.state
    }

    /// Processes an incoming chunk of Float32 PCM audio, returning any triggered VAD events.
    pub fn feed_chunk(&mut self, chunk: &[f32]) -> Vec<VadEvent> {
        if chunk.is_empty() {
            return Vec::new();
        }

        let mut events = Vec::new();
        let rms = compute_rms(chunk);
        let is_voice = rms >= self.threshold;

        let silence_threshold_samples =
            ((self.sample_rate as f64) * (self.silence_timeout_ms as f64 / 1000.0)) as usize;

        match self.state {
            VadState::Silence => {
                // Maintain pre-roll sliding buffer
                self.pre_roll_buffer.extend_from_slice(chunk);
                if self.pre_roll_buffer.len() > self.pre_roll_capacity {
                    let drain_count = self.pre_roll_buffer.len() - self.pre_roll_capacity;
                    self.pre_roll_buffer.drain(0..drain_count);
                }

                if is_voice {
                    self.consecutive_voice_frames += 1;
                    if self.consecutive_voice_frames >= Self::CONSECUTIVE_FRAMES_TRIGGER {
                        info!(
                            "VAD: Voice onset detected (RMS: {:.4} >= {:.4})",
                            rms, self.threshold
                        );
                        self.state = VadState::Speech;
                        self.consecutive_voice_frames = 0;
                        self.accumulated_silence_samples = 0;

                        // Start speech buffer with saved pre-roll audio
                        self.speech_buffer.clear();
                        self.speech_buffer.extend_from_slice(&self.pre_roll_buffer);
                        self.pre_roll_buffer.clear();

                        events.push(VadEvent::VoiceStarted);
                    }
                } else {
                    self.consecutive_voice_frames = 0;
                }
            }
            VadState::Speech => {
                self.speech_buffer.extend_from_slice(chunk);

                if is_voice {
                    self.accumulated_silence_samples = 0;
                } else {
                    self.accumulated_silence_samples += chunk.len();
                }

                // Check 20-second safety cutoff limit
                if self.speech_buffer.len() >= self.max_speech_samples {
                    info!(
                        "VAD: Max safety cutoff reached (20s limit exceeded); forcing segment cut"
                    );
                    let final_buffer = std::mem::take(&mut self.speech_buffer);
                    self.reset();
                    events.push(VadEvent::VoiceFinished(final_buffer));
                    return events;
                }

                // Check sustained silence timeout if enabled
                if self.silence_timeout_enabled
                    && self.accumulated_silence_samples >= silence_threshold_samples
                {
                    // Check if voiced speech segment exceeds minimum click threshold (250ms)
                    let voiced_samples = self
                        .speech_buffer
                        .len()
                        .saturating_sub(self.accumulated_silence_samples);

                    if voiced_samples >= self.min_speech_samples {
                        info!(
                            "VAD: Silence timeout reached ({} ms); completed segment with {} samples",
                            self.silence_timeout_ms,
                            self.speech_buffer.len()
                        );
                        let final_buffer = std::mem::take(&mut self.speech_buffer);
                        self.reset();
                        events.push(VadEvent::VoiceFinished(final_buffer));
                    } else {
                        debug!(
                            "VAD: Discarding spurious click (< 250ms duration: {} voiced samples)",
                            voiced_samples
                        );
                        self.reset();
                    }
                }
            }
        }

        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_rms_silence_and_sine() {
        let silence = vec![0.0; 320];
        assert_eq!(compute_rms(&silence), 0.0);

        let constant = vec![0.5; 320];
        assert!((compute_rms(&constant) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_vad_voice_started_and_finished_lifecycle() {
        let sample_rate = 16000;
        let mut vad = VoiceActivityDetector::new(sample_rate, 0.05, 400);

        // Frame duration 20ms = 320 samples @ 16kHz
        let silence_frame = vec![0.001; 320];
        let voice_frame = vec![0.2; 320];

        // 1. Initial silence
        let evs = vad.feed_chunk(&silence_frame);
        assert!(evs.is_empty());
        assert_eq!(vad.state(), VadState::Silence);

        // 2. First voice frame (threshold exceeded, but requires 2 consecutive)
        let evs = vad.feed_chunk(&voice_frame);
        assert!(evs.is_empty());
        assert_eq!(vad.state(), VadState::Silence);

        // 3. Second voice frame -> triggers VoiceStarted!
        let evs = vad.feed_chunk(&voice_frame);
        assert_eq!(evs, vec![VadEvent::VoiceStarted]);
        assert_eq!(vad.state(), VadState::Speech);

        // 4. Feed 20 frames of voice (400ms > 250ms minimum)
        for _ in 0..20 {
            vad.feed_chunk(&voice_frame);
        }

        // 5. Feed silence for 400ms (20 frames * 20ms = 400ms = timeout)
        let mut finished_event = None;
        for _ in 0..20 {
            let evs = vad.feed_chunk(&silence_frame);
            if let Some(ev) = evs
                .into_iter()
                .find(|e| matches!(e, VadEvent::VoiceFinished(_)))
            {
                finished_event = Some(ev);
                break;
            }
        }

        assert!(finished_event.is_some());
        if let Some(VadEvent::VoiceFinished(samples)) = finished_event {
            assert!(samples.len() > 4000); // Exceeds 250ms
        }
        assert_eq!(vad.state(), VadState::Silence);
    }

    #[test]
    fn test_vad_click_rejection() {
        let sample_rate = 16000;
        let mut vad = VoiceActivityDetector::new(sample_rate, 0.05, 200);

        let voice_frame = vec![0.3; 320];
        let silence_frame = vec![0.001; 320];

        // Start voice (2 frames = 40ms)
        vad.feed_chunk(&voice_frame);
        vad.feed_chunk(&voice_frame);
        assert_eq!(vad.state(), VadState::Speech);

        // Feed only 1 more voice frame (total 60ms < 250ms)
        vad.feed_chunk(&voice_frame);

        // Feed silence until timeout
        let mut finished = false;
        for _ in 0..15 {
            let evs = vad.feed_chunk(&silence_frame);
            if evs.iter().any(|e| matches!(e, VadEvent::VoiceFinished(_))) {
                finished = true;
            }
        }

        // Must be rejected as spurious click: NO VoiceFinished event!
        assert!(!finished);
        assert_eq!(vad.state(), VadState::Silence);
    }

    #[test]
    fn test_vad_safety_limit_cutoff_20s() {
        let sample_rate = 16000;
        let mut vad = VoiceActivityDetector::new(sample_rate, 0.05, 800);

        let voice_chunk = vec![0.2; 16000]; // 1 second chunk

        // Trigger speech
        vad.feed_chunk(&voice_chunk);
        vad.feed_chunk(&voice_chunk);
        assert_eq!(vad.state(), VadState::Speech);

        // Feed 20 seconds of continuous voice without pauses
        let mut cut_event = None;
        for _ in 0..20 {
            let evs = vad.feed_chunk(&voice_chunk);
            if let Some(ev) = evs
                .into_iter()
                .find(|e| matches!(e, VadEvent::VoiceFinished(_)))
            {
                cut_event = Some(ev);
                break;
            }
        }

        assert!(
            cut_event.is_some(),
            "Safety cutoff must trigger at 20 seconds"
        );
        assert_eq!(vad.state(), VadState::Silence);
    }

    #[test]
    fn test_vad_ptt_mode_preserves_audio_across_pauses() {
        let sample_rate = 16000;
        let mut vad = VoiceActivityDetector::new(sample_rate, 0.05, 800);
        vad.set_silence_timeout_enabled(false);
        vad.start_speech_capture();

        let voice_frame = vec![0.2; 320]; // 20ms of speech
        let silence_frame = vec![0.001; 320]; // 20ms of silence

        // Feed speech (500ms)
        for _ in 0..25 {
            vad.feed_chunk(&voice_frame);
        }
        assert_eq!(vad.state(), VadState::Speech);

        // Feed long silence pause (1.2 seconds > 800ms silence timeout)
        for _ in 0..60 {
            let evs = vad.feed_chunk(&silence_frame);
            assert!(
                evs.is_empty(),
                "PTT mode must not emit VoiceFinished on pause"
            );
        }
        assert_eq!(
            vad.state(),
            VadState::Speech,
            "PTT mode must stay in Speech state"
        );

        // Feed speech again (500ms)
        for _ in 0..25 {
            vad.feed_chunk(&voice_frame);
        }

        // Release PTT
        let finished = vad.force_finish().expect("Must retain full speech buffer");
        // 25 + 60 + 25 = 110 frames * 320 samples = 35200 samples
        assert_eq!(finished.len(), 35_200);
        assert_eq!(vad.state(), VadState::Silence);
    }
}
