//! Daemon Health Monitor, Service Control, and Session Lifecycle View (Tab 5).

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use super::logs::LogsView;

/// Default socket path used for IPC communication with the daemon.
pub const DEFAULT_IPC_SOCKET: &str = "/tmp/viernes_ptt.sock";
/// Systemd user service unit name.
pub const SYSTEMD_SERVICE_UNIT: &str = "viernes-rust.service";

/// Snapshot of the running Viernes daemon health metrics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonHealth {
    /// Indicates whether the daemon process is actively executing.
    pub is_active: bool,
    /// Operating system Process Identifier (PID), if active.
    pub pid: Option<u32>,
    /// Elapsed execution time in seconds.
    pub uptime_seconds: u64,
    /// Resident memory usage (RSS) in bytes.
    pub memory_rss_bytes: u64,
    /// Whether the IPC Unix Domain Socket is reachable and accepting connections.
    pub ipc_connected: bool,
    /// Method utilized to query status (e.g. "Systemd Unit", "Procfs").
    pub source: String,
}

impl Default for DaemonHealth {
    fn default() -> Self {
        Self {
            is_active: false,
            pid: None,
            uptime_seconds: 0,
            memory_rss_bytes: 0,
            ipc_connected: false,
            source: "None".into(),
        }
    }
}

impl DaemonHealth {
    /// Formats uptime into a human-readable string (e.g. "1h 24m 12s").
    #[must_use]
    pub fn format_uptime(&self) -> String {
        if !self.is_active || self.uptime_seconds == 0 {
            return "0s".to_string();
        }
        let hours = self.uptime_seconds / 3600;
        let minutes = (self.uptime_seconds % 3600) / 60;
        let seconds = self.uptime_seconds % 60;

        if hours > 0 {
            format!("{hours}h {minutes}m {seconds}s")
        } else if minutes > 0 {
            format!("{minutes}m {seconds}s")
        } else {
            format!("{seconds}s")
        }
    }

    /// Formats resident memory usage into megabytes.
    #[must_use]
    pub fn format_memory(&self) -> String {
        if !self.is_active || self.memory_rss_bytes == 0 {
            return "0 MB".to_string();
        }
        let mb = self.memory_rss_bytes as f64 / (1024.0 * 1024.0);
        format!("{mb:.1} MB")
    }
}

/// Abstract runner for executing service control commands (mockable for tests).
pub trait ServiceControlRunner: Send + Sync + std::fmt::Debug {
    /// Starts the daemon service.
    fn start_service(&self) -> Result<(), String>;
    /// Stops the daemon service.
    fn stop_service(&self) -> Result<(), String>;
    /// Restarts the daemon service.
    fn restart_service(&self) -> Result<(), String>;
    /// Dispatches dynamic session reset.
    fn reset_session(&self) -> Result<(), String>;
    /// Probes daemon health status.
    fn probe_health(&self) -> DaemonHealth;
}

/// Production service control runner querying `/proc` and systemd user services.
#[derive(Debug, Default)]
pub struct SystemdServiceRunner {
    ipc_socket_path: PathBuf,
}

impl SystemdServiceRunner {
    /// Constructs a runner with default socket path.
    #[must_use]
    pub fn new() -> Self {
        let path =
            std::env::var("VIERNES_PTT_SOCKET").unwrap_or_else(|_| DEFAULT_IPC_SOCKET.to_string());
        Self {
            ipc_socket_path: PathBuf::from(path),
        }
    }

    /// Scans `/proc` in safe Rust to locate any active `viernes --daemon` processes.
    pub fn scan_procfs() -> Option<(u32, u64, u64)> {
        let proc_dir = Path::new("/proc");
        if !proc_dir.exists() {
            return None;
        }

        let entries = fs::read_dir(proc_dir).ok()?;
        for entry in entries.flatten() {
            let file_name = entry.file_name();
            let pid_str = file_name.to_str()?;
            if let Ok(pid) = pid_str.parse::<u32>() {
                let cmdline_path = entry.path().join("cmdline");
                if let Ok(cmdline) = fs::read_to_string(&cmdline_path) {
                    if cmdline.contains("viernes") && cmdline.contains("--daemon") {
                        // Read memory from /proc/[pid]/statm (second entry is RSS pages)
                        let mut rss_bytes = 0;
                        if let Ok(statm) = fs::read_to_string(entry.path().join("statm")) {
                            if let Some(rss_pages_str) = statm.split_whitespace().nth(1) {
                                if let Ok(pages) = rss_pages_str.parse::<u64>() {
                                    rss_bytes = pages * 4096; // Standard Linux page size
                                }
                            }
                        }

                        // Approximate uptime via stat mtime
                        let mut uptime = 0;
                        if let Ok(meta) = fs::metadata(&cmdline_path) {
                            if let Ok(elapsed) = meta
                                .modified()
                                .and_then(|t| t.elapsed().map_err(std::io::Error::other))
                            {
                                uptime = elapsed.as_secs();
                            }
                        }

                        return Some((pid, uptime, rss_bytes));
                    }
                }
            }
        }
        None
    }

    /// Checks whether the IPC socket file exists and responds.
    pub fn probe_ipc_socket(&self) -> bool {
        self.ipc_socket_path.exists()
    }
}

impl ServiceControlRunner for SystemdServiceRunner {
    fn start_service(&self) -> Result<(), String> {
        let status = std::process::Command::new("systemctl")
            .args(["--user", "start", SYSTEMD_SERVICE_UNIT])
            .status();

        match status {
            Ok(s) if s.success() => Ok(()),
            Ok(s) => Err(format!("systemctl returned code {:?}", s.code())),
            Err(e) => Err(format!("Failed executing systemctl start: {e}")),
        }
    }

    fn stop_service(&self) -> Result<(), String> {
        let status = std::process::Command::new("systemctl")
            .args(["--user", "stop", SYSTEMD_SERVICE_UNIT])
            .status();

        match status {
            Ok(s) if s.success() => Ok(()),
            Ok(_) => {
                // Fallback: kill via PID from procfs
                if let Some((pid, _, _)) = Self::scan_procfs() {
                    let _ = std::process::Command::new("kill")
                        .arg(pid.to_string())
                        .status();
                    return Ok(());
                }
                Err("Failed to stop service via systemctl and no PID found".into())
            }
            Err(e) => Err(format!("Failed executing systemctl stop: {e}")),
        }
    }

    fn restart_service(&self) -> Result<(), String> {
        let status = std::process::Command::new("systemctl")
            .args(["--user", "restart", SYSTEMD_SERVICE_UNIT])
            .status();

        match status {
            Ok(s) if s.success() => Ok(()),
            Ok(s) => Err(format!("systemctl restart returned {:?}", s.code())),
            Err(e) => Err(format!("Failed executing systemctl restart: {e}")),
        }
    }

    fn reset_session(&self) -> Result<(), String> {
        let status = std::process::Command::new("viernes")
            .arg("--new-conversation")
            .status();

        match status {
            Ok(s) if s.success() => Ok(()),
            Ok(s) => Err(format!(
                "viernes --new-conversation returned {:?}",
                s.code()
            )),
            Err(e) => Err(format!("Failed invoking viernes --new-conversation: {e}")),
        }
    }

    fn probe_health(&self) -> DaemonHealth {
        let ipc_connected = self.probe_ipc_socket();

        // 1. Query systemd is-active
        let is_systemd_active = std::process::Command::new("systemctl")
            .args(["--user", "is-active", SYSTEMD_SERVICE_UNIT])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "active")
            .unwrap_or(false);

        if let Some((pid, uptime, rss)) = Self::scan_procfs() {
            return DaemonHealth {
                is_active: true,
                pid: Some(pid),
                uptime_seconds: uptime,
                memory_rss_bytes: rss,
                ipc_connected,
                source: if is_systemd_active {
                    "systemd user unit"
                } else {
                    "procfs standalone"
                }
                .into(),
            };
        }

        if is_systemd_active {
            return DaemonHealth {
                is_active: true,
                pid: None,
                uptime_seconds: 0,
                memory_rss_bytes: 0,
                ipc_connected,
                source: "systemd user unit".into(),
            };
        }

        DaemonHealth {
            is_active: false,
            pid: None,
            uptime_seconds: 0,
            memory_rss_bytes: 0,
            ipc_connected,
            source: "Inactivo".into(),
        }
    }
}

/// State and UI rendering for Tab 5: Service Management, Health & Live Logs.
#[derive(Debug)]
pub struct ServiceView {
    /// Runner executing system commands.
    pub runner: Box<dyn ServiceControlRunner>,
    /// Cached health telemetry.
    pub health: DaemonHealth,
    /// Embedded live log streamer view.
    pub logs: LogsView,
    /// Toast notification banner message.
    pub toast_message: Option<(String, bool)>, // (Message, is_error)
    /// Timestamp when toast message was published.
    pub toast_timestamp: Option<Instant>,
}

impl Default for ServiceView {
    fn default() -> Self {
        Self::new()
    }
}

impl ServiceView {
    /// Creates a production `ServiceView` querying live host services.
    #[must_use]
    pub fn new() -> Self {
        let runner = Box::new(SystemdServiceRunner::new());
        let health = runner.probe_health();
        Self {
            runner,
            health,
            logs: LogsView::new(),
            toast_message: None,
            toast_timestamp: None,
        }
    }

    /// Creates a mock `ServiceView` for testing.
    #[must_use]
    pub fn new_mock(runner: Box<dyn ServiceControlRunner>) -> Self {
        let health = runner.probe_health();
        Self {
            runner,
            health,
            logs: LogsView::new(),
            toast_message: None,
            toast_timestamp: None,
        }
    }

    /// Refreshes health status.
    pub fn refresh_health(&mut self) {
        self.health = self.runner.probe_health();
    }

    /// Sets a transient toast message expiring after 5 seconds.
    pub fn set_toast(&mut self, message: impl Into<String>, is_error: bool) {
        self.toast_message = Some((message.into(), is_error));
        self.toast_timestamp = Some(Instant::now());
    }

    /// Clears expired toast message (> 5 seconds).
    pub fn check_toast_expiration(&mut self) {
        if let Some(t) = self.toast_timestamp {
            if t.elapsed() > Duration::from_secs(5) {
                self.toast_message = None;
                self.toast_timestamp = None;
            }
        }
    }

    /// Action: Starts the daemon service.
    pub fn action_start(&mut self) {
        self.logs
            .push_line("Orden recibida: [S] Iniciar Servicio...");
        match self.runner.start_service() {
            Ok(()) => {
                self.set_toast("Servicio iniciado exitosamente", false);
                self.logs.push_line("[INFO] Daemon iniciado con éxito.");
                self.refresh_health();
            }
            Err(e) => {
                self.set_toast(format!("Error al iniciar: {e}"), true);
                self.logs
                    .push_line(format!("[ERROR] Fallo al iniciar servicio: {e}"));
            }
        }
    }

    /// Action: Stops the daemon service (`K`).
    pub fn action_stop(&mut self) {
        self.logs
            .push_line("Orden recibida: [K] Detener Servicio...");
        match self.runner.stop_service() {
            Ok(()) => {
                self.set_toast("Servicio detenido correctamente", false);
                self.logs.push_line("[INFO] Daemon detenido.");
                self.refresh_health();
            }
            Err(e) => {
                self.set_toast(format!("Error al detener: {e}"), true);
                self.logs
                    .push_line(format!("[ERROR] Fallo al detener servicio: {e}"));
            }
        }
    }

    /// Action: Restarts the daemon service (`R`).
    pub fn action_restart(&mut self) {
        self.logs
            .push_line("Orden recibida: [R] Reiniciar Servicio...");
        match self.runner.restart_service() {
            Ok(()) => {
                self.set_toast("Servicio reiniciado exitosamente", false);
                self.logs.push_line("[INFO] Daemon reiniciado con éxito.");
                self.refresh_health();
            }
            Err(e) => {
                self.set_toast(format!("Error al reiniciar: {e}"), true);
                self.logs
                    .push_line(format!("[ERROR] Fallo al reiniciar servicio: {e}"));
            }
        }
    }

    /// Action: Rotates dynamic conversation session (`N`).
    pub fn action_new_session(&mut self) {
        self.logs
            .push_line("Orden recibida: [N] Rotar Nueva Sesión...");
        match self.runner.reset_session() {
            Ok(()) => {
                self.set_toast("Nueva sesión conversacional iniciada", false);
                self.logs
                    .push_line("[INFO] Puntero de sesión rotado (--new-conversation).");
            }
            Err(e) => {
                self.set_toast(format!("Error al rotar sesión: {e}"), true);
                self.logs
                    .push_line(format!("[ERROR] Fallo al resetear sesión: {e}"));
            }
        }
    }

    /// Renders Tab 5 inside the provided frame and area.
    pub fn render(&mut self, f: &mut Frame, area: Rect) {
        self.check_toast_expiration();

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(8), // Status & Actions Panel
                Constraint::Min(8),    // Live Logs stream widget
                Constraint::Length(2), // Bottom Action bar
            ])
            .split(area);

        // 1. Status Panel Block
        let (status_badge, badge_color) = if self.health.is_active {
            ("● ACTIVO (ONLINE)", Color::Green)
        } else {
            ("○ DETENIDO (INACTIVO)", Color::Red)
        };

        let pid_str = self
            .health
            .pid
            .map(|p| p.to_string())
            .unwrap_or_else(|| "N/A".to_string());

        let ipc_str = if self.health.ipc_connected {
            "Conectado (/tmp/viernes_ptt.sock)"
        } else {
            "Desconectado"
        };
        let ipc_color = if self.health.ipc_connected {
            Color::Green
        } else {
            Color::Yellow
        };

        let status_lines = vec![
            Line::from(vec![
                Span::styled("Estado Daemon:  ", Style::default().fg(Color::Gray)),
                Span::styled(
                    status_badge,
                    Style::default()
                        .fg(badge_color)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" │ Origen: "),
                Span::styled(&self.health.source, Style::default().fg(Color::Cyan)),
            ]),
            Line::from(vec![
                Span::styled("PID Proceso:    ", Style::default().fg(Color::Gray)),
                Span::styled(pid_str, Style::default().fg(Color::White)),
                Span::raw(" │ Tiempo activo (Uptime): "),
                Span::styled(
                    self.health.format_uptime(),
                    Style::default().fg(Color::Yellow),
                ),
                Span::raw(" │ Memoria RSS: "),
                Span::styled(
                    self.health.format_memory(),
                    Style::default().fg(Color::Magenta),
                ),
            ]),
            Line::from(vec![
                Span::styled("Canal IPC PTT:  ", Style::default().fg(Color::Gray)),
                Span::styled(ipc_str, Style::default().fg(ipc_color)),
            ]),
            Line::from(vec![
                Span::raw("Acciones: "),
                Span::styled("[ [R] Reiniciar ] ", Style::default().fg(Color::Cyan)),
                Span::styled("[ [K] Detener ] ", Style::default().fg(Color::Red)),
                Span::styled("[ [S] Iniciar ] ", Style::default().fg(Color::Green)),
                Span::styled("[ [N] Nueva Sesión ] ", Style::default().fg(Color::Yellow)),
            ]),
        ];

        let mut status_block = Block::default()
            .borders(Borders::ALL)
            .title(" [5] SERVICIO DAEMON & CICLO DE VIDA (systemd / IPC) ")
            .border_style(Style::default().fg(Color::DarkGray));

        // Toast message overlay if active
        if let Some((ref msg, is_err)) = self.toast_message {
            let toast_col = if is_err { Color::Red } else { Color::Green };
            status_block = status_block.title_bottom(Line::from(vec![
                Span::styled(" AVISO: ", Style::default().bg(toast_col).fg(Color::Black)),
                Span::styled(format!(" {msg} "), Style::default().fg(toast_col)),
            ]));
        }

        let status_p = Paragraph::new(status_lines).block(status_block);
        f.render_widget(status_p, chunks[0]);

        // 2. Live Logs Widget
        self.logs.render(f, chunks[1]);

        // 3. Actions Footer
        let footer = Paragraph::new(Line::from(vec![
            Span::styled("[R]: ", Style::default().fg(Color::Yellow)),
            Span::raw("Reiniciar Daemon │ "),
            Span::styled("[K]: ", Style::default().fg(Color::Yellow)),
            Span::raw("Detener Daemon │ "),
            Span::styled("[S]: ", Style::default().fg(Color::Yellow)),
            Span::raw("Iniciar Daemon │ "),
            Span::styled("[N]: ", Style::default().fg(Color::Yellow)),
            Span::raw("Nueva Sesión (--new-conversation) │ "),
            Span::styled("[u]: ", Style::default().fg(Color::Yellow)),
            Span::raw("Actualizar Estado"),
        ]))
        .alignment(Alignment::Center);
        f.render_widget(footer, chunks[2]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[derive(Debug, Default)]
    struct MockRunner {
        should_fail: bool,
        active: bool,
    }

    impl ServiceControlRunner for MockRunner {
        fn start_service(&self) -> Result<(), String> {
            if self.should_fail {
                Err("Fallo simulado al iniciar".into())
            } else {
                Ok(())
            }
        }

        fn stop_service(&self) -> Result<(), String> {
            if self.should_fail {
                Err("Fallo simulado al detener".into())
            } else {
                Ok(())
            }
        }

        fn restart_service(&self) -> Result<(), String> {
            if self.should_fail {
                Err("Fallo simulado al reiniciar".into())
            } else {
                Ok(())
            }
        }

        fn reset_session(&self) -> Result<(), String> {
            if self.should_fail {
                Err("Fallo simulado al resetear".into())
            } else {
                Ok(())
            }
        }

        fn probe_health(&self) -> DaemonHealth {
            DaemonHealth {
                is_active: self.active,
                pid: if self.active { Some(12345) } else { None },
                uptime_seconds: 3665,
                memory_rss_bytes: 45 * 1024 * 1024,
                ipc_connected: self.active,
                source: "MockRunner".into(),
            }
        }
    }

    #[test]
    fn test_daemon_health_formatters() {
        let health = DaemonHealth {
            is_active: true,
            pid: Some(42),
            uptime_seconds: 3665,               // 1h 1m 5s
            memory_rss_bytes: 52 * 1024 * 1024, // 52 MB
            ipc_connected: true,
            source: "test".into(),
        };

        assert_eq!(health.format_uptime(), "1h 1m 5s");
        assert_eq!(health.format_memory(), "52.0 MB");
    }

    #[test]
    fn test_service_view_actions_success() {
        let runner = Box::new(MockRunner {
            should_fail: false,
            active: true,
        });
        let mut view = ServiceView::new_mock(runner);

        assert!(view.health.is_active);
        assert_eq!(view.health.pid, Some(12345));

        view.action_restart();
        assert!(view.toast_message.is_some());
        assert!(!view.toast_message.as_ref().unwrap().1); // not error
        assert!(!view.logs.is_empty());

        view.action_new_session();
        assert!(view
            .toast_message
            .as_ref()
            .unwrap()
            .0
            .contains("Nueva sesión"));
    }

    #[test]
    fn test_service_view_actions_failure() {
        let runner = Box::new(MockRunner {
            should_fail: true,
            active: false,
        });
        let mut view = ServiceView::new_mock(runner);

        view.action_start();
        assert!(view.toast_message.is_some());
        assert!(view.toast_message.as_ref().unwrap().1); // is error
        assert!(view
            .toast_message
            .as_ref()
            .unwrap()
            .0
            .contains("Fallo simulado"));
    }

    #[test]
    fn test_service_view_render_headless() {
        let runner = Box::new(MockRunner {
            should_fail: false,
            active: true,
        });
        let mut view = ServiceView::new_mock(runner);
        view.logs.push_line("12:00:00 [INFO] Daemon initialized");

        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                view.render(f, f.area());
            })
            .unwrap();
    }
}
