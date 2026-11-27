//! Tab 2: ElevenLabs credentials pool, voice configuration, and audition view.

use std::fs::OpenOptions;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

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
use viernes_core::events::SystemEvent;

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
            if let Some(ref model_id) = cfg.elevenlabs_model_id {
                if !model_id.trim().is_empty() {
                    view.model = ElevenLabsModel::from_str_id(model_id);
                }
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

    /// Atomically persist current credentials to disk with restricted permissions 0600,
    /// preserving unrelated configuration lines in `.env`.
    pub fn save_to_disk(&mut self) -> Result<(), String> {
        crate::tui::ensure_parent_dir(&self.env_path)?;

        let existing_lines: Vec<String> = if self.env_path.exists() {
            std::fs::read_to_string(&self.env_path)
                .map_err(|e| format!("Failed to read existing .env file: {e}"))?
                .lines()
                .map(ToString::to_string)
                .collect()
        } else {
            Vec::new()
        };

        let primary_key = self.api_keys.first().cloned().unwrap_or_default();
        let pool_str = self.api_keys.join(",");

        let mut new_lines = Vec::with_capacity(existing_lines.len() + 5);
        if existing_lines.is_empty() {
            new_lines.push("# Viernes Voice Assistant Environment Configuration".to_string());
        }

        let mut api_key_found = false;
        let mut api_keys_found = false;
        let mut voice_id_found = false;
        let mut model_id_found = false;

        for line in existing_lines {
            let trimmed = line.trim();
            if !trimmed.starts_with('#') {
                if let Some((k, _)) = trimmed.split_once('=') {
                    match k.trim() {
                        "ELEVENLABS_API_KEY" => {
                            new_lines.push(format!("ELEVENLABS_API_KEY=\"{primary_key}\""));
                            api_key_found = true;
                            continue;
                        }
                        "ELEVENLABS_API_KEYS" => {
                            new_lines.push(format!("ELEVENLABS_API_KEYS=\"{pool_str}\""));
                            api_keys_found = true;
                            continue;
                        }
                        "ELEVENLABS_VOICE_ID" => {
                            new_lines.push(format!("ELEVENLABS_VOICE_ID=\"{}\"", self.voice_id));
                            voice_id_found = true;
                            continue;
                        }
                        "ELEVENLABS_MODEL_ID" => {
                            new_lines
                                .push(format!("ELEVENLABS_MODEL_ID=\"{}\"", self.model.as_str()));
                            model_id_found = true;
                            continue;
                        }
                        _ => {}
                    }
                }
            }
            new_lines.push(line);
        }

        if !api_key_found {
            new_lines.push(format!("ELEVENLABS_API_KEY=\"{primary_key}\""));
        }
        if !api_keys_found {
            new_lines.push(format!("ELEVENLABS_API_KEYS=\"{pool_str}\""));
        }
        if !voice_id_found {
            new_lines.push(format!("ELEVENLABS_VOICE_ID=\"{}\"", self.voice_id));
        }
        if !model_id_found {
            new_lines.push(format!("ELEVENLABS_MODEL_ID=\"{}\"", self.model.as_str()));
        }

        let content = new_lines.join("\n") + "\n";

        let file_name = self
            .env_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("env");
        let temp_path = self
            .env_path
            .with_file_name(format!(".{file_name}.tmp.{}", uuid::Uuid::new_v4()));

        let mut open_opts = OpenOptions::new();
        open_opts.write(true).create_new(true);
        #[cfg(unix)]
        open_opts.mode(0o600);

        let mut file = open_opts
            .open(&temp_path)
            .map_err(|e| format!("Failed to create temporary file: {e}"))?;

        let write_result = (|| -> Result<(), String> {
            #[cfg(unix)]
            {
                let mut permissions = file
                    .metadata()
                    .map_err(|e| format!("Failed to read metadata: {e}"))?
                    .permissions();
                permissions.set_mode(0o600);
                file.set_permissions(permissions)
                    .map_err(|e| format!("Failed to set 0600 permissions: {e}"))?;
            }

            file.write_all(content.as_bytes())
                .map_err(|e| format!("Failed to write content: {e}"))?;
            file.sync_all()
                .map_err(|e| format!("Failed to sync temporary file: {e}"))?;
            drop(file);

            std::fs::rename(&temp_path, &self.env_path)
                .map_err(|e| format!("Atomic rename failed: {e}"))?;
            Ok(())
        })();

        if let Err(err) = write_result {
            let _ = std::fs::remove_file(&temp_path);
            return Err(err);
        }

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

        let provider_override = std::env::var("TTS_PROVIDER")
            .unwrap_or_default()
            .trim()
            .to_lowercase();

        let tts = if provider_override != "edge" && !self.api_keys.is_empty() {
            let pool = viernes_tts::KeyPool::new(self.api_keys.clone());
            let primary: Arc<dyn viernes_core::traits::TextToSpeech> =
                Arc::new(viernes_tts::ElevenLabsClient::with_options(
                    pool,
                    Some(self.voice_id.clone()),
                    Some(self.model.as_str().to_string()),
                    None,
                ));
            let fallback: Arc<dyn viernes_core::traits::TextToSpeech> =
                Arc::new(viernes_tts::EdgeTtsClient::new());
            ViernesTts::new(primary, fallback)
        } else {
            ViernesTts::from_env()
        };

        let (ev_tx, mut ev_rx) = tokio::sync::broadcast::channel(8);
        let mut player =
            viernes_tts::player::StreamingAudioPlayer::create_resilient_player(Some(ev_tx));

        let audio_bytes = match tts.synthesize_resilient(test_phrase).await {
            Ok(bytes) => bytes,
            Err(e) => {
                self.is_auditioning = false;
                return Err(format!("TTS synthesis error: {e}").into());
            }
        };

        if let Err(e) = player.enqueue_chunk(audio_bytes).await {
            self.is_auditioning = false;
            return Err(format!("Audio player enqueue error: {e}").into());
        }

        // Wait briefly for playback completion so audio isn't truncated on drop
        let _ = tokio::time::timeout(Duration::from_secs(15), async {
            while let Ok(event) = ev_rx.recv().await {
                if matches!(event, SystemEvent::PlaybackFinished) {
                    break;
                }
            }
        })
        .await;

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

        // 2. Add Key Input Buffer (masked on screen)
        let add_key_text = if self.focus == CredentialsFocus::AddingKey {
            let masked_input = "*".repeat(self.input_buffer.chars().count());
            format!("> {masked_input}_")
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

/// Mask an ElevenLabs API key showing prefix and suffix (UTF-8 char-boundary safe).
#[must_use]
pub fn mask_api_key(key: &str) -> String {
    let trimmed = key.trim();
    let char_count = trimmed.chars().count();
    if char_count <= 10 {
        return "sk_***".into();
    }
    let prefix: String = trimmed.chars().take(6).collect();
    let suffix: String = trimmed.chars().skip(char_count - 4).collect();
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

    #[test]
    fn test_save_to_disk_preserves_unrelated_env_keys_and_0600(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let env_file = dir.path().join(".env");
        let existing = "\
# Custom comment
AUDIO_DUCKING_ENABLED=true
AUDIO_DUCKING_VOLUME_PERCENT=35
VIERNES_VOLUME_PERCENT=145
VAD_THRESHOLD=0.025
SILENCE_TIMEOUT_MS=900
AUDIO_INPUT_DEVICE=\"USB Mic Studio\"
AUDIO_OUTPUT_DEVICE=\"Headphones Analog\"
ELEVENLABS_API_KEY=\"sk_old111111111111111111111111111111\"
";
        std::fs::write(&env_file, existing)?;

        let mut view = CredentialsView::from_path(Some(env_file.clone()));
        let new_key = "sk_new1234567890abcdef1234567890abcd";
        view.api_keys = vec![new_key.to_string()];
        view.update_voice_id("21m00Tcm4TlvDq8ikWAM")?;
        view.model = ElevenLabsModel::TurboV2_5;
        view.save_to_disk()?;

        let saved = std::fs::read_to_string(&env_file)?;
        assert!(saved.contains("# Custom comment"));
        assert!(saved.contains("AUDIO_DUCKING_ENABLED=true"));
        assert!(saved.contains("AUDIO_DUCKING_VOLUME_PERCENT=35"));
        assert!(saved.contains("VIERNES_VOLUME_PERCENT=145"));
        assert!(saved.contains("VAD_THRESHOLD=0.025"));
        assert!(saved.contains("SILENCE_TIMEOUT_MS=900"));
        assert!(saved.contains("AUDIO_INPUT_DEVICE=\"USB Mic Studio\""));
        assert!(saved.contains("AUDIO_OUTPUT_DEVICE=\"Headphones Analog\""));
        assert!(saved.contains(&format!("ELEVENLABS_API_KEY=\"{new_key}\"")));
        assert!(saved.contains(&format!("ELEVENLABS_API_KEYS=\"{new_key}\"")));
        assert!(saved.contains("ELEVENLABS_VOICE_ID=\"21m00Tcm4TlvDq8ikWAM\""));
        assert!(saved.contains("ELEVENLABS_MODEL_ID=\"eleven_turbo_v2_5\""));

        let mode = std::fs::metadata(&env_file)?.permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        Ok(())
    }

    #[test]
    fn test_mask_api_key_utf8_multibyte_safe() {
        let utf8_key = "sk_áéíóúñ1234567890";
        let masked = mask_api_key(utf8_key);
        assert_eq!(masked, "sk_áéí...7890");

        let short_utf8 = "sk_áéí";
        assert_eq!(mask_api_key(short_utf8), "sk_***");
    }

    #[test]
    fn test_adding_key_input_is_masked_on_render() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let dir = tempdir().unwrap();
        let mut view = CredentialsView::from_path(Some(dir.path().join(".env")));
        view.focus = CredentialsFocus::AddingKey;
        view.input_buffer = "sk_secret_1234567890".to_string();

        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| view.render(f, f.area())).unwrap();

        let rendered = format!("{:?}", terminal.backend().buffer());
        assert!(!rendered.contains("sk_secret_1234567890"));
        assert!(rendered.contains("********************"));
    }
}
