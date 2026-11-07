//! Interactive Push-to-Talk emulator widget and Unix domain socket IPC client.

use std::io;
use std::path::Path;

use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget};
use tokio::io::AsyncWriteExt;
use tokio::net::UnixStream;
use viernes_core::state::SystemState;

/// Canonical Unix domain socket endpoint utilized by the background daemon.
pub const DAEMON_PTT_SOCKET: &str = "/tmp/viernes_ptt.sock";

/// Returns the configured PTT IPC socket path.
#[must_use]
pub fn ptt_socket_path() -> String {
    std::env::var("VIERNES_PTT_SOCKET").unwrap_or_else(|_| DAEMON_PTT_SOCKET.to_string())
}

/// Sends a Push-to-Talk trigger command (`press`, `release`, `cancel`) to the background daemon.
pub async fn send_ipc_ptt(command: &str) -> io::Result<()> {
    let socket_path = ptt_socket_path();
    if !Path::new(&socket_path).exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("PTT socket '{socket_path}' does not exist"),
        ));
    }

    let mut stream = UnixStream::connect(&socket_path).await?;
    let payload = format!("{}\n", command.trim());
    stream.write_all(payload.as_bytes()).await?;
    stream.flush().await?;
    Ok(())
}

/// Widget rendering the interactive Push-to-Talk control bar.
pub struct PttWidget {
    ptt_active: bool,
    state: SystemState,
    daemon_connected: bool,
}

impl PttWidget {
    /// Creates a new PTT status bar widget.
    #[must_use]
    pub fn new(ptt_active: bool, state: SystemState, daemon_connected: bool) -> Self {
        Self {
            ptt_active,
            state,
            daemon_connected,
        }
    }
}

impl Widget for PttWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let (label, style, border_color) = if !self.daemon_connected {
            (
                "[○ DAEMON DESCONECTADO] Inicia el servicio con 'viernes --daemon'",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                Color::Red,
            )
        } else if self.ptt_active || self.state == SystemState::Listening {
            (
                "[● ESCUCHANDO...] (Suelta Espacio / Enter para enviar)",
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
                Color::Yellow,
            )
        } else if self.state == SystemState::Thinking {
            (
                "[● PENSANDO...] Procesando respuesta en Antigravity...",
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
                Color::Magenta,
            )
        } else if self.state == SystemState::Speaking {
            (
                "[🔊 HABLANDO...] Presiona Esc para interrumpir (Barge-in)",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
                Color::Cyan,
            )
        } else {
            (
                "[ MANTÉN ESPACIO / ENTER PARA HABLAR ] ─── (Simula Super+V)",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
                Color::DarkGray,
            )
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Control Push-to-Talk (Super+V) ")
            .border_style(Style::default().fg(border_color));

        let paragraph = Paragraph::new(Line::from(vec![Span::styled(label, style)]))
            .block(block)
            .alignment(Alignment::Center);

        paragraph.render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn test_ptt_widget_rendering_idle_vs_recording() {
        let backend = TestBackend::new(80, 5);
        let mut terminal = Terminal::new(backend).unwrap();

        // 1. Idle state
        terminal
            .draw(|f| {
                let widget = PttWidget::new(false, SystemState::Idle, true);
                f.render_widget(widget, f.area());
            })
            .unwrap();
        let buffer_str = format!("{:?}", terminal.backend().buffer());
        assert!(buffer_str.contains("MANTÉN ESPACIO / ENTER PARA HABLAR"));

        // 2. Active recording
        terminal
            .draw(|f| {
                let widget = PttWidget::new(true, SystemState::Listening, true);
                f.render_widget(widget, f.area());
            })
            .unwrap();
        let buffer_rec = format!("{:?}", terminal.backend().buffer());
        assert!(buffer_rec.contains("ESCUCHANDO"));

        // 3. Disconnected daemon
        terminal
            .draw(|f| {
                let widget = PttWidget::new(false, SystemState::Idle, false);
                f.render_widget(widget, f.area());
            })
            .unwrap();
        let buffer_dis = format!("{:?}", terminal.backend().buffer());
        assert!(buffer_dis.contains("DAEMON DESCONECTADO"));
    }
}
