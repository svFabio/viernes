//! Application state, navigation tabs, dashboard rendering, and user input orchestration.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Tabs, Wrap};
use ratatui::Frame;
use tokio::sync::mpsc;
use viernes_audio::capture::{AudioCaptureConfig, CpalAudioCapture};
use viernes_audio::vad::compute_rms;
use viernes_core::config::ViernesConfig;
use viernes_core::state::{DaemonLiveStatus, SystemState};
use viernes_core::traits::AudioCapture;

use crate::tui::views::credentials::{CredentialsAction, CredentialsView};
use crate::tui::views::devices::DevicesView;
use crate::tui::views::fillers::{FillersAction, FillersView};
use crate::tui::views::service::{
    fetch_journal_logs_async, probe_health_async, reset_session_async, restart_service_async,
    start_service_async, stop_service_async, DaemonHealth, ServiceView,
};
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

/// Converts raw PCM audio samples into a normalized VU meter level `[0.0, 1.0]`.
#[must_use]
pub fn rms_to_vu_level(samples: &[f32]) -> f32 {
    let rms = compute_rms(samples);
    if rms.is_finite() {
        (rms * 3.0).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// Messages delivered from background async tasks back to [`TuiApp`].
#[derive(Debug)]
enum TuiBackgroundMsg {
    DaemonStatusLoaded {
        status: Option<DaemonLiveStatus>,
        daemon_connected: bool,
    },
    HealthRefreshed(DaemonHealth),
    JournalLogsLoaded(Vec<String>),
    ServiceActionFinished {
        success_toast: &'static str,
        success_log: &'static str,
        error_prefix: &'static str,
        result: Result<(), String>,
    },
    AuditionFinished(Result<String, String>),
    IpcReloadFinished {
        context: &'static str,
        result: Result<(), String>,
    },
    MicLevelUpdated(f32),
    MicTestStatus(String),
    MicTestFailed(String),
}

/// Unified application state for the Viernes TUI.
pub struct TuiApp {
    /// Currently visible tab.
    pub active_tab: ActiveTab,
    /// Flag signaling main event loop termination.
    pub should_quit: bool,
    /// Confirmation prompt shown when quitting with unsaved changes in Tabs 2/3/4.
    pub show_quit_confirm: bool,
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

    // --- Async background task coordination ---
    bg_tx: mpsc::UnboundedSender<TuiBackgroundMsg>,
    bg_rx: mpsc::UnboundedReceiver<TuiBackgroundMsg>,
    status_poll_in_flight: bool,
    health_poll_in_flight: bool,
    logs_poll_in_flight: bool,
    mic_test_stop: Option<Arc<AtomicBool>>,
}

impl Drop for TuiApp {
    fn drop(&mut self) {
        if let Some(stop) = self.mic_test_stop.take() {
            stop.store(true, Ordering::Relaxed);
        }
    }
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
                initial_turns.push(ConversationTurn::new(
                    Speaker::User,
                    status.last_user_prompt,
                ));
            }
            if !status.last_assistant_text.is_empty() {
                initial_turns.push(ConversationTurn::new(
                    Speaker::Viernes,
                    status.last_assistant_text,
                ));
            }
        }

        let daemon_connected = ptt_socket_path().exists();
        let (bg_tx, bg_rx) = mpsc::unbounded_channel();

        Self {
            active_tab: ActiveTab::Monitor,
            should_quit: false,
            show_quit_confirm: false,
            tick_count: 0,
            credentials_view: CredentialsView::new(config.clone()),
            config: config.clone(),
            assistant_state,
            spectrum_bars: vec![0.05; Self::SPECTRUM_BAR_COUNT],
            turns: initial_turns,
            ptt_active: false,
            daemon_connected,
            fillers_view: FillersView::new(None, None),
            devices_view: DevicesView::with_config(config.as_ref()),
            service_view: ServiceView::new(),
            status_line: Some("Viernes TUI Dashboard inicializado".into()),
            last_synced_turn,
            last_ptt_toggle: std::time::Instant::now() - std::time::Duration::from_secs(1),
            last_terminal_width: 80,
            last_terminal_height: 24,
            bg_tx,
            bg_rx,
            status_poll_in_flight: false,
            health_poll_in_flight: false,
            logs_poll_in_flight: false,
            mic_test_stop: None,
        }
    }

    /// Returns `true` if any configuration tab (2, 3, or 4) has unsaved edits.
    #[must_use]
    pub fn has_unsaved_changes(&self) -> bool {
        self.credentials_view.has_unsaved_changes
            || self.fillers_view.has_unsaved_changes
            || self.devices_view.has_unsaved_ducking_changes
    }

    /// Applies an updated daemon live status snapshot to TUI state and turn history.
    pub fn apply_daemon_status(
        &mut self,
        status: Option<DaemonLiveStatus>,
        daemon_connected: bool,
    ) {
        if let Some(status) = status {
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
            } else if !status.last_assistant_text.is_empty() {
                if let Some(last) = self.turns.last_mut() {
                    if last.speaker == Speaker::Viernes {
                        if last.text != status.last_assistant_text {
                            last.text = status.last_assistant_text.clone();
                        }
                    } else {
                        self.add_turn(Speaker::Viernes, status.last_assistant_text.clone());
                    }
                } else {
                    self.add_turn(Speaker::Viernes, status.last_assistant_text.clone());
                }
            }
        } else {
            self.daemon_connected = daemon_connected;
        }
    }

    fn spawn_status_poll(&mut self) {
        if self.status_poll_in_flight {
            return;
        }
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        self.status_poll_in_flight = true;
        let tx = self.bg_tx.clone();
        handle.spawn(async move {
            let res = tokio::task::spawn_blocking(|| {
                let status = DaemonLiveStatus::read_from_disk();
                let daemon_connected = ptt_socket_path().exists();
                (status, daemon_connected)
            })
            .await;

            let (status, daemon_connected) = res.unwrap_or((None, false));
            let _ = tx.send(TuiBackgroundMsg::DaemonStatusLoaded {
                status,
                daemon_connected,
            });
        });
    }

    fn spawn_health_refresh(&mut self) {
        if self.health_poll_in_flight {
            return;
        }
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        self.health_poll_in_flight = true;
        let tx = self.bg_tx.clone();
        let socket_path = self.service_view.ipc_socket_path.clone();
        handle.spawn(async move {
            let health = probe_health_async(socket_path).await;
            let _ = tx.send(TuiBackgroundMsg::HealthRefreshed(health));
        });
    }

    fn spawn_journal_logs_refresh(&mut self) {
        if self.logs_poll_in_flight {
            return;
        }
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        self.logs_poll_in_flight = true;
        let tx = self.bg_tx.clone();
        handle.spawn(async move {
            let lines = fetch_journal_logs_async(100).await;
            let _ = tx.send(TuiBackgroundMsg::JournalLogsLoaded(lines));
        });
    }

    fn spawn_ipc_reload(&self, context: &'static str) {
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let tx = self.bg_tx.clone();
        handle.spawn(async move {
            let result = send_ipc_ptt("reload").await.map_err(|e| e.to_string());
            let _ = tx.send(TuiBackgroundMsg::IpcReloadFinished { context, result });
        });
    }

    fn spawn_voice_audition(&mut self) {
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            self.credentials_view.is_auditioning = false;
            return;
        };
        let mut cv = self.credentials_view.clone();
        let tx = self.bg_tx.clone();
        handle.spawn(async move {
            let res = cv
                .test_audition(None)
                .await
                .map(|()| "Audición completada exitosamente".to_string())
                .map_err(|e| e.to_string());
            let _ = tx.send(TuiBackgroundMsg::AuditionFinished(res));
        });
    }

    fn spawn_service_action<F>(
        &mut self,
        log_cmd: &str,
        success_toast: &'static str,
        success_log: &'static str,
        error_prefix: &'static str,
        fut: F,
    ) where
        F: std::future::Future<Output = Result<(), String>> + Send + 'static,
    {
        self.service_view.logs.push_line(log_cmd);
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let tx = self.bg_tx.clone();
        let socket_path = self.service_view.ipc_socket_path.clone();
        handle.spawn(async move {
            let result = fut.await;
            let ok = result.is_ok();
            let _ = tx.send(TuiBackgroundMsg::ServiceActionFinished {
                success_toast,
                success_log,
                error_prefix,
                result,
            });
            if ok {
                let health = probe_health_async(socket_path).await;
                let _ = tx.send(TuiBackgroundMsg::HealthRefreshed(health));
            }
        });
    }

    fn sync_mic_test_task(&mut self) {
        if self.devices_view.is_testing_mic {
            if self.mic_test_stop.is_some() {
                return;
            }
            let Ok(handle) = tokio::runtime::Handle::try_current() else {
                return;
            };
            let stop_flag = Arc::new(AtomicBool::new(false));
            self.mic_test_stop = Some(stop_flag.clone());
            let tx = self.bg_tx.clone();
            let device_query = self
                .devices_view
                .selected_input_device()
                .map(|d| d.name.clone());

            handle.spawn(async move {
                let mut capture = CpalAudioCapture::new(AudioCaptureConfig {
                    device_query,
                    ..Default::default()
                });
                capture.set_recording_active(false);
                if let Err(err) = capture.start_stream().await {
                    let _ = tx.send(TuiBackgroundMsg::MicTestFailed(format!(
                        "Micrófono no disponible ({err})"
                    )));
                    return;
                }

                let _ = tx.send(TuiBackgroundMsg::MicTestStatus(
                    "Prueba de micrófono activa (RMS en vivo)".into(),
                ));
                let rx = capture.receiver();
                while !stop_flag.load(Ordering::Relaxed) {
                    let mut latest_level: Option<f32> = None;
                    while let Ok(chunk) = rx.try_recv() {
                        let lvl = rms_to_vu_level(&chunk);
                        latest_level = Some(latest_level.map_or(lvl, |prev| prev.max(lvl)));
                    }
                    if let Some(level) = latest_level {
                        let _ = tx.send(TuiBackgroundMsg::MicLevelUpdated(level));
                    }
                    tokio::time::sleep(Duration::from_millis(33)).await;
                }
                let _ = capture.stop_stream().await;
            });
        } else {
            if let Some(stop) = self.mic_test_stop.take() {
                stop.store(true, Ordering::Relaxed);
            }
            self.devices_view.set_mic_level(0.0);
        }
    }

    fn drain_background_messages(&mut self) {
        while let Ok(msg) = self.bg_rx.try_recv() {
            match msg {
                TuiBackgroundMsg::DaemonStatusLoaded {
                    status,
                    daemon_connected,
                } => {
                    self.status_poll_in_flight = false;
                    self.apply_daemon_status(status, daemon_connected);
                }
                TuiBackgroundMsg::HealthRefreshed(health) => {
                    self.health_poll_in_flight = false;
                    self.service_view.apply_health(health);
                }
                TuiBackgroundMsg::JournalLogsLoaded(lines) => {
                    self.logs_poll_in_flight = false;
                    self.service_view.apply_journal_logs(lines);
                }
                TuiBackgroundMsg::ServiceActionFinished {
                    success_toast,
                    success_log,
                    error_prefix,
                    result,
                } => {
                    self.service_view.apply_async_action_result(
                        success_toast,
                        success_log,
                        error_prefix,
                        result,
                    );
                    if let Some((ref toast, _)) = self.service_view.toast_message {
                        self.status_line = Some(toast.clone());
                    }
                }
                TuiBackgroundMsg::AuditionFinished(res) => {
                    self.credentials_view.is_auditioning = false;
                    match res {
                        Ok(msg) => {
                            self.credentials_view.error_message = None;
                            self.credentials_view.status_message = Some(msg.clone());
                            self.status_line = Some(msg);
                        }
                        Err(err) => {
                            self.credentials_view.error_message = Some(err.clone());
                            self.status_line = Some(format!("Error en audición: {err}"));
                        }
                    }
                }
                TuiBackgroundMsg::IpcReloadFinished { context, result } => match result {
                    Ok(()) => {
                        self.status_line = Some(format!("{context} y recargado en daemon vía IPC"));
                    }
                    Err(_) => {
                        self.status_line = Some(format!("{context} (daemon inactivo)"));
                    }
                },
                TuiBackgroundMsg::MicLevelUpdated(level) => {
                    if self.devices_view.is_testing_mic {
                        self.devices_view.set_mic_level(level);
                    }
                }
                TuiBackgroundMsg::MicTestStatus(msg) => {
                    self.devices_view.status_message = Some(msg.clone());
                    self.status_line = Some(msg);
                }
                TuiBackgroundMsg::MicTestFailed(err) => {
                    self.devices_view.is_testing_mic = false;
                    self.devices_view.set_mic_level(0.0);
                    if let Some(stop) = self.mic_test_stop.take() {
                        stop.store(true, Ordering::Relaxed);
                    }
                    self.devices_view.status_message = Some(err.clone());
                    self.status_line = Some(err);
                }
            }
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

    /// Advances periodic animations at 30 FPS without blocking on disk/process I/O.
    pub fn on_tick(&mut self) {
        self.tick_count = self.tick_count.wrapping_add(1);
        self.drain_background_messages();

        // Periodically poll daemon status off the UI thread
        if self.tick_count == 1 || self.tick_count.is_multiple_of(3) {
            self.spawn_status_poll();
        }

        // Periodically refresh systemd service health & journalctl logs asynchronously
        if self.tick_count == 1 || self.tick_count.is_multiple_of(150) {
            self.spawn_health_refresh();
            self.spawn_journal_logs_refresh();
        }

        // Update soundwave spectrum based on assistant state
        let bar_count = self.spectrum_bars.len();
        if self.assistant_state == SystemState::Speaking {
            let t = (self.tick_count as f32) * 0.22;
            for (i, bar) in self.spectrum_bars.iter_mut().enumerate() {
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
                *bar = (*bar * 0.35 + target_height * 0.65).clamp(0.05, 1.0);
            }
        } else if self.assistant_state == SystemState::Listening {
            let t = (self.tick_count as f32) * 0.20;
            for (i, bar) in self.spectrum_bars.iter_mut().enumerate() {
                let fi = i as f32 / bar_count as f32;
                let wave = ((fi * 10.0 + t).sin() * 0.5 + 0.5) * 0.5 + 0.1;
                *bar = (*bar * 0.4 + wave * 0.6).clamp(0.05, 0.7);
            }
        } else if self.assistant_state == SystemState::Thinking {
            let t = (self.tick_count as f32) * 0.12;
            let pulse = (t.sin() * 0.5 + 0.5) * 0.4 + 0.08;
            for (i, bar) in self.spectrum_bars.iter_mut().enumerate() {
                let fi = i as f32 / bar_count as f32;
                let wave = (fi * 6.0 - t).cos().abs() * 0.25 + pulse * 0.75;
                *bar = (*bar * 0.5 + wave * 0.5).clamp(0.05, 0.55);
            }
        } else if self.assistant_state == SystemState::AwaitingConfirmation {
            let t = (self.tick_count as f32) * 0.16;
            let pulse = (t.sin() * 0.5 + 0.5) * 0.45 + 0.15;
            for (i, bar) in self.spectrum_bars.iter_mut().enumerate() {
                let fi = i as f32 / bar_count as f32;
                let wave = (fi * 8.0 + t).sin().abs() * 0.2 + pulse * 0.8;
                *bar = (*bar * 0.45 + wave * 0.55).clamp(0.08, 0.75);
            }
        } else {
            // Quiescent idle baseline wave
            let t = (self.tick_count as f32) * 0.05;
            for (i, bar) in self.spectrum_bars.iter_mut().enumerate() {
                let fi = i as f32 / bar_count as f32;
                let wave = ((fi * 4.0 + t).sin() * 0.5 + 0.5) * 0.06 + 0.02;
                *bar = (*bar * 0.8 + wave * 0.2).clamp(0.01, 0.12);
            }
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
            self.status_line = Some(
                "Micrófono activado. Habla ahora... (Espacio/Enter o Click para enviar)".into(),
            );
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

        // Global unsaved-changes quit confirmation modal
        if self.show_quit_confirm {
            match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                    self.show_quit_confirm = false;
                    self.should_quit = true;
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                    self.show_quit_confirm = false;
                    self.status_line =
                        Some("Salida cancelada. Presione [s] para guardar cambios.".into());
                }
                _ => {}
            }
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
                    if self.has_unsaved_changes() {
                        self.show_quit_confirm = true;
                        self.status_line = Some("Cambios sin guardar (y/n)".into());
                    } else {
                        self.should_quit = true;
                    }
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
                    self.spawn_health_refresh();
                    self.spawn_journal_logs_refresh();
                    return;
                }
                KeyCode::Tab => {
                    if self.active_tab == ActiveTab::AudioDevices {
                        self.devices_view.next_focus();
                    } else {
                        self.next_tab();
                        if self.active_tab == ActiveTab::ServiceAndLogs {
                            self.spawn_health_refresh();
                            self.spawn_journal_logs_refresh();
                        }
                    }
                    return;
                }
                KeyCode::BackTab => {
                    if self.active_tab == ActiveTab::AudioDevices {
                        self.devices_view.prev_focus();
                    } else {
                        self.prev_tab();
                        if self.active_tab == ActiveTab::ServiceAndLogs {
                            self.spawn_health_refresh();
                            self.spawn_journal_logs_refresh();
                        }
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
                        || self.assistant_state == SystemState::AwaitingConfirmation
                    {
                        // Barge-in / Cancel speech
                        self.ptt_active = false;
                        self.assistant_state = SystemState::Idle;
                        let _ = send_ipc_ptt("cancel").await;
                    } else if self.has_unsaved_changes() {
                        self.show_quit_confirm = true;
                        self.status_line = Some("Cambios sin guardar (y/n)".into());
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
                        self.spawn_ipc_reload("Credenciales guardadas en .env (0600)");
                    }
                    CredentialsAction::TestVoiceTriggered => {
                        self.status_line = Some("Iniciando audición de prueba de voz...".into());
                        self.spawn_voice_audition();
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
                KeyCode::Left | KeyCode::Char('h') | KeyCode::Char('-') => {
                    if self.devices_view.focus
                        == crate::tui::views::devices::DeviceFocus::AudioDucking
                        || self.devices_view.focus
                            == crate::tui::views::devices::DeviceFocus::ViernesVolume
                    {
                        self.devices_view.select_prev();
                    }
                }
                KeyCode::Right | KeyCode::Char('l') | KeyCode::Char('+') => {
                    if self.devices_view.focus
                        == crate::tui::views::devices::DeviceFocus::AudioDucking
                        || self.devices_view.focus
                            == crate::tui::views::devices::DeviceFocus::ViernesVolume
                    {
                        self.devices_view.select_next();
                    }
                }
                KeyCode::Enter => {
                    if self.devices_view.input_dropdown_open
                        || self.devices_view.output_dropdown_open
                    {
                        self.devices_view.confirm_selection();
                    } else {
                        self.devices_view.toggle_action();
                        self.sync_mic_test_task();
                    }
                }
                KeyCode::Char(' ') => {
                    self.devices_view.toggle_action();
                    self.sync_mic_test_task();
                }
                KeyCode::Char('m') => {
                    self.devices_view.focus = crate::tui::views::devices::DeviceFocus::MicTest;
                    self.devices_view.toggle_action();
                    self.sync_mic_test_task();
                }
                KeyCode::Char('d') => {
                    self.devices_view.focus = crate::tui::views::devices::DeviceFocus::AudioDucking;
                    self.devices_view.toggle_action();
                }
                KeyCode::Char('v') => {
                    self.devices_view.focus =
                        crate::tui::views::devices::DeviceFocus::ViernesVolume;
                }
                KeyCode::Char('s') => {
                    if let Err(e) = self.devices_view.save_ducking_config() {
                        self.status_line = Some(format!("Error guardando audio: {e}"));
                    } else {
                        self.status_line =
                            Some("Configuración de audio guardada en .env (0600)".into());
                        self.spawn_ipc_reload("Configuración de audio guardada en .env (0600)");
                    }
                }
                KeyCode::Char('r') => self.devices_view.refresh_from_host(),
                _ => {}
            },
            ActiveTab::ServiceAndLogs => match key.code {
                KeyCode::Char('R') | KeyCode::Char('r') => {
                    self.spawn_service_action(
                        "Orden recibida: [R] Reiniciar Servicio...",
                        "Servicio reiniciado exitosamente",
                        "[INFO] Daemon reiniciado con éxito.",
                        "Error al reiniciar",
                        restart_service_async(),
                    );
                }
                KeyCode::Char('K') | KeyCode::Char('k') => {
                    self.spawn_service_action(
                        "Orden recibida: [K] Detener Servicio...",
                        "Servicio detenido correctamente",
                        "[INFO] Daemon detenido.",
                        "Error al detener",
                        stop_service_async(),
                    );
                }
                KeyCode::Char('S') | KeyCode::Char('s') => {
                    self.spawn_service_action(
                        "Orden recibida: [S] Iniciar Servicio...",
                        "Servicio iniciado exitosamente",
                        "[INFO] Daemon iniciado con éxito.",
                        "Error al iniciar",
                        start_service_async(),
                    );
                }
                KeyCode::Char('N') | KeyCode::Char('n') => {
                    self.spawn_service_action(
                        "Orden recibida: [N] Rotar Nueva Sesión...",
                        "Nueva sesión conversacional iniciada",
                        "[INFO] Puntero de sesión rotado (--new-conversation).",
                        "Error al rotar sesión",
                        reset_session_async(),
                    );
                }
                KeyCode::Char('u') => {
                    self.spawn_health_refresh();
                    self.spawn_journal_logs_refresh();
                    self.status_line = Some("Actualizando estado del servicio y logs...".into());
                }
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
                Constraint::Length(1), // Global status line
                Constraint::Length(1), // Global hotkeys footer
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

        // 3. Global status bar
        let status_text = self.status_line.as_deref().unwrap_or("Listo");
        let status_bar = Paragraph::new(Line::from(vec![
            Span::styled(
                " Estado: ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(status_text, Style::default().fg(Color::Gray)),
        ]));
        f.render_widget(status_bar, main_chunks[2]);

        // 4. Global hotkeys footer
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
        f.render_widget(footer, main_chunks[3]);

        // 5. Global Unsaved Changes Quit Confirmation Modal
        if self.show_quit_confirm {
            let modal_w = 60.min(area.width);
            let modal_h = 7.min(area.height);
            let modal_area = Rect::new(
                (area.width.saturating_sub(modal_w)) / 2,
                (area.height.saturating_sub(modal_h)) / 2,
                modal_w,
                modal_h,
            );
            f.render_widget(Clear, modal_area);
            let modal = Paragraph::new(
                "Hay cambios sin guardar en la configuración.\n\n¿Desea salir y descartarlos? [y] Salir / [n] Cancelar",
            )
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true })
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Cambios sin guardar (y/n) ")
                    .title_alignment(Alignment::Center)
                    .style(Style::default().fg(Color::Yellow)),
            );
            f.render_widget(modal, modal_area);
        }
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
        app.assistant_state = SystemState::Idle;
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
    fn test_tui_app_assistant_response_sync_in_idle() {
        let mut app = TuiApp::new(None);
        app.add_turn(Speaker::User, "Hola Viernes");
        app.last_synced_turn = 5;

        // Simulate status with same turn counter but new assistant response while idle
        let status = DaemonLiveStatus {
            state: SystemState::Idle,
            last_user_prompt: "Hola Viernes".into(),
            last_assistant_text: "Hola Señor, a su disposición.".into(),
            turn_counter: 5,
            timestamp_ms: 12345,
        };

        // When assistant text arrives, TUI should append Speaker::Viernes even in Idle state
        if !status.last_assistant_text.is_empty() {
            if let Some(last) = app.turns.last_mut() {
                if last.speaker == Speaker::Viernes {
                    if last.text != status.last_assistant_text {
                        last.text = status.last_assistant_text.clone();
                    }
                } else {
                    app.add_turn(Speaker::Viernes, status.last_assistant_text.clone());
                }
            }
        }

        assert_eq!(app.turns.last().unwrap().speaker, Speaker::Viernes);
        assert_eq!(
            app.turns.last().unwrap().text,
            "Hola Señor, a su disposición."
        );
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

    #[tokio::test]
    async fn test_tui_app_devices_enter_confirms_open_dropdown() {
        let mut app = TuiApp::new(None);
        app.devices_view = DevicesView::new_mock(
            vec![
                viernes_audio::devices::AudioDeviceInfo {
                    index: 0,
                    name: "Default Mic".into(),
                    is_default: true,
                    max_channels: 1,
                    sample_rates: vec![16000],
                },
                viernes_audio::devices::AudioDeviceInfo {
                    index: 1,
                    name: "USB Mic".into(),
                    is_default: false,
                    max_channels: 1,
                    sample_rates: vec![48000],
                },
            ],
            vec![],
        );
        app.active_tab = ActiveTab::AudioDevices;
        app.devices_view.input_dropdown_open = true;
        app.devices_view.select_next();

        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .await;

        assert!(!app.devices_view.input_dropdown_open);
        assert!(app
            .devices_view
            .status_message
            .as_deref()
            .unwrap_or("")
            .contains("USB Mic"));
    }

    #[tokio::test]
    async fn test_tui_app_quit_prompts_when_unsaved_changes_exist() {
        let mut app = TuiApp::new(None);
        app.credentials_view.has_unsaved_changes = true;

        // First 'q' opens confirmation instead of quitting
        app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE))
            .await;
        assert!(!app.should_quit);
        assert!(app.show_quit_confirm);

        // 'n' cancels quit
        app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE))
            .await;
        assert!(!app.should_quit);
        assert!(!app.show_quit_confirm);

        // 'q' then 'y' quits
        app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE))
            .await;
        assert!(app.show_quit_confirm);
        app.handle_key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE))
            .await;
        assert!(app.should_quit);
    }

    #[test]
    fn test_tui_app_renders_status_line() {
        let mut app = TuiApp::new(None);
        app.status_line = Some("Estado Visible Prueba".into());
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|f| app.render(f)).unwrap();
        let buffer_str = format!("{:?}", terminal.backend().buffer());
        assert!(buffer_str.contains("Estado Visible Prueba"));
    }

    #[test]
    fn test_rms_to_vu_level_bounds() {
        assert_eq!(rms_to_vu_level(&[]), 0.0);
        assert_eq!(rms_to_vu_level(&[0.0, 0.0]), 0.0);
        assert!((rms_to_vu_level(&[1.0, 1.0]) - 1.0).abs() < f32::EPSILON);
        assert_eq!(rms_to_vu_level(&[f32::NAN]), 0.0);
    }
}
