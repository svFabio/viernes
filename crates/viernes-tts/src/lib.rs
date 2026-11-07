//! Text-to-speech synthesis engines, multi-key quota management, and streaming audio playback.
//!
//! # Architecture Overview
//!
//! - **KeyPool & ElevenLabs**: Lock-free API key rotation (`AtomicUsize`) across free tiers with `wiremock` tests.
//! - **Edge-TTS**: 100% native asynchronous WebSocket synthesis with binary MP3 frame extraction.
//! - **StreamingAudioPlayer**: Non-blocking audio playback powered by `rodio` and in-memory MP3 decoding.
//! - **SilentAudioSink**: Hermetic mock sink for headless CI environments.
//! - **Barge-in**: Sub-80ms atomic interruption kill-switch with acoustic echo grace windows.
//! - **TempStorageManager**: Restrictive `0700` temporary directory management, 2-hour TTL eviction, and 100MB quota.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod barge_in;
pub mod edge;
pub mod elevenlabs;
pub mod error;
pub mod key_pool;
pub mod player;
pub mod resilient;
pub mod temp_storage;

pub use barge_in::BargeInController;
pub use edge::EdgeTtsClient;
pub use elevenlabs::ElevenLabsClient;
pub use error::TtsError;
pub use key_pool::KeyPool;
pub use player::{SilentAudioSink, StreamingAudioPlayer};
pub use resilient::ResilientTtsEngine;
pub use temp_storage::TempStorageManager;
