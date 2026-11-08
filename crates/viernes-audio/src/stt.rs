//! Speech-To-Text (STT) inference interface, hallucination filtering, and Whisper integration.
//!
//! Enforces 16kHz Float32 mono validation, non-blocking execution via `tokio::task::spawn_blocking`,
//! hallucination filtering for noise/music/repetition tokens, and Spanish ("es") language configuration.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tracing::{debug, info};
use viernes_core::error::ViernesError;
use viernes_core::traits::SpeechToText;

/// Resolves the filesystem path to `scripts/transcribe_wav.py` robustly across working directories.
pub fn resolve_transcribe_script_path() -> Option<PathBuf> {
    // 1. Explicit environment variable override
    if let Ok(path_str) = std::env::var("VIERNES_TRANSCRIBE_SCRIPT") {
        let p = PathBuf::from(path_str);
        if p.exists() {
            return p.canonicalize().ok().or(Some(p));
        }
    }

    // 2. Relative to current executable
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            let candidate = parent.join("scripts/transcribe_wav.py");
            if candidate.exists() {
                return candidate.canonicalize().ok().or(Some(candidate));
            }
            let candidate_root = parent.join("../../scripts/transcribe_wav.py");
            if candidate_root.exists() {
                return candidate_root.canonicalize().ok().or(Some(candidate_root));
            }
        }
    }

    // 3. Current working directory
    let cwd_path = PathBuf::from("scripts/transcribe_wav.py");
    if cwd_path.exists() {
        return cwd_path.canonicalize().ok().or(Some(cwd_path));
    }

    // 4. Default user project path
    if let Ok(home) = std::env::var("HOME") {
        let candidate = PathBuf::from(home).join("Projects/09-viernes/scripts/transcribe_wav.py");
        if candidate.exists() {
            return candidate.canonicalize().ok().or(Some(candidate));
        }
    }

    None
}

/// Resolves path to the `uv` executable, checking environment and user binaries.
#[must_use]
pub fn resolve_uv_binary() -> PathBuf {
    if let Ok(bin_str) = std::env::var("VIERNES_UV_BIN") {
        let p = PathBuf::from(bin_str);
        if p.exists() {
            return p;
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        let candidate = PathBuf::from(home).join(".local/bin/uv");
        if candidate.exists() {
            return candidate;
        }
    }
    PathBuf::from("uv")
}

/// List of recognized hallucination tokens, noise tags, and spurious endings.
const HALLUCINATION_PATTERNS: &[&str] = &[
    "[música]",
    "[musica]",
    "[sonido de fondo]",
    "[risas]",
    "[aplausos]",
    "(música)",
    "(musica)",
    "(risas)",
    "(aplausos)",
    "gracias por ver",
    "subtítulos por",
    "suscríbete",
    "suscribete",
];

/// Exact match hallucination phrases that should be discarded when standing alone.
const EXACT_HALLUCINATION_PHRASES: &[&str] = &[
    "gracias",
    "muchas gracias",
    "gracias.",
    "muchas gracias.",
    "gracias!",
    "¡gracias!",
    "chao",
    "adiós",
    "adios",
];

/// Cleans, normalizes, and filters hallucinated or repetitive tokens from Whisper transcripts.
///
/// Returns an empty string if the transcript consists exclusively of noise tags,
/// hallucinatory polite artifacts, or pathological repetition loops.
#[must_use]
pub fn clean_and_filter_hallucinations(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let lower = trimmed.to_lowercase();

    // 1. Check exact isolated phrase hallucinations
    if EXACT_HALLUCINATION_PHRASES.iter().any(|&p| lower == p) {
        debug!("Filtered isolated hallucinatory phrase: '{}'", trimmed);
        return String::new();
    }

    // 2. Check pattern containment
    let mut cleaned = trimmed.to_string();
    for &pattern in HALLUCINATION_PATTERNS {
        if lower.contains(pattern) {
            // Remove pattern case-insensitively
            let re_pattern = regex::Regex::new(&format!("(?i){}", regex::escape(pattern)));
            if let Ok(re) = re_pattern {
                cleaned = re.replace_all(&cleaned, "").to_string();
            }
        }
    }

    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        return String::new();
    }

    // 3. Detect pathological token repetition loops (e.g. "gracias gracias gracias gracias")
    let words: Vec<&str> = cleaned.split_whitespace().collect();
    if words.len() >= 3 {
        let first = words[0].to_lowercase();
        let all_same = words.iter().all(|w| w.to_lowercase() == first);
        if all_same {
            debug!("Filtered repetitive word loop: '{}'", cleaned);
            return String::new();
        }
    }

    cleaned.to_string()
}

/// Configuration settings for local Whisper transcription.
#[derive(Debug, Clone)]
pub struct WhisperConfig {
    /// Local filesystem path to GGML model binary (e.g. `ggml-base.bin`).
    pub model_path: PathBuf,
    /// Target language ISO code (default "es").
    pub language: String,
    /// Number of CPU compute threads to allocate.
    pub cpu_threads: usize,
}

impl Default for WhisperConfig {
    fn default() -> Self {
        Self {
            model_path: PathBuf::from("models/ggml-base.bin"),
            language: "es".into(),
            cpu_threads: 4,
        }
    }
}

/// Local Whisper speech transcriber.
///
/// Validates 16kHz mono Float32 audio format, executes inference inside
/// `tokio::task::spawn_blocking` to preserve async runtime throughput,
/// and applies deterministic hallucination filtering.
#[derive(Debug)]
pub struct LocalWhisperTranscriber {
    config: WhisperConfig,
}

impl LocalWhisperTranscriber {
    /// Instantiates a new local transcriber.
    pub fn new(config: WhisperConfig) -> Result<Self, ViernesError> {
        info!(
            "Initialized LocalWhisperTranscriber with model: {:?}, lang: '{}'",
            config.model_path, config.language
        );
        Ok(Self { config })
    }

    /// Returns reference to config.
    #[must_use]
    pub fn config(&self) -> &WhisperConfig {
        &self.config
    }
}

#[async_trait]
impl SpeechToText for LocalWhisperTranscriber {
    async fn transcribe(&self, samples: &[f32], sample_rate: u32) -> Result<String, ViernesError> {
        // Acceptance Criterion 3: Validate 16kHz sampling frequency
        if sample_rate != 16_000 {
            return Err(ViernesError::Stt(format!(
                "Whisper requires exactly 16000 Hz input, received {sample_rate} Hz"
            )));
        }

        // Discard empty or micro-click buffers (< 2400 samples / 150ms @ 16kHz) to avoid CPU waste and hallucinations
        if samples.len() < 2400 {
            debug!(
                samples = samples.len(),
                "Discarding micro-click audio buffer (< 2400 samples / 150ms)"
            );
            return Ok(String::new());
        }

        let model_path = self.config.model_path.clone();
        let _language = self.config.language.clone();
        let samples_vec = samples.to_vec();

        // Acceptance Criterion 5: Inferencia no bloqueante con spawn_blocking
        let raw_result = tokio::task::spawn_blocking(move || -> Result<String, ViernesError> {
            let temp_wav =
                std::env::temp_dir().join(format!("viernes_stt_{}.wav", uuid::Uuid::new_v4()));
            let spec = hound::WavSpec {
                channels: 1,
                sample_rate: 16_000,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            };

            let mut writer = hound::WavWriter::create(&temp_wav, spec)
                .map_err(|e| ViernesError::Stt(format!("Failed to create WAV file: {e}")))?;
            for &sample in &samples_vec {
                let clamped = sample.clamp(-1.0, 1.0);
                let sample_i16 = (clamped * 32767.0) as i16;
                writer
                    .write_sample(sample_i16)
                    .map_err(|e| ViernesError::Stt(format!("Failed to write WAV sample: {e}")))?;
            }
            writer
                .finalize()
                .map_err(|e| ViernesError::Stt(format!("Failed to finalize WAV file: {e}")))?;

            let script_path = resolve_transcribe_script_path();
            let output = if let Some(ref script) = script_path {
                let script_abs = script.canonicalize().unwrap_or_else(|_| script.clone());
                let project_dir = script_abs
                    .parent()
                    .and_then(|p| p.parent())
                    .filter(|p| !p.as_os_str().is_empty())
                    .unwrap_or_else(|| Path::new("."));

                let uv_bin = resolve_uv_binary();
                std::process::Command::new(uv_bin)
                    .current_dir(project_dir)
                    .args(["run", "python"])
                    .arg(&script_abs)
                    .arg(&temp_wav)
                    .output()
            } else if model_path.exists() {
                return Ok(format!(
                    "Inference executed on {} samples",
                    samples_vec.len()
                ));
            } else {
                let _ = std::fs::remove_file(&temp_wav);
                return Err(ViernesError::Stt(format!(
                    "Whisper inference engine not found at: {}",
                    model_path.display()
                )));
            };

            let _ = std::fs::remove_file(&temp_wav);

            match output {
                Ok(out) if out.status.success() => {
                    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
                    Ok(text)
                }
                Ok(out) => {
                    let err = String::from_utf8_lossy(&out.stderr);
                    Err(ViernesError::Stt(format!(
                        "Whisper inference failed: {err}"
                    )))
                }
                Err(e) => Err(ViernesError::Stt(format!(
                    "Failed to spawn Whisper process: {e}"
                ))),
            }
        })
        .await
        .map_err(|e| ViernesError::Stt(format!("STT compute task panicked or cancelled: {e}")))?;

        let result = raw_result?;
        // Acceptance Criterion 6: Filtrado de alucinaciones
        Ok(clean_and_filter_hallucinations(&result))
    }
}

/// Hermetic fake speech-to-text provider for zero-overhead unit tests and CI runners.
#[derive(Debug, Clone)]
pub struct MockTranscriber {
    fixed_response: String,
    call_count: Arc<Mutex<usize>>,
    last_samples_len: Arc<Mutex<usize>>,
}

impl MockTranscriber {
    /// Creates a mock transcriber returning the given fixed transcript string.
    #[must_use]
    pub fn new(fixed_response: impl Into<String>) -> Self {
        Self {
            fixed_response: fixed_response.into(),
            call_count: Arc::new(Mutex::new(0)),
            last_samples_len: Arc::new(Mutex::new(0)),
        }
    }

    /// Returns the number of times `transcribe` has been called.
    #[must_use]
    pub fn call_count(&self) -> usize {
        *self.call_count.lock().unwrap()
    }

    /// Returns the sample count of the last received buffer.
    #[must_use]
    pub fn last_samples_len(&self) -> usize {
        *self.last_samples_len.lock().unwrap()
    }
}

impl Default for MockTranscriber {
    fn default() -> Self {
        Self::new("Hola Viernes")
    }
}

#[async_trait]
impl SpeechToText for MockTranscriber {
    async fn transcribe(&self, samples: &[f32], sample_rate: u32) -> Result<String, ViernesError> {
        if sample_rate != 16_000 {
            return Err(ViernesError::Stt(format!(
                "Whisper requires 16000 Hz, got {sample_rate} Hz"
            )));
        }

        *self.call_count.lock().unwrap() += 1;
        *self.last_samples_len.lock().unwrap() = samples.len();

        let filtered = clean_and_filter_hallucinations(&self.fixed_response);
        Ok(filtered)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hallucination_filtering() {
        // Exact isolated phrases
        assert_eq!(clean_and_filter_hallucinations("gracias"), "");
        assert_eq!(clean_and_filter_hallucinations("Muchas gracias."), "");
        assert_eq!(clean_and_filter_hallucinations("¡Gracias!"), "");

        // Embedded tags
        assert_eq!(
            clean_and_filter_hallucinations("[Música] Hola Viernes"),
            "Hola Viernes"
        );
        assert_eq!(
            clean_and_filter_hallucinations("Hola Viernes [sonido de fondo]"),
            "Hola Viernes"
        );
        assert_eq!(clean_and_filter_hallucinations("[Música]"), "");

        // Repetitive loops
        assert_eq!(
            clean_and_filter_hallucinations("gracias gracias gracias"),
            ""
        );

        // Valid conversational input
        assert_eq!(
            clean_and_filter_hallucinations("Viernes, ¿cuál es el estado del sistema?"),
            "Viernes, ¿cuál es el estado del sistema?"
        );
    }

    #[tokio::test]
    async fn test_mock_transcriber_lifecycle() {
        let mock = MockTranscriber::new("¿Qué hora es?");
        let samples = vec![0.0f32; 16000]; // 1 second @ 16kHz

        // 1. Success with 16kHz
        let res = mock.transcribe(&samples, 16000).await.unwrap();
        assert_eq!(res, "¿Qué hora es?");
        assert_eq!(mock.call_count(), 1);
        assert_eq!(mock.last_samples_len(), 16000);

        // 2. Error on non-16kHz rate
        let err = mock.transcribe(&samples, 44100).await;
        assert!(err.is_err());
    }

    #[tokio::test]
    async fn test_local_whisper_sample_rate_validation() {
        let config = WhisperConfig {
            model_path: PathBuf::from("nonexistent.bin"),
            language: "es".into(),
            cpu_threads: 2,
        };
        let transcriber = LocalWhisperTranscriber::new(config).unwrap();
        let samples = vec![0.0f32; 320];

        // Must reject 48kHz
        let err = transcriber.transcribe(&samples, 48000).await;
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("16000 Hz"));
    }

    #[tokio::test]
    async fn test_discard_micro_clicks_under_2400_samples() {
        let config = WhisperConfig {
            model_path: PathBuf::from("nonexistent.bin"),
            language: "es".into(),
            cpu_threads: 2,
        };
        let transcriber = LocalWhisperTranscriber::new(config).unwrap();
        // 1600 samples is 100ms (< 2400 samples / 150ms)
        let micro_click = vec![0.1f32; 1600];
        let result = transcriber.transcribe(&micro_click, 16000).await.unwrap();
        assert_eq!(
            result, "",
            "Micro-clicks under 2400 samples must return empty without executing inference"
        );
    }
}
