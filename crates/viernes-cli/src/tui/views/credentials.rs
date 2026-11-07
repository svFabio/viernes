//! Tab 2: ElevenLabs credentials pool, voice configuration, and audition view.

use std::fs::OpenOptions;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame,
};
use regex::Regex;
use viernes_core::config::ViernesConfig;

/// Alias for the primary resilient Text-to-Speech engine in Viernes.
pub type ViernesTts = viernes_tts::resilient::ResilientTtsEngine;

/// Supported ElevenLabs neural speech models.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ElevenLabsModel {
    /// Flash v2.5 (Fastest real-time synthesis).
    #[default]
    FlashV2_5,
    /// Turbo v2.5 (Higher emotional fidelity).
    TurboV2_5,
}

impl ElevenLabsModel {
    /// Return the model ID string expected by the API.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::FlashV2_5 => "eleven_flash_v2_5",
            Self::TurboV2_5 => "eleven_turbo_v2_5",
        }
    }

    /// Parse model ID string into enum variant.
    #[must_use]
    pub fn from_str_id(s: &str) -> Self {
        if s.contains("turbo") {
            Self::TurboV2_5
        } else {
            Self::FlashV2_5
        }
    }
}

/// Active input focus/mode within the credentials view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CredentialsFocus {
    /// Key pool list navigation.
    #[default]
    KeyList,
    /// Typing a new API key.
    AddingKey,
    /// Editing Voice ID text input.
    VoiceIdInput,
    /// Model selector toggle.
    ModelSelector,
}

/// Action signal emitted by the credentials view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CredentialsAction {
    /// No global transition.
    None,
    /// Triggered voice audition test.
    TestVoiceTriggered,
    /// Changes saved to disk successfully.
    SavedToDisk,
}

/// Credentials and voice settings manager view for Tab 2.
#[derive(Debug, Clone)]
pub struct CredentialsView {
    /// Pool of configured ElevenLabs API keys.
    pub api_keys: Vec<String>,
    /// Selected index in key list.
    pub selected_key_index: usize,
    /// Configured Voice ID.
    pub voice_id: String,
    /// Configured neural model.
    pub model: ElevenLabsModel,
    /// Current input focus.
    pub focus: CredentialsFocus,
    /// Temporary buffer during text input.
    pub input_buffer: String,
    /// Transient error message from input validation.
    pub error_message: Option<String>,
    /// Status notification banner.
    pub status_message: Option<String>,
    /// Whether a voice test is currently playing.
    pub is_auditioning: bool,
    /// Whether in-memory state has unsaved modifications.
    pub has_unsaved_changes: bool,
    /// Confirmation dialog state for deleting an API key.
    pub show_delete_confirm: bool,
    /// Confirmation dialog state for discarding unsaved changes.
    pub show_discard_confirm: bool,
    /// Path to target `.env` file.
    pub env_path: PathBuf,
}

impl CredentialsView {
    /// Create view initialized from `ViernesConfig` and default `~/.config/viernes/.env` path.
    pub fn new(config: Option<ViernesConfig>) -> Self {
        let env_path = {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
            PathBuf::from(home).join(".config/viernes/.env")
        };
        let mut view = Self::from_path(Some(env_path));
        if let Some(cfg) = config {
            if !cfg.elevenlabs_voice_id.is_empty() {
                view.voice_id = cfg.elevenlabs_voice_id;
            }
            if !cfg.elevenlabs_api_keys.is_empty() {
                view.api_keys = cfg.elevenlabs_api_keys;
            } else if !cfg.elevenlabs_api_key.is_empty() {
                view.api_keys = vec![cfg.elevenlabs_api_key];
            }
        }
        view
    }

    /// Create view initialized from `~/.config/viernes/.env` or specified path.
    pub fn from_path(custom_env_path: Option<PathBuf>) -> Self {
        let env_path = custom_env_path.unwrap_or_else(|| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
            PathBuf::from(home).join(".config/viernes/.env")
        });

        let mut view = Self {
            api_keys: Vec::new(),
            selected_key_index: 0,
            voice_id: "cgSgspJ2msm6clMCkdW9".into(), // Default Jessica voice
            model: ElevenLabsModel::FlashV2_5,
            focus: CredentialsFocus::KeyList,
            input_buffer: String::new(),
            error_message: None,
            status_message: None,
            is_auditioning: false,
            has_unsaved_changes: false,
            show_delete_confirm: false,
            show_discard_confirm: false,
            env_path,
        };

        view.load_from_disk();
        view
    }

    /// Read settings from disk file if present.
    pub fn load_from_disk(&mut self) {
        if !self.env_path.exists() {
            return;
        }

        if let Ok(content) = std::fs::read_to_string(&self.env_path) {
            let mut keys = Vec::new();
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with('#') || trimmed.is_empty() {
                    continue;
                }
                if let Some((k, v)) = trimmed.split_once('=') {
                    let key_name = k.trim();
                    let val = v.trim().trim_matches('"').trim_matches('\'');
                    match key_name {
                        "ELEVENLABS_VOICE_ID" => {
                            if !val.is_empty() {
                                self.voice_id = val.to_string();
                            }
                        }
                        "ELEVENLABS_MODEL_ID" => {
                            self.model = ElevenLabsModel::from_str_id(val);
                        }
                        "ELEVENLABS_API_KEYS" => {
                            let pool_keys: Vec<String> = val
                                .split(',')
                                .map(|s| s.trim().to_string())
                                .filter(|s| !s.is_empty())
                                .collect();
                            if !pool_keys.is_empty() {
                                keys = pool_keys;
                            }
                        }
                        "ELEVENLABS_API_KEY" if keys.is_empty() && !val.is_empty() => {
                            keys.push(val.to_string());
                        }
                        _ => {}
                    }
                }
            }
            if !keys.is_empty() {
                self.api_keys = keys;
            }
        }
    }

    /// Validate and add a new API key to the in-memory pool.
    pub fn add_api_key(&mut self, key: &str) -> Result<(), String> {
        validate_api_key(key)?;
        let trimmed = key.trim().to_string();
        if self.api_keys.iter().any(|k| k == &trimmed) {
            return Err("API key already exists in pool".into());
        }
        self.api_keys.push(trimmed);
        self.has_unsaved_changes = true;
        self.error_message = None;
        self.status_message = Some("API key added to pool".into());
        Ok(())
    }

    /// Delete selected API key.
    pub fn delete_selected_key(&mut self) -> Result<(), String> {
        if self.api_keys.is_empty() {
            return Err("No keys to delete".into());
        }
        if self.selected_key_index < self.api_keys.len() {
            self.api_keys.remove(self.selected_key_index);
            if self.selected_key_index > 0 && self.selected_key_index >= self.api_keys.len() {
                self.selected_key_index = self.api_keys.len().saturating_sub(1);
            }
            self.has_unsaved_changes = true;
            self.status_message = Some("API key removed from pool".into());
        }
        Ok(())
    }

    /// Update configured Voice ID with validation.
    pub fn update_voice_id(&mut self, new_id: &str) -> Result<(), String> {
        validate_voice_id(new_id)?;
        self.voice_id = new_id.trim().to_string();
        self.has_unsaved_changes = true;
        self.error_message = None;
        self.status_message = Some("Voice ID updated".into());
        Ok(())
    }

    /// Toggle model between Flash and Turbo.
    pub fn toggle_model(&mut self) {
        self.model = match self.model {
            ElevenLabsModel::FlashV2_5 => ElevenLabsModel::TurboV2_5,
            ElevenLabsModel::TurboV2_5 => ElevenLabsModel::FlashV2_5,
        };
        self.has_unsaved_changes = true;
        self.status_message = Some(format!("Model switched to {}", self.model.as_str()));
    }

    /// Atomically persist current credentials to disk with restricted permissions 0600.
    pub fn save_to_disk(&mut self) -> Result<(), String> {
        if let Some(parent) = self.env_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let primary_key = self.api_keys.first().cloned().unwrap_or_default();
        let pool_str = self.api_keys.join(",");

        let content = format!(
            "# Viernes Voice Assistant Environment Configuration\n\
             ELEVENLABS_API_KEY=\"{}\"\n\
             ELEVENLABS_API_KEYS=\"{}\"\n\
             ELEVENLABS_VOICE_ID=\"{}\"\n\
             ELEVENLABS_MODEL_ID=\"{}\"\n",
            primary_key,
            pool_str,
            self.voice_id,
            self.model.as_str()
        );

        let temp_path = self.env_path.with_extension("tmp");
        {
            let mut file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&temp_path)
                .map_err(|e| format!("Failed to create temporary file: {e}"))?;

            // Set POSIX 0600 permissions
            let mut permissions = file
                .metadata()
                .map_err(|e| format!("Failed to read metadata: {e}"))?
                .permissions();
            permissions.set_mode(0o600);
            file.set_permissions(permissions)
                .map_err(|e| format!("Failed to set 0600 permissions: {e}"))?;

            file.write_all(content.as_bytes())
                .map_err(|e| format!("Failed to write content: {e}"))?;
            file.sync_all()
                .map_err(|e| format!("Failed to sync temporary file: {e}"))?;
        }

        std::fs::rename(&temp_path, &self.env_path)
            .map_err(|e| format!("Atomic rename failed: {e}"))?;

        self.has_unsaved_changes = false;
        self.status_message = Some("Credentials saved to ~/.config/viernes/.env (0600)".into());
        Ok(())
    }

    /// Trigger a test audition phrase synthesizing via [`ViernesTts`] and playing via audio player.
    pub async fn test_audition(
        &mut self,
        phrase: Option<&str>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.is_auditioning = true;
        let test_phrase =
            phrase.unwrap_or("A la orden, Señor. Viernes en línea y lista para operar.");

        let tts = ViernesTts::from_env();
        let mut player = viernes_tts::player::StreamingAudioPlayer::create_resilient_player(None);

        let audio_bytes = tts
            .synthesize_resilient(test_phrase)
            .await
            .map_err(|e| format!("TTS synthesis error: {e}"))?;

        player
            .enqueue_chunk(audio_bytes)
            .await
            .map_err(|e| format!("Audio player enqueue error: {e}"))?;

        self.is_auditioning = false;
        self.status_message = Some("Audición completada exitosamente".into());
        Ok(())
    }

    /// Process keyboard input for Tab 2 navigation and editing.
    pub fn handle_key(&mut self, key: KeyEvent) -> CredentialsAction {
        // Modal dialog handling
        if self.show_delete_confirm {
            match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                    self.show_delete_confirm = false;
                    let _ = self.delete_selected_key();
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                    self.show_delete_confirm = false;
                    self.status_message = Some("Deletion cancelled".into());
                }
                _ => {}
            }
            return CredentialsAction::None;
        }

        if self.show_discard_confirm {
            match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                    self.show_discard_confirm = false;
                    self.load_from_disk();
                    self.has_unsaved_changes = false;
                    self.status_message = Some("Unsaved changes discarded".into());
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                    self.show_discard_confirm = false;
                }
                _ => {}
            }
            return CredentialsAction::None;
        }

        // Active text input modes
        match self.focus {
            CredentialsFocus::AddingKey => match key.code {
                KeyCode::Enter => {
                    let candidate = std::mem::take(&mut self.input_buffer);
                    match self.add_api_key(&candidate) {
                        Ok(()) => {
                            self.focus = CredentialsFocus::KeyList;
                        }
                        Err(err) => {
                            self.input_buffer = candidate;
                            self.error_message = Some(err);
                        }
                    }
                }
                KeyCode::Esc => {
                    self.input_buffer.clear();
                    self.error_message = None;
                    self.focus = CredentialsFocus::KeyList;
                }
                KeyCode::Backspace => {
                    self.input_buffer.pop();
                }
                KeyCode::Char(c) => {
                    self.input_buffer.push(c);
                }
                _ => {}
            },
            CredentialsFocus::VoiceIdInput => match key.code {
                KeyCode::Enter => {
                    let candidate = std::mem::take(&mut self.input_buffer);
                    match self.update_voice_id(&candidate) {
                        Ok(()) => {
                            self.focus = CredentialsFocus::KeyList;
                        }
                        Err(err) => {
                            self.input_buffer = candidate;
                            self.error_message = Some(err);
                        }
                    }
                }
                KeyCode::Esc => {
                    self.input_buffer.clear();
                    self.error_message = None;
                    self.focus = CredentialsFocus::KeyList;
                }
                KeyCode::Backspace => {
                    self.input_buffer.pop();
                }
                KeyCode::Char(c) => {
                    self.input_buffer.push(c);
                }
                _ => {}
            },
            CredentialsFocus::KeyList | CredentialsFocus::ModelSelector => match key.code {
                // Navigation between keys
                KeyCode::Up => {
                    if self.selected_key_index > 0 {
                        self.selected_key_index -= 1;
                    }
                }
                KeyCode::Down => {
                    if !self.api_keys.is_empty()
                        && self.selected_key_index + 1 < self.api_keys.len()
                    {
                        self.selected_key_index += 1;
                    }
                }
                // Actions
                KeyCode::Char('a') | KeyCode::Char('+') => {
                    self.focus = CredentialsFocus::AddingKey;
                    self.input_buffer.clear();
                    self.error_message = None;
                }
                KeyCode::Char('d') => {
                    if !self.api_keys.is_empty() {
                        self.show_delete_confirm = true;
                    }
                }
                KeyCode::Char('v') => {
                    self.focus = CredentialsFocus::VoiceIdInput;
                    self.input_buffer = self.voice_id.clone();
                    self.error_message = None;
                }
                KeyCode::Char('m') => {
                    self.toggle_model();
                }
                KeyCode::Char('s') | KeyCode::Char('S') => {
                    if let Err(e) = self.save_to_disk() {
                        self.error_message = Some(e);
                    } else {
                        return CredentialsAction::SavedToDisk;
                    }
                }
                KeyCode::Char('t') | KeyCode::Char('T') => {
                    self.is_auditioning = true;
                    self.status_message = Some("Audition test requested...".into());
                    return CredentialsAction::TestVoiceTriggered;
                }
                KeyCode::Esc if self.has_unsaved_changes => {
                    self.show_discard_confirm = true;
                }
                _ => {}
            },
        }

        CredentialsAction::None
    }

    /// Render Tab 2 UI layout.
    pub fn render(&self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(9), // Key Pool list
                Constraint::Length(4), // Add Key input
                Constraint::Length(5), // Voice & Model config
                Constraint::Length(3), // Action bar
                Constraint::Min(2),    // Status & errors
            ])
            .split(area);

        // 1. Key Pool List
        let items: Vec<ListItem> = if self.api_keys.is_empty() {
            vec![ListItem::new(Span::styled(
                "  (No API keys configured. Press [a] to add a key)",
                Style::default().fg(Color::DarkGray),
            ))]
        } else {
            self.api_keys
                .iter()
                .enumerate()
                .map(|(idx, key)| {
                    let is_selected = idx == self.selected_key_index;
                    let masked = mask_api_key(key);
                    let active_tag = if idx == 0 {
                        " [Primary/Active]"
                    } else {
                        " [Failover]"
                    };
                    let style = if is_selected {
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::White)
                    };
                    ListItem::new(Line::from(vec![
                        Span::styled(
                            format!("  [{}] ", idx + 1),
                            Style::default().fg(Color::Yellow),
                        ),
                        Span::styled(masked, style),
                        Span::styled(active_tag, Style::default().fg(Color::Green)),
                        Span::raw(" (Cuota: OK)"),
                    ]))
                })
                .collect()
        };

        let key_block = Block::default()
            .borders(Borders::ALL)
            .title(" [2] ElevenLabs API Key Pool ")
            .style(Style::default().fg(Color::Cyan));
        f.render_widget(List::new(items).block(key_block), chunks[0]);

        // 2. Add Key Input Buffer
        let add_key_text = if self.focus == CredentialsFocus::AddingKey {
            format!("> {}_", self.input_buffer)
        } else {
            "Press [a] to add new ElevenLabs API Key".to_string()
        };
        let add_key_style = if self.focus == CredentialsFocus::AddingKey {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let add_block = Block::default()
            .borders(Borders::ALL)
            .title(" [+] Agregar API Key (sk_...) ")
            .style(if self.focus == CredentialsFocus::AddingKey {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::Gray)
            });
        f.render_widget(
            Paragraph::new(add_key_text)
                .style(add_key_style)
                .block(add_block),
            chunks[1],
        );

        // 3. Voice ID & Model Selection
        let voice_line = Line::from(vec![
            Span::styled(" Voice ID: ", Style::default().fg(Color::White)),
            Span::styled(
                format!("[{}] ", self.voice_id),
                if self.focus == CredentialsFocus::VoiceIdInput {
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Green)
                },
            ),
            Span::styled(
                " (Press [v] to edit)  |  ",
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled("Modelo: ", Style::default().fg(Color::White)),
            Span::styled(
                format!("(•) {} ", self.model.as_str()),
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "(Press [m] to switch)",
                Style::default().fg(Color::DarkGray),
            ),
        ]);

        let voice_block = Block::default()
            .borders(Borders::ALL)
            .title(" Parámetros de Voz & Síntesis ")
            .style(Style::default().fg(Color::White));
        f.render_widget(
            Paragraph::new(vec![voice_line]).block(voice_block),
            chunks[2],
        );

        // 4. Action Bar
        let mut action_spans = vec![
            Span::styled(
                " [a] ",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("Agregar Key  "),
            Span::styled(
                " [d] ",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            Span::raw("Eliminar  "),
            Span::styled(
                " [v] ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("Voice ID  "),
            Span::styled(
                " [m] ",
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("Modelo  "),
            Span::styled(
                " [T] ",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("Probar Voz (Audición)  "),
            Span::styled(
                " [S] ",
                Style::default()
                    .fg(Color::Blue)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("Guardar Cambios"),
        ];
        if self.has_unsaved_changes {
            action_spans.push(Span::styled(
                "  *CAMBIOS PENDIENTES*",
                Style::default()
                    .fg(Color::LightRed)
                    .add_modifier(Modifier::BOLD),
            ));
        }
        let action_block = Block::default()
            .borders(Borders::ALL)
            .title(" Atajos de Acción ");
        f.render_widget(
            Paragraph::new(Line::from(action_spans)).block(action_block),
            chunks[3],
        );

        // 5. Status & Error Feedback
        let mut feedback_lines = Vec::new();
        if let Some(ref err) = self.error_message {
            feedback_lines.push(Line::from(vec![
                Span::styled(
                    "ERROR: ",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ),
                Span::styled(err, Style::default().fg(Color::LightRed)),
            ]));
        }
        if let Some(ref status) = self.status_message {
            feedback_lines.push(Line::from(vec![
                Span::styled(
                    "ESTADO: ",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(status, Style::default().fg(Color::LightGreen)),
            ]));
        }
        f.render_widget(
            Paragraph::new(feedback_lines).wrap(Wrap { trim: true }),
            chunks[4],
        );

        // Delete Confirmation Modal
        if self.show_delete_confirm {
            render_modal(
                f,
                area,
                " Confirmar Eliminación de API Key ",
                "¿Está seguro que desea eliminar la clave seleccionada?\n\nPresione [y/Enter] para confirmar o [n/Esc] para cancelar.",
                Color::Red,
            );
        }

        // Discard Confirmation Modal
        if self.show_discard_confirm {
            render_modal(
                f,
                area,
                " Cambios sin guardar ",
                "Hay cambios pendientes de guardar en disco.\n\n¿Desea descartarlos? Presione [y/Enter] para descartar o [n/Esc] para volver.",
                Color::Yellow,
            );
        }
    }
}

/// Mask an ElevenLabs API key showing prefix and suffix.
#[must_use]
pub fn mask_api_key(key: &str) -> String {
    let trimmed = key.trim();
    if trimmed.len() <= 10 {
        return "sk_***".into();
    }
    let prefix = &trimmed[..6.min(trimmed.len())];
    let suffix = &trimmed[trimmed.len().saturating_sub(4)..];
    format!("{prefix}...{suffix}")
}

/// Validate that an ElevenLabs API key matches `sk_[a-zA-Z0-9]{32,}`.
pub fn validate_api_key(key: &str) -> Result<(), String> {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return Err("API key cannot be empty".into());
    }
    let re =
        Regex::new(r"^sk_[a-zA-Z0-9]{32,}$").map_err(|e| format!("Internal regex error: {e}"))?;
    if !re.is_match(trimmed) {
        return Err(
            "Invalid format: Must start with 'sk_' followed by >= 32 alphanumeric characters"
                .into(),
        );
    }
    Ok(())
}

/// Validate that a Voice ID has no spaces/special chars and is at least 15 characters long.
pub fn validate_voice_id(id: &str) -> Result<(), String> {
    let trimmed = id.trim();
    if trimmed.len() < 15 {
        return Err("Voice ID must be at least 15 characters long".into());
    }
    let re =
        Regex::new(r"^[a-zA-Z0-9_-]{15,}$").map_err(|e| format!("Internal regex error: {e}"))?;
    if !re.is_match(trimmed) {
        return Err("Voice ID cannot contain spaces or invalid special characters".into());
    }
    Ok(())
}

/// Helper to render centered modal dialog box.
fn render_modal(f: &mut Frame, area: Rect, title: &str, body: &str, border_color: Color) {
    let modal_area = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(30),
            Constraint::Length(8),
            Constraint::Percentage(30),
        ])
        .split(area)[1];

    let modal_box = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20),
            Constraint::Percentage(60),
            Constraint::Percentage(20),
        ])
        .split(modal_area)[1];

    f.render_widget(Clear, modal_box);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .title_alignment(Alignment::Center)
        .style(Style::default().fg(border_color));
    let text = Paragraph::new(body)
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true })
        .block(block);
    f.render_widget(text, modal_box);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_mask_api_key_formatting() {
        let key = "sk_425e981b3a1234567890abcdef123456";
        let masked = mask_api_key(key);
        assert_eq!(masked, "sk_425...3456");

        let short_key = "sk_abc";
        assert_eq!(mask_api_key(short_key), "sk_***");
    }

    #[test]
    fn test_validate_api_key_rules() {
        let valid = "sk_425e981b3a1234567890abcdef123456";
        assert!(validate_api_key(valid).is_ok());

        let invalid_no_prefix = "425e981b3a1234567890abcdef123456";
        assert!(validate_api_key(invalid_no_prefix).is_err());

        let invalid_short = "sk_short";
        assert!(validate_api_key(invalid_short).is_err());

        let invalid_spaces = "sk_425e981b3a 1234567890abcdef123456";
        assert!(validate_api_key(invalid_spaces).is_err());
    }

    #[test]
    fn test_validate_voice_id_rules() {
        let valid = "cgSgspJ2msm6clMCkdW9";
        assert!(validate_voice_id(valid).is_ok());

        let invalid_short = "short_id";
        assert!(validate_voice_id(invalid_short).is_err());

        let invalid_spaces = "cgSgspJ2 msMCkdW9";
        assert!(validate_voice_id(invalid_spaces).is_err());
    }

    #[test]
    fn test_credentials_parsing_from_env_file() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let env_file = dir.path().join(".env");
        let content = r#"
# Comment line
ELEVENLABS_VOICE_ID="test_voice_id_12345"
ELEVENLABS_MODEL_ID="eleven_turbo_v2_5"
ELEVENLABS_API_KEYS="sk_11111111111111111111111111111111,sk_22222222222222222222222222222222"
"#;
        std::fs::write(&env_file, content)?;

        let view = CredentialsView::from_path(Some(env_file));
        assert_eq!(view.voice_id, "test_voice_id_12345");
        assert_eq!(view.model, ElevenLabsModel::TurboV2_5);
        assert_eq!(view.api_keys.len(), 2);
        assert_eq!(view.api_keys[0], "sk_11111111111111111111111111111111");
        assert_eq!(view.api_keys[1], "sk_22222222222222222222222222222222");
        Ok(())
    }

    #[test]
    fn test_model_switching() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let env_file = dir.path().join(".env");
        let mut view = CredentialsView::from_path(Some(env_file));
        assert_eq!(view.model, ElevenLabsModel::FlashV2_5);
        view.toggle_model();
        assert_eq!(view.model, ElevenLabsModel::TurboV2_5);
        view.toggle_model();
        assert_eq!(view.model, ElevenLabsModel::FlashV2_5);
        Ok(())
    }

    #[test]
    fn test_atomic_persistence_with_0600_permissions() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let env_file = dir.path().join(".env");
        let mut view = CredentialsView::from_path(Some(env_file.clone()));

        let key1 = "sk_425e981b3a1234567890abcdef123456";
        let key2 = "sk_81b3a981b3a1234567890abcdef654321";
        view.add_api_key(key1)?;
        view.add_api_key(key2)?;
        view.update_voice_id("21m00Tcm4TlvDq8ikWAM")?;
        view.model = ElevenLabsModel::TurboV2_5;

        view.save_to_disk()?;
        assert!(env_file.exists());

        // Verify Unix permissions 0600
        let metadata = std::fs::metadata(&env_file)?;
        let permissions = metadata.permissions();
        assert_eq!(permissions.mode() & 0o777, 0o600);

        // Verify content reload
        let reloaded = CredentialsView::from_path(Some(env_file));
        assert_eq!(reloaded.api_keys.len(), 2);
        assert_eq!(reloaded.api_keys[0], key1);
        assert_eq!(reloaded.api_keys[1], key2);
        assert_eq!(reloaded.voice_id, "21m00Tcm4TlvDq8ikWAM");
        assert_eq!(reloaded.model, ElevenLabsModel::TurboV2_5);
        Ok(())
    }

    #[test]
    fn test_delete_key_flow() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let env_file = dir.path().join(".env");
        let mut view = CredentialsView::from_path(Some(env_file));
        let key = "sk_425e981b3a1234567890abcdef123456";
        view.add_api_key(key)?;
        assert_eq!(view.api_keys.len(), 1);

        view.delete_selected_key()?;
        assert_eq!(view.api_keys.len(), 0);
        Ok(())
    }
}
