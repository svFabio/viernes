//! Application state, navigation tabs, dashboard rendering, and user input orchestration.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Tabs};
use ratatui::Frame;
use viernes_core::config::ViernesConfig;
use viernes_core::state::{DaemonLiveStatus, SystemState};

use crate::tui::views::credentials::{CredentialsAction, CredentialsView};
use crate::tui::views::devices::DevicesView;
use crate::tui::views::fillers::{FillersAction, FillersView};
use crate::tui::views::service::ServiceView;
use crate::tui::widgets::ptt::ptt_socket_path;
use crate::tui::widgets::{
    send_ipc_ptt, ConversationTurn, ConversationWidget, PttWidget, Speaker, SpectrumWidget,
};

/// Minimum required terminal columns according to HU-RUST-29.
pub const MIN_TERMINAL_WIDTH: u16 = 80;
/// Minimum required terminal rows according to HU-RUST-29.
pub const MIN_TERMINAL_HEIGHT: u16 = 24;

/// Active tab index in the dashboard navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ActiveTab {
    /// Tab 1: Central State, Spectrum Soundwave, and Conversation Turn Monitor.
    #[default]
    Monitor,
    /// Tab 2: Voice Provider & Key Pool Configuration.
    VoiceAndKeys,
    /// Tab 3: Conversational Filler Phrases Editor.
    Fillers,
    /// Tab 4: Audio Hardware Devices & Level Meter.
    AudioDevices,
    /// Tab 5: Daemon Service, IPC & Live Logs.
    ServiceAndLogs,
}

impl ActiveTab {
    /// Array containing all tabs in display order.
    pub const ALL: [ActiveTab; 5] = [
        Self::Monitor,
        Self::VoiceAndKeys,
        Self::Fillers,
        Self::AudioDevices,
        Self::ServiceAndLogs,
    ];

    /// Zero-based numerical index.
    #[must_use]
    pub const fn index(&self) -> usize {
        match self {
            Self::Monitor => 0,
            Self::VoiceAndKeys => 1,
            Self::Fillers => 2,
            Self::AudioDevices => 3,
            Self::ServiceAndLogs => 4,
        }
    }

    /// Resolves tab from numerical index.
    #[must_use]
    pub fn from_index(idx: usize) -> Self {
        match idx {
            0 => Self::Monitor,
            1 => Self::VoiceAndKeys,
            2 => Self::Fillers,
            3 => Self::AudioDevices,
            _ => Self::ServiceAndLogs,
        }
    }

    /// Next navigation tab.
    #[must_use]
    pub fn next(&self) -> Self {
        Self::from_index((self.index() + 1) % Self::ALL.len())
    }

    /// Previous navigation tab.
    #[must_use]
    pub fn prev(&self) -> Self {
        Self::from_index((self.index() + Self::ALL.len() - 1) % Self::ALL.len())
    }

    /// Human-readable label for navigation bar.
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Monitor => "1: Monitor",
            Self::VoiceAndKeys => "2: Voz & Claves",
            Self::Fillers => "3: Frases Relleno",
            Self::AudioDevices => "4: Audio",
            Self::ServiceAndLogs => "5: Servicio",
        }
    }
}

/// Unified application state for the Viernes TUI.
pub struct TuiApp {
    /// Currently visible tab.
    pub active_tab: ActiveTab,
    /// Flag signaling main event loop termination.
    pub should_quit: bool,
    /// Number of ticks elapsed for 30 FPS animation.
    pub tick_count: u64,

    // --- Tab 1: Monitor State ---
    /// Runtime configuration if available.
    pub config: Option<ViernesConfig>,
    /// Observed runtime state of the assistant daemon.
    pub assistant_state: SystemState,
    /// Live frequency spectrum amplitudes [0.0, 1.0].
    pub spectrum_bars: Vec<f32>,
    /// Scrollable conversation turn history.
    pub turns: Vec<ConversationTurn>,
    /// Whether Push-to-Talk emulator is actively recording.
    pub ptt_active: bool,
    /// Whether connection to background socket is detected.
    pub daemon_connected: bool,

    // --- Tab 2-5 Views ---
    /// Tab 2 view state (Credentials & Voice).
    pub credentials_view: CredentialsView,
    /// Tab 3 view state (Fillers).
    pub fillers_view: FillersView,
    /// Tab 4 view state (Audio Devices).
    pub devices_view: DevicesView,
    /// Tab 5 view state (Service & Live Logs).
    pub service_view: ServiceView,

    /// Global status notification message.
    pub status_line: Option<String>,
    /// Last synchronized turn counter from daemon status snapshot.
    pub last_synced_turn: u64,
    /// Timestamp of last PTT toggle to debounce key repeat events.
    pub last_ptt_toggle: std::time::Instant,
    /// Last recorded terminal width for responsive layout and click mapping.
    pub last_terminal_width: u16,
    /// Last recorded terminal height for responsive layout and click mapping.
    pub last_terminal_height: u16,
}

/// Convenience alias for `TuiApp`.
pub type App = TuiApp;

impl Default for TuiApp {
    fn default() -> Self {
        Self::new(None)
    }
}

impl TuiApp {
    /// Number of frequency bars displayed in the spectrum soundwave.
    pub const SPECTRUM_BAR_COUNT: usize = 64;

    /// Constructs a new `TuiApp` instance.
    #[must_use]
    pub fn new(config: Option<ViernesConfig>) -> Self {
        let mut initial_turns = vec![ConversationTurn::new(
            Speaker::System,
            "Viernes Terminal Dashboard v0.1.0 inicializado. Listo.",
        )];

        let mut assistant_state = SystemState::Idle;
        let mut last_synced_turn = 0;
        if let Some(status) = DaemonLiveStatus::read_from_disk() {
            assistant_state = status.state;
            last_synced_turn = status.turn_counter;
            if !status.last_user_prompt.is_empty() {
                initial_turns.push(ConversationTurn::new(Speaker::User, status.last_user_prompt));
            }
            if !status.last_assistant_text.is_empty() {
                initial_turns.push(ConversationTurn::new(
                    Speaker::Viernes,
                    status.last_assistant_text,
                ));
            }
        }

        let daemon_connected = std::path::Path::new(&ptt_socket_path()).exists();

        Self {
            active_tab: ActiveTab::Monitor,
            should_quit: false,
            tick_count: 0,
            credentials_view: CredentialsView::new(config.clone()),
            config: config.clone(),
            assistant_state,
            spectrum_bars: vec![0.05; Self::SPECTRUM_BAR_COUNT],
            turns: initial_turns,
            ptt_active: false,
            daemon_connected,
            fillers_view: FillersView::new(None, None),
            devices_view: DevicesView::new(),
            service_view: ServiceView::new(),
            status_line: Some("Viernes TUI Dashboard inicializado".into()),
            last_synced_turn,
            last_ptt_toggle: std::time::Instant::now() - std::time::Duration::from_secs(1),
            last_terminal_width: 80,
            last_terminal_height: 24,
        }
    }

    /// Next navigation tab.
    pub fn next_tab(&mut self) {
        self.active_tab = self.active_tab.next();
    }

    /// Previous navigation tab.
    pub fn prev_tab(&mut self) {
        self.active_tab = self.active_tab.prev();
    }

    /// Advances periodic animations at 30 FPS.
    pub fn on_tick(&mut self) {
        self.tick_count = self.tick_count.wrapping_add(1);

        // Periodically verify background daemon status and socket connectivity
        if self.tick_count % 3 == 0 {
            if let Some(status) = DaemonLiveStatus::read_from_disk() {
                self.daemon_connected = true;
                if !self.ptt_active {
                    self.assistant_state = status.state;
                }

                if status.turn_counter > self.last_synced_turn {
                    if !status.last_user_prompt.is_empty() {
                        self.add_turn(Speaker::User, status.last_user_prompt.clone());
                    }
                    if !status.last_assistant_text.is_empty() {
                        self.add_turn(Speaker::Viernes, status.last_assistant_text.clone());
                    }
                    self.last_synced_turn = status.turn_counter;
                } else if self.assistant_state == SystemState::Speaking
                    && !status.last_assistant_text.is_empty()
                {
                    // Update streaming response
                    if let Some(last) = self.turns.last_mut() {
                        if last.speaker == Speaker::Viernes {
                            last.text = status.last_assistant_text.clone();
                        } else {
                            self.add_turn(Speaker::Viernes, status.last_assistant_text.clone());
                        }
                    }
                }
            } else if self.tick_count.is_multiple_of(30) {
                self.daemon_connected = std::path::Path::new(&ptt_socket_path()).exists();
            }
        }

        // Update soundwave spectrum based on assistant state
        let bar_count = self.spectrum_bars.len();
        if self.assistant_state == SystemState::Speaking {
            let t = (self.tick_count as f32) * 0.22;
            for i in 0..bar_count {
                let fi = i as f32 / bar_count as f32;
                let formant1 = (-(fi - 0.20).powi(2) / 0.03).exp();
                let formant2 = (-(fi - 0.52).powi(2) / 0.05).exp() * 0.85;
                let formant3 = (-(fi - 0.76).powi(2) / 0.04).exp() * 0.5;
                let envelope = (formant1 + formant2 + formant3).clamp(0.1, 1.2);

                let wave1 = (fi * 14.0 + t).sin() * 0.5 + 0.5;
                let wave2 = (fi * 28.0 - t * 1.8).cos() * 0.5 + 0.5;
                let wave3 = ((fi * 7.0 + t * 0.8).sin()).abs();

                let dynamic_amp = (wave1 * 0.45 + wave2 * 0.35 + wave3 * 0.20) * envelope;
                let target_height = (0.2 + dynamic_amp * 0.8).clamp(0.08, 0.98);
                self.spectrum_bars[i] =
                    (self.spectrum_bars[i] * 0.35 + target_height * 0.65).clamp(0.05, 1.0);
            }
        } else if self.assistant_state == SystemState::Listening {
            let t = (self.tick_count as f32) * 0.20;
            for i in 0..bar_count {
                let fi = i as f32 / bar_count as f32;
                let wave = ((fi * 10.0 + t).sin() * 0.5 + 0.5) * 0.5 + 0.1;
                self.spectrum_bars[i] =
                    (self.spectrum_bars[i] * 0.4 + wave * 0.6).clamp(0.05, 0.7);
            }
        } else if self.assistant_state == SystemState::Thinking {
            let t = (self.tick_count as f32) * 0.12;
            let pulse = (t.sin() * 0.5 + 0.5) * 0.4 + 0.08;
            for i in 0..bar_count {
                let fi = i as f32 / bar_count as f32;
                let wave = (fi * 6.0 - t).cos().abs() * 0.25 + pulse * 0.75;
                self.spectrum_bars[i] =
                    (self.spectrum_bars[i] * 0.5 + wave * 0.5).clamp(0.05, 0.55);
            }
        } else {
            // Quiescent idle baseline wave
            let t = (self.tick_count as f32) * 0.05;
            for i in 0..bar_count {
                let fi = i as f32 / bar_count as f32;
                let wave = ((fi * 4.0 + t).sin() * 0.5 + 0.5) * 0.06 + 0.02;
                self.spectrum_bars[i] =
                    (self.spectrum_bars[i] * 0.8 + wave * 0.2).clamp(0.01, 0.12);
            }
        }

        // Background simulation for mic level in Devices view if testing
        if self.devices_view.is_testing_mic {
            let level = (((self.tick_count as f32 * 0.2).sin().abs()) * 0.75) + 0.1;
            self.devices_view.set_mic_level(level);
        }
    }

    /// Appends a new conversation turn to the history log.
    pub fn add_turn(&mut self, speaker: Speaker, text: impl Into<String>) {
        self.turns.push(ConversationTurn::new(speaker, text));
        if self.turns.len() > 100 {
            self.turns.remove(0);
        }
    }

    /// Toggles Push-to-Talk recording state cleanly with debouncing.
    pub async fn toggle_ptt(&mut self) {
        let now = std::time::Instant::now();
        if now.duration_since(self.last_ptt_toggle) < std::time::Duration::from_millis(250) {
            return;
        }
        self.last_ptt_toggle = now;

        if !self.ptt_active {
            // Start recording (Habilitar micrófono)
            self.ptt_active = true;
            self.assistant_state = SystemState::Listening;
            self.status_line =
                Some("Micrófono activado. Habla ahora... (Espacio/Enter o Click para enviar)".into());
            let _ = send_ipc_ptt("press").await;
        } else {
            // Stop recording & send for processing
            self.ptt_active = false;
            self.assistant_state = SystemState::Thinking;
            self.status_line = Some("Audio enviado. Procesando en Antigravity...".into());
            let _ = send_ipc_ptt("release").await;
        }
    }

    /// Handles mouse click events across navigation tabs and interactive controls.
    pub async fn handle_mouse(&mut self, mouse: crossterm::event::MouseEvent) {
        use crossterm::event::MouseEventKind;

        if let MouseEventKind::Down(crossterm::event::MouseButton::Left) = mouse.kind {
            let col = mouse.column;
            let row = mouse.row;

            // 1. Click on Tab Header (row 0, 1, or 2)
            if row <= 2 {
                if col < 16 {
                    self.active_tab = ActiveTab::Monitor;
                } else if col < 32 {
                    self.active_tab = ActiveTab::VoiceAndKeys;
                } else if col < 51 {
                    self.active_tab = ActiveTab::Fillers;
                } else if col < 64 {
                    self.active_tab = ActiveTab::AudioDevices;
                } else {
                    self.active_tab = ActiveTab::ServiceAndLogs;
                }
                return;
            }

            // 2. Click in Tab 1: Monitor -> PTT Bar at bottom
            if self.active_tab == ActiveTab::Monitor {
                let ptt_zone_start = self.last_terminal_height.saturating_sub(5);
                if row >= ptt_zone_start {
                    self.toggle_ptt().await;
                }
            }
        }
    }

    /// Dispatches a key event according to active tab and global shortcuts.
    pub async fn handle_key(&mut self, key: KeyEvent) {
        // Drop OS/terminal key repeat to prevent rapid on/off toggling while holding down Space
        if key.kind == KeyEventKind::Repeat {
            return;
        }
        if key.kind != KeyEventKind::Press {
            return;
        }

        // Global quit shortcut: Ctrl+C
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return;
        }

        // 1. Tab-specific delegates when editing text fields
        let is_editing = (self.active_tab == ActiveTab::VoiceAndKeys
            && self.credentials_view.focus
                != crate::tui::views::credentials::CredentialsFocus::KeyList)
            || (self.active_tab == ActiveTab::Fillers
                && self.fillers_view.mode
                    != crate::tui::views::fillers::FillerEditorMode::Browsing);

        if !is_editing {
            // Global navigation keys
            match key.code {
                KeyCode::Char('q') => {
                    // Close dropdowns in Tab 4 first if open
                    if self.active_tab == ActiveTab::AudioDevices
                        && (self.devices_view.input_dropdown_open
                            || self.devices_view.output_dropdown_open)
                    {
                        self.devices_view.input_dropdown_open = false;
                        self.devices_view.output_dropdown_open = false;
                        return;
                    }
                    self.should_quit = true;
                    return;
                }
                KeyCode::Char('1') => {
                    self.active_tab = ActiveTab::Monitor;
                    return;
                }
                KeyCode::Char('2') => {
                    self.active_tab = ActiveTab::VoiceAndKeys;
                    return;
                }
                KeyCode::Char('3') => {
                    self.active_tab = ActiveTab::Fillers;
                    return;
                }
                KeyCode::Char('4') => {
                    self.active_tab = ActiveTab::AudioDevices;
                    return;
                }
                KeyCode::Char('5') => {
                    self.active_tab = ActiveTab::ServiceAndLogs;
                    return;
                }
                KeyCode::Tab => {
                    if self.active_tab == ActiveTab::AudioDevices {
                        self.devices_view.next_focus();
                    } else {
                        self.next_tab();
                    }
                    return;
                }
                KeyCode::BackTab => {
                    if self.active_tab == ActiveTab::AudioDevices {
                        self.devices_view.prev_focus();
                    } else {
                        self.prev_tab();
                    }
                    return;
                }
                _ => {}
            }
        }

        // 2. Active Tab Event Processing
        match self.active_tab {
            ActiveTab::Monitor => match key.code {
                KeyCode::Char(' ') | KeyCode::Enter => {
                    self.toggle_ptt().await;
                }
                KeyCode::Esc => {
                    if self.ptt_active
                        || self.assistant_state == SystemState::Speaking
                        || self.assistant_state == SystemState::Listening
                        || self.assistant_state == SystemState::Thinking
                    {
                        // Barge-in / Cancel speech
                        self.ptt_active = false;
                        self.assistant_state = SystemState::Idle;
                        let _ = send_ipc_ptt("cancel").await;
                    } else {
                        self.should_quit = true;
                    }
                }
                _ => {}
            },
            ActiveTab::VoiceAndKeys => {
                let action = self.credentials_view.handle_key(key);
                match action {
                    CredentialsAction::SavedToDisk => {
                        self.status_line = Some("Credenciales guardadas en .env (0600)".into());
                    }
                    CredentialsAction::TestVoiceTriggered => {
                        self.status_line = Some("Iniciando audición de prueba de voz...".into());
                    }
                    CredentialsAction::None => {}
                }
            }
            ActiveTab::Fillers => {
                let action = self.fillers_view.handle_key(key);
                if action == FillersAction::SavedAndReloaded {
                    self.status_line = Some("Guardado y recargado vía IPC".into());
                    let _ = self.fillers_view.dispatch_hot_reload().await;
                }
            }
            ActiveTab::AudioDevices => match key.code {
                KeyCode::Up | KeyCode::Char('k') => self.devices_view.select_prev(),
                KeyCode::Down | KeyCode::Char('j') => self.devices_view.select_next(),
                KeyCode::Enter | KeyCode::Char(' ') => self.devices_view.toggle_action(),
                KeyCode::Char('m') => self.devices_view.toggle_action(),
                KeyCode::Char('r') => self.devices_view.refresh_from_host(),
                _ => {}
            },
            ActiveTab::ServiceAndLogs => match key.code {
                KeyCode::Char('R') | KeyCode::Char('r') => self.service_view.action_restart(),
                KeyCode::Char('K') | KeyCode::Char('k') => self.service_view.action_stop(),
                KeyCode::Char('S') | KeyCode::Char('s') => self.service_view.action_start(),
                KeyCode::Char('N') | KeyCode::Char('n') => self.service_view.action_new_session(),
                KeyCode::Char('u') => self.service_view.refresh_health(),
                KeyCode::Char('f') => self.service_view.logs.cycle_filter(),
                KeyCode::Char('a') => self.service_view.logs.toggle_auto_scroll(),
                KeyCode::Char('c') => self.service_view.logs.clear(),
                KeyCode::Up => self.service_view.logs.scroll_up(),
                KeyCode::Down => self.service_view.logs.scroll_down(),
                KeyCode::PageUp => self.service_view.logs.page_up(10),
                KeyCode::PageDown => self.service_view.logs.page_down(10),
                _ => {}
            },
        }
    }

    /// Renders the complete dashboard layout inside the terminal frame.
    pub fn render(&mut self, f: &mut Frame) {
        let area = f.area();
        self.last_terminal_width = area.width;
        self.last_terminal_height = area.height;

        // ST-RUST-29.3: Enforce minimum terminal dimensions (80x24)
        if area.width < MIN_TERMINAL_WIDTH || area.height < MIN_TERMINAL_HEIGHT {
            let too_small = Paragraph::new(vec![
                Line::from(Span::styled(
                    "Dimensiones de Terminal insuficientes",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(format!("Tamaño actual:   {}x{}", area.width, area.height)),
                Line::from(format!(
                    "Mínimo requerido: {}x{}",
                    MIN_TERMINAL_WIDTH, MIN_TERMINAL_HEIGHT
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "Por favor, amplíe la ventana para continuar.",
                    Style::default().fg(Color::DarkGray),
                )),
            ])
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Advertencia "),
            );

            let dialog_w = 56.min(area.width);
            let dialog_h = 8.min(area.height);
            let dialog_area = Rect::new(
                (area.width.saturating_sub(dialog_w)) / 2,
                (area.height.saturating_sub(dialog_h)) / 2,
                dialog_w,
                dialog_h,
            );

            f.render_widget(Clear, dialog_area);
            f.render_widget(too_small, dialog_area);
            return;
        }

        let main_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Navigation Tabs bar
                Constraint::Min(18),   // Active tab body
                Constraint::Length(1), // Global hotkeys status line
            ])
            .split(area);

        // 1. Navigation Tabs Header
        let titles: Vec<Line> = ActiveTab::ALL
            .iter()
            .map(|t| {
                let style = if t.index() == self.active_tab.index() {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::DarkGray)
                };
                Line::from(Span::styled(t.label(), style))
            })
            .collect();

        let tabs_widget = Tabs::new(titles)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Viernes Terminal Dashboard v0.1.0 "),
            )
            .select(self.active_tab.index())
            .highlight_style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
                    .add_modifier(Modifier::UNDERLINED),
            );
        f.render_widget(tabs_widget, main_chunks[0]);

        // 2. Active Tab Body
        match self.active_tab {
            ActiveTab::Monitor => {
                let monitor_chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(8), // Soundwave Spectrum
                        Constraint::Min(6),    // Scrollable Conversation history
                        Constraint::Length(3), // Interactive PTT Emulator bar
                    ])
                    .split(main_chunks[1]);

                let spectrum = SpectrumWidget::new(&self.spectrum_bars, self.assistant_state);
                f.render_widget(spectrum, monitor_chunks[0]);

                let conversation = ConversationWidget::new(&self.turns);
                f.render_widget(conversation, monitor_chunks[1]);

                let ptt =
                    PttWidget::new(self.ptt_active, self.assistant_state, self.daemon_connected);
                f.render_widget(ptt, monitor_chunks[2]);
            }
            ActiveTab::VoiceAndKeys => {
                self.credentials_view.render(f, main_chunks[1]);
            }
            ActiveTab::Fillers => {
                self.fillers_view.render(f, main_chunks[1]);
            }
            ActiveTab::AudioDevices => {
                self.devices_view.render(f, main_chunks[1]);
            }
            ActiveTab::ServiceAndLogs => {
                self.service_view.render(f, main_chunks[1]);
            }
        }

        // 3. Global hotkeys footer
        let footer_line = Line::from(vec![
            Span::styled(
                "[1-5]: ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("Pestañas │ "),
            Span::styled(
                "[Tab]: ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("Foco │ "),
            Span::styled(
                "[Espacio / Enter]: ",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("PTT │ "),
            Span::styled(
                "[Esc]: ",
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("Barge-in │ "),
            Span::styled(
                "[q]: ",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            Span::raw("Salir"),
        ]);
        let footer = Paragraph::new(footer_line).alignment(Alignment::Center);
        f.render_widget(footer, main_chunks[2]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn test_tui_app_tab_cycling() {
        let mut app = TuiApp::new(None);
        assert_eq!(app.active_tab, ActiveTab::Monitor);

        app.next_tab();
        assert_eq!(app.active_tab, ActiveTab::VoiceAndKeys);

        app.next_tab();
        assert_eq!(app.active_tab, ActiveTab::Fillers);

        app.prev_tab();
        assert_eq!(app.active_tab, ActiveTab::VoiceAndKeys);
    }

    #[tokio::test]
    async fn test_tui_app_key_quit() {
        let mut app = TuiApp::new(None);
        assert!(!app.should_quit);

        app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE))
            .await;
        assert!(app.should_quit);
    }

    #[tokio::test]
    async fn test_ptt_key_toggle_in_monitor() {
        let mut app = TuiApp::new(None);
        assert_eq!(app.active_tab, ActiveTab::Monitor);
        assert!(!app.ptt_active);
        assert_eq!(app.assistant_state, SystemState::Idle);

        // Press Space -> PTT active / Listening
        app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE))
            .await;
        assert!(app.ptt_active);
        assert_eq!(app.assistant_state, SystemState::Listening);

        // Advance beyond debounce window
        tokio::time::sleep(std::time::Duration::from_millis(260)).await;

        // Press Enter -> PTT inactive / Thinking
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .await;
        assert!(!app.ptt_active);
        assert_eq!(app.assistant_state, SystemState::Thinking);

        // Press Esc -> Cancel / Idle
        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
            .await;
        assert_eq!(app.assistant_state, SystemState::Idle);
    }

    #[test]
    fn test_tui_app_render_headless_normal() {
        let mut app = TuiApp::new(None);
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                app.render(f);
            })
            .unwrap();

        let buffer_str = format!("{:?}", terminal.backend().buffer());
        assert!(buffer_str.contains("Viernes Terminal Dashboard"));
        assert!(buffer_str.contains("Espectro de Voz"));
        assert!(buffer_str.contains("Transcripción & Historial Conversacional"));
        assert!(buffer_str.contains("Control Push-to-Talk"));
    }

    #[test]
    fn test_tui_app_render_headless_too_small_triggers_warning() {
        let mut app = TuiApp::new(None);
        let backend = TestBackend::new(70, 20); // Less than 80x24
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                app.render(f);
            })
            .unwrap();

        let buffer_str = format!("{:?}", terminal.backend().buffer());
        assert!(buffer_str.contains("Dimensiones de Terminal insuficientes"));
        assert!(buffer_str.contains("70x20"));
        assert!(buffer_str.contains("80x24"));
    }
}
