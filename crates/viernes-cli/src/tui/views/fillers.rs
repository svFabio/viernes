//! Tab 3: Conversational fillers interactive editor and hot-reload view.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame,
};
use tokio::io::AsyncWriteExt;
use tokio::net::UnixStream;
use viernes_brain::fillers::FillerCollection;

/// Maximum allowed character length for a conversational filler phrase.
pub const MAX_PHRASE_LENGTH: usize = 120;

/// Categories of conversational fillers editable in Tab 3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FillerCategory {
    /// Immediate speech termination feedback.
    #[default]
    Generic,
    /// System tools and subprocess dispatch feedback.
    Tools,
    /// High-latency patience masking feedback (>6.5s).
    Patience,
}

impl FillerCategory {
    /// Return localized title for the category tab.
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Generic => "Genéricas",
            Self::Tools => "Herramientas",
            Self::Patience => "Paciencia",
        }
    }

    /// Return the next category in circular order.
    #[must_use]
    pub const fn next(&self) -> Self {
        match self {
            Self::Generic => Self::Tools,
            Self::Tools => Self::Patience,
            Self::Patience => Self::Generic,
        }
    }

    /// Return the previous category in circular order.
    #[must_use]
    pub const fn prev(&self) -> Self {
        match self {
            Self::Generic => Self::Patience,
            Self::Tools => Self::Generic,
            Self::Patience => Self::Tools,
        }
    }
}

/// Mode of user interaction in the fillers editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FillerEditorMode {
    /// Navigating list of phrases.
    #[default]
    Browsing,
    /// Adding a new phrase to the active category.
    Adding,
    /// Modifying an existing phrase inline.
    Editing,
}

/// Action outcome from handling keyboard events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FillersAction {
    /// No global transition.
    None,
    /// Content saved to disk and hot-reload triggered.
    SavedAndReloaded,
}

/// Interactive fillers editor view for Tab 3.
#[derive(Debug, Clone)]
pub struct FillersView {
    /// In-memory structured collection of phrases.
    pub fillers: FillerCollection,
    /// Currently selected category.
    pub current_category: FillerCategory,
    /// Cursor index in current category list.
    pub selected_index: usize,
    /// Active editor mode.
    pub mode: FillerEditorMode,
    /// Text input buffer for adding/editing.
    pub input_buffer: String,
    /// Transient error message from validation.
    pub error_message: Option<String>,
    /// Status notification banner.
    pub status_message: Option<String>,
    /// Whether in-memory state has unsaved modifications.
    pub has_unsaved_changes: bool,
    /// Modal dialog indicating corrupted file recovery with `.bak` backup.
    pub show_corrupted_modal: bool,
    /// Confirmation dialog before discarding unsaved edits.
    pub show_discard_confirm: bool,
    /// Target path to `fillers.json`.
    pub file_path: PathBuf,
    /// Target path to Viernes daemon PTT IPC socket.
    pub socket_path: PathBuf,
}

impl FillersView {
    /// Create view initialized from disk or default collections.
    pub fn new(custom_file_path: Option<PathBuf>, custom_socket_path: Option<PathBuf>) -> Self {
        let file_path = custom_file_path.unwrap_or_else(|| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
            PathBuf::from(home).join(".config/viernes/fillers.json")
        });

        let socket_path = custom_socket_path.unwrap_or_else(|| {
            let env_sock = std::env::var("VIERNES_PTT_SOCKET")
                .unwrap_or_else(|_| "/tmp/viernes_ptt.sock".to_string());
            PathBuf::from(env_sock)
        });

        let mut view = Self {
            fillers: FillerCollection::default(),
            current_category: FillerCategory::Generic,
            selected_index: 0,
            mode: FillerEditorMode::Browsing,
            input_buffer: String::new(),
            error_message: None,
            status_message: None,
            has_unsaved_changes: false,
            show_corrupted_modal: false,
            show_discard_confirm: false,
            file_path,
            socket_path,
        };

        view.load_from_disk();
        view
    }

    /// Read phrases from disk with automatic `.bak` recovery if corrupt.
    pub fn load_from_disk(&mut self) {
        if !self.file_path.exists() {
            return;
        }

        match std::fs::read_to_string(&self.file_path) {
            Ok(content) => match serde_json::from_str::<FillerCollection>(&content) {
                Ok(loaded) => {
                    self.fillers = loaded;
                }
                Err(err) => {
                    // Create .bak backup of the corrupted file
                    let bak_path = self.file_path.with_extension("json.bak");
                    let _ = std::fs::copy(&self.file_path, &bak_path);
                    self.fillers = FillerCollection::default();
                    self.show_corrupted_modal = true;
                    self.error_message = Some(format!(
                        "JSON corrupto detectado ({err}). Valores por defecto restaurados y respaldo creado en {:?}",
                        bak_path.file_name().unwrap_or_default()
                    ));
                }
            },
            Err(err) => {
                self.error_message = Some(format!("Error al leer fillers.json: {err}"));
            }
        }
    }

    /// Return reference to phrases in the currently selected category.
    #[must_use]
    pub fn current_phrases(&self) -> Vec<String> {
        match self.current_category {
            FillerCategory::Generic => self.fillers.immediate.clone(),
            FillerCategory::Tools => {
                // Flatten all tools phrases for unified editing
                let mut all = Vec::new();
                for list in self.fillers.tools.values() {
                    all.extend(list.clone());
                }
                if all.is_empty() {
                    vec!["Accediendo a las herramientas del sistema...".into()]
                } else {
                    all
                }
            }
            FillerCategory::Patience => self.fillers.patience.clone(),
        }
    }

    /// Add a validated phrase to the current category.
    pub fn add_phrase(&mut self, text: &str) -> Result<(), String> {
        validate_phrase(text)?;
        let trimmed = text.trim().to_string();

        match self.current_category {
            FillerCategory::Generic => {
                self.fillers.immediate.push(trimmed);
            }
            FillerCategory::Tools => {
                self.fillers
                    .tools
                    .entry("terminal".into())
                    .or_default()
                    .push(trimmed);
            }
            FillerCategory::Patience => {
                self.fillers.patience.push(trimmed);
            }
        }

        self.has_unsaved_changes = true;
        self.error_message = None;
        self.status_message = Some("Frase agregada con éxito".into());
        Ok(())
    }

    /// Inline edit selected phrase.
    pub fn edit_current_phrase(&mut self, new_text: &str) -> Result<(), String> {
        validate_phrase(new_text)?;
        let trimmed = new_text.trim().to_string();

        match self.current_category {
            FillerCategory::Generic => {
                if self.selected_index < self.fillers.immediate.len() {
                    self.fillers.immediate[self.selected_index] = trimmed;
                }
            }
            FillerCategory::Tools => {
                let mut found = false;
                let mut current_idx = 0;
                for list in self.fillers.tools.values_mut() {
                    for item in list.iter_mut() {
                        if current_idx == self.selected_index {
                            *item = trimmed.clone();
                            found = true;
                            break;
                        }
                        current_idx += 1;
                    }
                    if found {
                        break;
                    }
                }
            }
            FillerCategory::Patience => {
                if self.selected_index < self.fillers.patience.len() {
                    self.fillers.patience[self.selected_index] = trimmed;
                }
            }
        }

        self.has_unsaved_changes = true;
        self.error_message = None;
        self.status_message = Some("Frase modificada con éxito".into());
        Ok(())
    }

    /// Delete current phrase enforcing at least 1 phrase per category.
    pub fn delete_current_phrase(&mut self) -> Result<(), String> {
        let count = self.current_phrases().len();
        if count <= 1 {
            return Err("No se puede eliminar la última frase de la categoría".into());
        }

        match self.current_category {
            FillerCategory::Generic => {
                if self.selected_index < self.fillers.immediate.len() {
                    self.fillers.immediate.remove(self.selected_index);
                }
            }
            FillerCategory::Tools => {
                let mut current_idx = 0;
                let mut remove_target = None;
                for (cat_key, list) in self.fillers.tools.iter_mut() {
                    for (i, _) in list.iter().enumerate() {
                        if current_idx == self.selected_index {
                            remove_target = Some((cat_key.clone(), i));
                            break;
                        }
                        current_idx += 1;
                    }
                    if remove_target.is_some() {
                        break;
                    }
                }
                if let Some((cat_key, i)) = remove_target {
                    if let Some(list) = self.fillers.tools.get_mut(&cat_key) {
                        list.remove(i);
                    }
                }
            }
            FillerCategory::Patience => {
                if self.selected_index < self.fillers.patience.len() {
                    self.fillers.patience.remove(self.selected_index);
                }
            }
        }

        if self.selected_index > 0 && self.selected_index >= self.current_phrases().len() {
            self.selected_index = self.current_phrases().len().saturating_sub(1);
        }

        self.has_unsaved_changes = true;
        self.status_message = Some("Frase eliminada".into());
        Ok(())
    }

    /// Atomically write current collection to `fillers.json` (temp file + rename).
    pub fn save_to_disk(&mut self) -> Result<(), String> {
        if let Some(parent) = self.file_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let json_str = serde_json::to_string_pretty(&self.fillers)
            .map_err(|e| format!("Error de serialización JSON: {e}"))?;

        let temp_path = self.file_path.with_extension("tmp");
        {
            let mut file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&temp_path)
                .map_err(|e| format!("Error al crear archivo temporal: {e}"))?;

            file.write_all(json_str.as_bytes())
                .map_err(|e| format!("Error al escribir archivo: {e}"))?;
            file.sync_all()
                .map_err(|e| format!("Error de sincronización en disco: {e}"))?;
        }

        std::fs::rename(&temp_path, &self.file_path)
            .map_err(|e| format!("Error al renombrar archivo atómicamente: {e}"))?;

        self.has_unsaved_changes = false;
        self.status_message = Some("Frases guardadas atómicamente en fillers.json".into());
        Ok(())
    }

    /// Send asynchronous hot-reload IPC trigger to running Viernes daemon.
    pub async fn dispatch_hot_reload(&self) -> Result<(), String> {
        if !self.socket_path.exists() {
            return Err(
                "Daemon no está activo (socket /tmp/viernes_ptt.sock no encontrado)".into(),
            );
        }

        let mut stream = UnixStream::connect(&self.socket_path)
            .await
            .map_err(|e| format!("Error de conexión IPC: {e}"))?;

        stream
            .write_all(b"reload\n")
            .await
            .map_err(|e| format!("Error enviando comando reload: {e}"))?;
        stream
            .flush()
            .await
            .map_err(|e| format!("Error de flush IPC: {e}"))?;

        Ok(())
    }

    /// Process keyboard navigation and text entry for Tab 3.
    pub fn handle_key(&mut self, key: KeyEvent) -> FillersAction {
        // Corrupted file notification modal dismissal
        if self.show_corrupted_modal {
            if matches!(key.code, KeyCode::Enter | KeyCode::Esc | KeyCode::Char(' ')) {
                self.show_corrupted_modal = false;
                self.error_message = None;
            }
            return FillersAction::None;
        }

        // Active text entry (Adding or Editing)
        match self.mode {
            FillerEditorMode::Adding => match key.code {
                KeyCode::Enter => {
                    let text = std::mem::take(&mut self.input_buffer);
                    match self.add_phrase(&text) {
                        Ok(()) => {
                            self.mode = FillerEditorMode::Browsing;
                            self.selected_index = self.current_phrases().len().saturating_sub(1);
                        }
                        Err(err) => {
                            self.input_buffer = text;
                            self.error_message = Some(err);
                        }
                    }
                }
                KeyCode::Esc => {
                    self.input_buffer.clear();
                    self.error_message = None;
                    self.mode = FillerEditorMode::Browsing;
                }
                KeyCode::Backspace => {
                    self.input_buffer.pop();
                }
                KeyCode::Char(c) => {
                    if self.input_buffer.len() < MAX_PHRASE_LENGTH {
                        self.input_buffer.push(c);
                    } else {
                        self.error_message = Some("Límite de 120 caracteres alcanzado".into());
                    }
                }
                _ => {}
            },
            FillerEditorMode::Editing => match key.code {
                KeyCode::Enter => {
                    let text = std::mem::take(&mut self.input_buffer);
                    match self.edit_current_phrase(&text) {
                        Ok(()) => {
                            self.mode = FillerEditorMode::Browsing;
                        }
                        Err(err) => {
                            self.input_buffer = text;
                            self.error_message = Some(err);
                        }
                    }
                }
                KeyCode::Esc => {
                    self.input_buffer.clear();
                    self.error_message = None;
                    self.mode = FillerEditorMode::Browsing;
                }
                KeyCode::Backspace => {
                    self.input_buffer.pop();
                }
                KeyCode::Char(c) => {
                    if self.input_buffer.len() < MAX_PHRASE_LENGTH {
                        self.input_buffer.push(c);
                    } else {
                        self.error_message = Some("Límite de 120 caracteres alcanzado".into());
                    }
                }
                _ => {}
            },
            FillerEditorMode::Browsing => match key.code {
                // Category switching
                KeyCode::Left => {
                    self.current_category = self.current_category.prev();
                    self.selected_index = 0;
                    self.error_message = None;
                }
                KeyCode::Right => {
                    self.current_category = self.current_category.next();
                    self.selected_index = 0;
                    self.error_message = None;
                }
                // List navigation
                KeyCode::Up => {
                    if self.selected_index > 0 {
                        self.selected_index -= 1;
                    }
                }
                KeyCode::Down => {
                    let count = self.current_phrases().len();
                    if count > 0 && self.selected_index + 1 < count {
                        self.selected_index += 1;
                    }
                }
                // Actions
                KeyCode::Char('a') | KeyCode::Char('+') => {
                    self.mode = FillerEditorMode::Adding;
                    self.input_buffer.clear();
                    self.error_message = None;
                }
                KeyCode::Char('e') | KeyCode::Enter => {
                    let phrases = self.current_phrases();
                    if let Some(selected) = phrases.get(self.selected_index) {
                        self.input_buffer = selected.clone();
                        self.mode = FillerEditorMode::Editing;
                        self.error_message = None;
                    }
                }
                KeyCode::Char('d') => {
                    if let Err(err) = self.delete_current_phrase() {
                        self.error_message = Some(err);
                    }
                }
                KeyCode::Char('s') | KeyCode::Char('S') => {
                    if let Err(err) = self.save_to_disk() {
                        self.error_message = Some(err);
                    } else {
                        return FillersAction::SavedAndReloaded;
                    }
                }
                _ => {}
            },
        }

        FillersAction::None
    }

    /// Render Tab 3 UI layout.
    pub fn render(&self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Category Selector Tabs
                Constraint::Min(8),    // Phrases List
                Constraint::Length(4), // Add/Edit Input Box
                Constraint::Length(3), // Action bar
                Constraint::Length(2), // Status & error messages
            ])
            .split(area);

        // 1. Category Switcher Header
        let cat_spans = vec![
            Span::raw(" Categoría: "),
            render_category_pill(FillerCategory::Generic, self.current_category),
            Span::raw("   "),
            render_category_pill(FillerCategory::Tools, self.current_category),
            Span::raw("   "),
            render_category_pill(FillerCategory::Patience, self.current_category),
            Span::styled(
                "   [← / →]: Cambiar categoría",
                Style::default().fg(Color::DarkGray),
            ),
        ];
        let cat_block = Block::default()
            .borders(Borders::ALL)
            .title(" [3] Editor de Frases de Relleno (fillers.json) ")
            .style(Style::default().fg(Color::Magenta));
        f.render_widget(
            Paragraph::new(Line::from(cat_spans)).block(cat_block),
            chunks[0],
        );

        // 2. Phrases List
        let phrases = self.current_phrases();
        let total = phrases.len();
        let list_items: Vec<ListItem> = phrases
            .iter()
            .enumerate()
            .map(|(idx, phrase)| {
                let is_selected = idx == self.selected_index;
                let marker = if is_selected { " > " } else { "   " };
                let num_span = Span::styled(
                    format!("{marker}{}. ", idx + 1),
                    if is_selected {
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    },
                );
                let text_span = Span::styled(
                    phrase,
                    if is_selected {
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::Gray)
                    },
                );
                ListItem::new(Line::from(vec![num_span, text_span]))
            })
            .collect();

        let list_title = format!(
            " Frases registradas ({total}/50 en {}) ",
            self.current_category.label()
        );
        let list_block = Block::default()
            .borders(Borders::ALL)
            .title(list_title)
            .style(Style::default().fg(Color::White));
        f.render_widget(List::new(list_items).block(list_block), chunks[1]);

        // 3. Add / Edit Text Buffer Box
        let (input_title, input_text, input_style) = match self.mode {
            FillerEditorMode::Adding => (
                format!(" [+] Nueva frase ({}/120) ", self.input_buffer.len()),
                format!("> {}_", self.input_buffer),
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            ),
            FillerEditorMode::Editing => (
                format!(" [✎] Editando frase ({}/120) ", self.input_buffer.len()),
                format!("> {}_", self.input_buffer),
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
            FillerEditorMode::Browsing => (
                " Editor de frases ".to_string(),
                "Presione [a] para agregar una nueva frase o [Enter / e] para editar la seleccionada.".to_string(),
                Style::default().fg(Color::DarkGray),
            ),
        };

        let input_block = Block::default()
            .borders(Borders::ALL)
            .title(input_title)
            .style(if self.mode != FillerEditorMode::Browsing {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::Gray)
            });
        f.render_widget(
            Paragraph::new(input_text)
                .style(input_style)
                .block(input_block),
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
            Span::raw("Agregar  "),
            Span::styled(
                " [e/Enter] ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("Editar  "),
            Span::styled(
                " [d] ",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            Span::raw("Eliminar  "),
            Span::styled(
                " [s] ",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("Guardar y Recargar Daemon  "),
            Span::styled(
                " [Esc] ",
                Style::default()
                    .fg(Color::Gray)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("Cancelar"),
        ];
        if self.has_unsaved_changes {
            action_spans.push(Span::styled(
                "  *CAMBIOS PENDIENTES*",
                Style::default()
                    .fg(Color::LightRed)
                    .add_modifier(Modifier::BOLD),
            ));
        }
        let action_block = Block::default().borders(Borders::ALL).title(" Atajos ");
        f.render_widget(
            Paragraph::new(Line::from(action_spans)).block(action_block),
            chunks[3],
        );

        // 5. Status / Error Message
        let mut msg_spans = Vec::new();
        if let Some(ref err) = self.error_message {
            msg_spans.push(Span::styled(
                format!("ERROR: {err} "),
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ));
        }
        if let Some(ref status) = self.status_message {
            msg_spans.push(Span::styled(
                format!("ESTADO: {status} "),
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ));
        }
        f.render_widget(Paragraph::new(Line::from(msg_spans)), chunks[4]);

        // Corrupted JSON Warning Modal
        if self.show_corrupted_modal {
            render_modal(
                f,
                area,
                " Archivo Corrupto Detectado ",
                "Se detectó un JSON corrupto en ~/.config/viernes/fillers.json.\n\nSe creó un respaldo de seguridad (.bak) y se cargaron los valores por defecto.\n\nPresione [Enter/Esc] para continuar.",
                Color::Yellow,
            );
        }
    }
}

/// Helper formatting category selector pills.
fn render_category_pill(cat: FillerCategory, current: FillerCategory) -> Span<'static> {
    if cat == current {
        Span::styled(
            format!("(•) {} ", cat.label()),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(
            format!("( ) {} ", cat.label()),
            Style::default().fg(Color::DarkGray),
        )
    }
}

/// Validate that a filler phrase is non-empty and does not exceed 120 characters.
pub fn validate_phrase(text: &str) -> Result<(), String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err("La frase no puede estar vacía".into());
    }
    if trimmed.len() > MAX_PHRASE_LENGTH {
        return Err(format!(
            "La frase supera el límite de {MAX_PHRASE_LENGTH} caracteres (longitud: {})",
            trimmed.len()
        ));
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
    fn test_validate_phrase_boundaries() {
        assert!(validate_phrase("A la orden, Señor.").is_ok());
        assert!(validate_phrase("").is_err());
        assert!(validate_phrase("   ").is_err());

        // Exactly 120 characters
        let exactly_120 = "a".repeat(120);
        assert!(validate_phrase(&exactly_120).is_ok());

        // 121 characters
        let over_120 = "a".repeat(121);
        assert!(validate_phrase(&over_120).is_err());
    }

    #[test]
    fn test_category_navigation_cycle() {
        let cat = FillerCategory::Generic;
        assert_eq!(cat.next(), FillerCategory::Tools);
        assert_eq!(cat.next().next(), FillerCategory::Patience);
        assert_eq!(cat.next().next().next(), FillerCategory::Generic);

        assert_eq!(cat.prev(), FillerCategory::Patience);
    }

    #[test]
    fn test_json_validation_valid_and_invalid_structures() -> Result<(), Box<dyn std::error::Error>>
    {
        // Valid JSON structure
        let valid_json = r#"{
            "immediate": ["Enseguida.", "A la orden."],
            "tools": { "terminal": ["Ejecutando comando..."] },
            "patience": ["Sigo buscando..."]
        }"#;
        let parsed: Result<FillerCollection, _> = serde_json::from_str(valid_json);
        assert!(parsed.is_ok());
        let collection = parsed?;
        assert_eq!(collection.immediate.len(), 2);
        assert_eq!(collection.patience.len(), 1);

        // Invalid JSON: missing quotes, broken syntax
        let invalid_json = r#"{ immediate: [ broken ] }"#;
        let failed_parse: Result<FillerCollection, _> = serde_json::from_str(invalid_json);
        assert!(failed_parse.is_err());
        Ok(())
    }

    #[test]
    fn test_add_edit_and_delete_invariants() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let json_path = dir.path().join("fillers.json");
        let mut view = FillersView::new(Some(json_path), None);
        let initial_count = view.current_phrases().len();

        // Add
        view.add_phrase("Nueva confirmación inmediata.")?;
        assert_eq!(view.current_phrases().len(), initial_count + 1);

        // Edit
        view.selected_index = initial_count;
        view.edit_current_phrase("Frase editada correctamente.")?;
        assert_eq!(
            view.current_phrases()[initial_count],
            "Frase editada correctamente."
        );

        // Delete until only 1 remains
        while view.current_phrases().len() > 1 {
            view.delete_current_phrase()?;
        }
        assert_eq!(view.current_phrases().len(), 1);

        // Enforce constraint: Cannot delete last remaining phrase
        assert!(view.delete_current_phrase().is_err());
        Ok(())
    }

    #[test]
    fn test_atomic_persistence_and_serialization() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let json_path = dir.path().join("fillers.json");
        let mut view = FillersView::new(Some(json_path.clone()), None);

        view.add_phrase("Frase de prueba para persistencia")?;
        view.save_to_disk()?;
        assert!(json_path.exists());

        // Reload from disk and verify
        let reloaded = FillersView::new(Some(json_path), None);
        assert!(reloaded
            .fillers
            .immediate
            .contains(&"Frase de prueba para persistencia".to_string()));
        Ok(())
    }

    #[test]
    fn test_corrupted_json_creates_backup() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let json_path = dir.path().join("fillers.json");

        // Write corrupt contents
        std::fs::write(&json_path, "{ broken json ...")?;

        let view = FillersView::new(Some(json_path.clone()), None);
        assert!(view.show_corrupted_modal);

        let bak_path = dir.path().join("fillers.json.bak");
        assert!(bak_path.exists());
        assert_eq!(std::fs::read_to_string(bak_path)?, "{ broken json ...");
        Ok(())
    }
}
