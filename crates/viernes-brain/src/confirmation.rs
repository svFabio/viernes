//! Voice confirmation for destructive commands and system failure notifications.

use regex::Regex;
use std::time::Duration;
use tokio::sync::oneshot;
use tracing::{info, warn};

use crate::error::BrainError;

/// Confirmation timeout threshold (10 seconds).
pub const CONFIRMATION_TIMEOUT: Duration = Duration::from_secs(10);

/// Classifier analyzing tool calls and commands to identify destructive or high-risk actions.
pub struct RiskClassifier {
    destructive_patterns: Vec<Regex>,
    affirmative_regex: Regex,
    negative_regex: Regex,
}

impl Default for RiskClassifier {
    fn default() -> Self {
        let raw_patterns = vec![
            r"(?i)\brm\s+(-[a-zA-Z]*r[a-zA-Z]*f*|-rf)\b",
            r"(?i)\bmkfs\b",
            r"(?i)\bdd\s+if=",
            r"(?i)\b(format|fdisk|parted)\b",
            r"(?i)\bgit\s+reset\s+--hard\b",
            r"(?i)\bgit\s+clean\s+-fdx?\b",
            r"(?i)\b(drop\s+database|drop\s+table|truncate\s+table)\b",
            r"(?i)\bkill\s+-9\b",
            r"(?i)\b(shutdown|reboot|poweroff)\b",
        ];

        let destructive_patterns = raw_patterns
            .into_iter()
            .filter_map(|p| Regex::new(p).ok())
            .collect();

        let affirmative_regex = Regex::new(
            r"(?i)\b(s[ií]|afirmativo|procede|dale|confirmado|hazlo|ejecuta|adelante|yes)\b",
        )
        .expect("valid regex");

        let negative_regex =
            Regex::new(r"(?i)\b(no|cancela|aborta|det[eé]n|rechaza|cancelar|abort|negativo)\b")
                .expect("valid regex");

        Self {
            destructive_patterns,
            affirmative_regex,
            negative_regex,
        }
    }
}

impl RiskClassifier {
    /// Evaluate whether a command line or operation is classified as destructive.
    #[must_use]
    pub fn is_destructive(&self, command: &str) -> bool {
        let trimmed = command.trim();
        self.destructive_patterns
            .iter()
            .any(|re| re.is_match(trimmed))
    }

    /// Evaluate user spoken response to determine affirmative, negative, or ambiguous intent.
    #[must_use]
    pub fn parse_confirmation_response(&self, spoken_text: &str) -> Option<bool> {
        let text = spoken_text.trim();
        if self.negative_regex.is_match(text) {
            Some(false)
        } else if self.affirmative_regex.is_match(text) {
            Some(true)
        } else {
            None
        }
    }

    /// Generate clear spoken notification message for an infrastructure failure.
    #[must_use]
    pub fn format_failure_notification(subsystem: &str, details: &str) -> String {
        match subsystem {
            "network" => format!("Alerta de red: Se ha interrumpido la conexión. {details}."),
            "microphone" | "audio_input" => {
                format!("Alerta de audio: No se detecta señal en el micrófono. {details}.")
            }
            "api" | "elevenlabs" | "tts" => {
                format!("Alerta de servicio: Fallo en el proveedor de síntesis. {details}.")
            }
            _ => format!("Alerta del sistema: Se produjo una anomalía en {subsystem}. {details}."),
        }
    }
}

/// Confirmation coordinator managing pending authorization requests and 10s cancellation timers.
pub struct ConfirmationManager {
    classifier: RiskClassifier,
}

impl Default for ConfirmationManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfirmationManager {
    /// Create a new confirmation manager instance.
    #[must_use]
    pub fn new() -> Self {
        Self {
            classifier: RiskClassifier::default(),
        }
    }

    /// Get reference to underlying risk classifier.
    #[must_use]
    pub fn classifier(&self) -> &RiskClassifier {
        &self.classifier
    }

    /// Wait for user vocal confirmation within 10s timeout window.
    pub async fn await_confirmation(
        &self,
        mut response_rx: oneshot::Receiver<String>,
    ) -> Result<bool, BrainError> {
        info!("Awaiting explicit user voice confirmation (10s timeout active)");

        match tokio::time::timeout(CONFIRMATION_TIMEOUT, &mut response_rx).await {
            Ok(Ok(response_text)) => {
                if let Some(confirmed) = self.classifier.parse_confirmation_response(&response_text)
                {
                    if confirmed {
                        info!("Destructive operation explicitly confirmed by user");
                        Ok(true)
                    } else {
                        warn!("Destructive operation explicitly aborted by user");
                        Ok(false)
                    }
                } else {
                    warn!(response = %response_text, "Ambiguous response treated as cancellation for safety");
                    Ok(false)
                }
            }
            Ok(Err(_)) => {
                warn!("Confirmation response channel dropped; canceling operation");
                Ok(false)
            }
            Err(_) => {
                warn!("Confirmation window timed out after 10s; canceling operation by default");
                Err(BrainError::ConfirmationTimeout)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_destructive_command_detection() {
        let classifier = RiskClassifier::default();

        assert!(classifier.is_destructive("rm -rf /tmp/data"));
        assert!(classifier.is_destructive("rm -r -f ./build"));
        assert!(classifier.is_destructive("mkfs.ext4 /dev/sda1"));
        assert!(classifier.is_destructive("dd if=/dev/zero of=/dev/sdb"));
        assert!(classifier.is_destructive("git reset --hard HEAD~1"));
        assert!(classifier.is_destructive("DROP DATABASE production;"));

        // Safe operations
        assert!(!classifier.is_destructive("ls -la"));
        assert!(!classifier.is_destructive("cargo test"));
        assert!(!classifier.is_destructive("git status"));
    }

    #[test]
    fn test_confirmation_response_mapping() {
        let classifier = RiskClassifier::default();

        assert_eq!(
            classifier.parse_confirmation_response("Sí, procede"),
            Some(true)
        );
        assert_eq!(
            classifier.parse_confirmation_response("Afirmativo, hazlo"),
            Some(true)
        );
        assert_eq!(
            classifier.parse_confirmation_response("Dale adelante"),
            Some(true)
        );

        assert_eq!(
            classifier.parse_confirmation_response("No, cancela"),
            Some(false)
        );
        assert_eq!(
            classifier.parse_confirmation_response("Aborta la operación"),
            Some(false)
        );
        assert_eq!(
            classifier.parse_confirmation_response("Detén eso"),
            Some(false)
        );

        assert_eq!(
            classifier.parse_confirmation_response("¿Qué hora es?"),
            None
        );
    }

    #[tokio::test]
    async fn test_await_confirmation_affirmative_flow() {
        let mgr = ConfirmationManager::new();
        let (tx, rx) = oneshot::channel();

        let handle = tokio::spawn(async move { mgr.await_confirmation(rx).await });

        tx.send("Afirmativo, procede".into()).unwrap();
        let result = handle.await.unwrap().unwrap();
        assert!(result);
    }

    #[tokio::test]
    async fn test_await_confirmation_rejection_flow() {
        let mgr = ConfirmationManager::new();
        let (tx, rx) = oneshot::channel();

        let handle = tokio::spawn(async move { mgr.await_confirmation(rx).await });

        tx.send("No, cancela inmediatamente".into()).unwrap();
        let result = handle.await.unwrap().unwrap();
        assert!(!result);
    }

    #[tokio::test]
    async fn test_await_confirmation_timeout_flow() {
        let mgr = ConfirmationManager::new();
        let (_tx, rx) = oneshot::channel();

        // Testing timeout behavior with short window test
        let res = tokio::time::timeout(Duration::from_millis(50), mgr.await_confirmation(rx)).await;
        assert!(res.is_err(), "Should have timed out");
    }
}
