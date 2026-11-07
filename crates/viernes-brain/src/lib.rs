//! Viernes Brain: Antigravity NDJSON client, session pointer, conversational fillers,
//! sentence tokenizer, and confirmation pipeline.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod client;
pub mod confirmation;
pub mod error;
pub mod fillers;
pub mod session;
pub mod tokenizer;

pub use client::{AntigravityClient, BrainEvent, MockAntigravityServer};
pub use confirmation::{ConfirmationManager, RiskClassifier};
pub use error::BrainError;
pub use fillers::{FillerCacheManager, FillerCollection, FillerDispatcher};
pub use session::{SessionPointerManager, SessionState};
pub use tokenizer::SentenceTokenizer;
