//! Live Reactive Log Streaming Widget with Circular Buffer and Severity Highlights (ST-RUST-33.3).

use std::collections::VecDeque;

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

/// Maximum number of log lines preserved in the in-memory circular buffer (Criterion 7).
pub const MAX_LOG_BUFFER_LINES: usize = 500;

/// Severity classification for log lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LogSeverity {
    /// Informational trace.
    #[default]
    Info,
    /// Warning or non-fatal condition.
    Warn,
    /// Error, failure, or exception.
    Error,
    /// Debug or fine-grained diagnostic.
    Debug,
    /// Unclassified or plain log line.
    Other,
}

impl LogSeverity {
    /// Detects severity from a raw log string.
    #[must_use]
    pub fn parse(line: &str) -> Self {
        let upper = line.to_uppercase();
        if upper.contains("ERROR") || upper.contains("FATAL") || upper.contains("PANIC") {
            Self::Error
        } else if upper.contains("WARN") {
            Self::Warn
        } else if upper.contains("INFO") {
            Self::Info
        } else if upper.contains("DEBUG") || upper.contains("TRACE") {
            Self::Debug
        } else {
            Self::Other
        }
    }

    /// Color representing the severity level.
    #[must_use]
    pub fn color(self) -> Color {
        match self {
            Self::Info => Color::Green,
            Self::Warn => Color::Yellow,
            Self::Error => Color::Red,
            Self::Debug => Color::Cyan,
            Self::Other => Color::White,
        }
    }

    /// Short label tag for header display.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
            Self::Debug => "DEBUG",
            Self::Other => "LOG",
        }
    }
}

/// A parsed structured entry within the log stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogEntry {
    /// Raw unparsed log text.
    pub raw: String,
    /// Detected severity.
    pub severity: LogSeverity,
    /// Extracted or synthesized timestamp.
    pub timestamp: String,
    /// Stripped message payload.
    pub message: String,
}

impl LogEntry {
    /// Creates a `LogEntry` parsing the given line.
    #[must_use]
    pub fn new(line: impl Into<String>) -> Self {
        let raw = line.into();
        let severity = LogSeverity::parse(&raw);

        // Simple heuristic extraction of timestamp if present
        let (timestamp, message) =
            if raw.len() >= 8 && raw.chars().nth(2) == Some(':') && raw.chars().nth(5) == Some(':')
            {
                let ts = raw[..8].to_string();
                let msg = raw[8..].trim_start().to_string();
                (ts, msg)
            } else {
                ("".to_string(), raw.clone())
            };

        Self {
            raw,
            severity,
            timestamp,
            message,
        }
    }
}

/// Reactive live log view widget managing a bounded circular buffer.
#[derive(Debug, Clone)]
pub struct LogsView {
    /// Bounded circular buffer holding up to `MAX_LOG_BUFFER_LINES`.
    pub buffer: VecDeque<LogEntry>,
    /// Vertical scroll offset.
    pub scroll_offset: u16,
    /// Whether auto-scroll is actively pinning to the newest log entries.
    pub auto_scroll: bool,
    /// Filter for minimum visible severity level.
    pub min_severity: Option<LogSeverity>,
}

impl Default for LogsView {
    fn default() -> Self {
        Self::new()
    }
}

impl LogsView {
    /// Constructs an empty `LogsView` with auto-scroll enabled.
    #[must_use]
    pub fn new() -> Self {
        Self {
            buffer: VecDeque::with_capacity(MAX_LOG_BUFFER_LINES),
            scroll_offset: 0,
            auto_scroll: true,
            min_severity: None,
        }
    }

    /// Appends a new log line, evicting the oldest entries once capacity (500) is reached.
    pub fn push_line(&mut self, line: impl Into<String>) {
        if self.buffer.len() >= MAX_LOG_BUFFER_LINES {
            self.buffer.pop_front();
        }
        self.buffer.push_back(LogEntry::new(line));
        if self.auto_scroll {
            self.scroll_to_bottom();
        }
    }

    /// Appends multiple lines in sequence.
    pub fn push_lines<I, S>(&mut self, lines: I)
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        for line in lines {
            self.push_line(line);
        }
    }

    /// Total number of stored log entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Indicates whether the buffer contains zero lines.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Clears the circular log buffer.
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.scroll_offset = 0;
    }

    /// Scrolls view down by one step.
    pub fn scroll_down(&mut self) {
        let max_scroll = (self.buffer.len() as u16).saturating_sub(1);
        if self.scroll_offset < max_scroll {
            self.scroll_offset += 1;
        } else {
            self.auto_scroll = true;
        }
    }

    /// Scrolls view up by one step (disables auto-scroll).
    pub fn scroll_up(&mut self) {
        self.auto_scroll = false;
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    /// Scrolls view down by a page.
    pub fn page_down(&mut self, page_size: u16) {
        let max_scroll = (self.buffer.len() as u16).saturating_sub(1);
        self.scroll_offset = (self.scroll_offset + page_size).min(max_scroll);
        if self.scroll_offset >= max_scroll {
            self.auto_scroll = true;
        }
    }

    /// Scrolls view up by a page.
    pub fn page_up(&mut self, page_size: u16) {
        self.auto_scroll = false;
        self.scroll_offset = self.scroll_offset.saturating_sub(page_size);
    }

    /// Resets scroll to pin to the latest incoming lines.
    pub fn scroll_to_bottom(&mut self) {
        let len = self.buffer.len() as u16;
        self.scroll_offset = len.saturating_sub(1);
        self.auto_scroll = true;
    }

    /// Toggles the auto-scroll mode.
    pub fn toggle_auto_scroll(&mut self) {
        self.auto_scroll = !self.auto_scroll;
        if self.auto_scroll {
            self.scroll_to_bottom();
        }
    }

    /// Cycles the active severity filter.
    pub fn cycle_filter(&mut self) {
        self.min_severity = match self.min_severity {
            None => Some(LogSeverity::Info),
            Some(LogSeverity::Info) => Some(LogSeverity::Warn),
            Some(LogSeverity::Warn) => Some(LogSeverity::Error),
            Some(LogSeverity::Error) => None,
            _ => None,
        };
    }

    /// Renders the log streaming widget in the given frame and area.
    pub fn render(&self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(4), Constraint::Length(1)])
            .split(area);

        // Filter and format lines
        let visible_entries: Vec<&LogEntry> = self
            .buffer
            .iter()
            .filter(|e| match self.min_severity {
                None => true,
                Some(LogSeverity::Info) => matches!(
                    e.severity,
                    LogSeverity::Info | LogSeverity::Warn | LogSeverity::Error
                ),
                Some(LogSeverity::Warn) => {
                    matches!(e.severity, LogSeverity::Warn | LogSeverity::Error)
                }
                Some(LogSeverity::Error) => matches!(e.severity, LogSeverity::Error),
                _ => true,
            })
            .collect();

        let mut lines = Vec::with_capacity(visible_entries.len());
        for entry in &visible_entries {
            let mut spans = Vec::new();

            if !entry.timestamp.is_empty() {
                spans.push(Span::styled(
                    format!("{} ", entry.timestamp),
                    Style::default().fg(Color::DarkGray),
                ));
            }

            let badge = format!("[{:5}] ", entry.severity.label());
            let badge_style = match entry.severity {
                LogSeverity::Error => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                LogSeverity::Warn => Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
                LogSeverity::Info => Style::default().fg(Color::Green),
                LogSeverity::Debug => Style::default().fg(Color::Cyan),
                LogSeverity::Other => Style::default().fg(Color::Gray),
            };
            spans.push(Span::styled(badge, badge_style));

            spans.push(Span::styled(
                &entry.message,
                Style::default().fg(Color::White),
            ));

            lines.push(Line::from(spans));
        }

        if lines.is_empty() {
            lines.push(Line::from(Span::styled(
                " (No hay registros disponibles en el buffer) ",
                Style::default().fg(Color::DarkGray),
            )));
        }

        let filter_str = match self.min_severity {
            None => "TODOS",
            Some(LogSeverity::Info) => "INFO+",
            Some(LogSeverity::Warn) => "WARN+",
            Some(LogSeverity::Error) => "SOLO ERROR",
            _ => "CUSTOM",
        };

        let auto_str = if self.auto_scroll {
            "ACTIVO"
        } else {
            "PAUSADO"
        };

        let title = format!(
            " Logs en Vivo ({} / 500) │ Filtro: [{}] │ Auto-Scroll: [{}] ",
            self.buffer.len(),
            filter_str,
            auto_str
        );

        let paragraph = Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(title)
                    .border_style(Style::default().fg(Color::DarkGray)),
            )
            .wrap(Wrap { trim: false })
            .scroll((self.scroll_offset, 0));

        f.render_widget(paragraph, chunks[0]);

        // Controls footer
        let footer = Paragraph::new(Line::from(vec![
            Span::styled("[f]: ", Style::default().fg(Color::Yellow)),
            Span::raw("Filtrar severidad │ "),
            Span::styled("[a]: ", Style::default().fg(Color::Yellow)),
            Span::raw("Auto-scroll toggle │ "),
            Span::styled("[↑/↓]: ", Style::default().fg(Color::Yellow)),
            Span::raw("Línea │ "),
            Span::styled("[PgUp/PgDn]: ", Style::default().fg(Color::Yellow)),
            Span::raw("Página │ "),
            Span::styled("[c]: ", Style::default().fg(Color::Yellow)),
            Span::raw("Limpiar buffer"),
        ]))
        .alignment(Alignment::Center);

        f.render_widget(footer, chunks[1]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn test_severity_parsing() {
        assert_eq!(
            LogSeverity::parse("16:28:19 [INFO] Capture stream started"),
            LogSeverity::Info
        );
        assert_eq!(
            LogSeverity::parse("16:28:22 WARN VAD energy degraded"),
            LogSeverity::Warn
        );
        assert_eq!(
            LogSeverity::parse("16:28:25 ERROR Failed to reach ElevenLabs API"),
            LogSeverity::Error
        );
        assert_eq!(
            LogSeverity::parse("16:28:28 Just a generic message"),
            LogSeverity::Other
        );
    }

    #[test]
    fn test_circular_buffer_capacity_cap_at_500() {
        let mut logs = LogsView::new();
        assert_eq!(logs.len(), 0);

        for i in 0..600 {
            logs.push_line(format!("12:00:00 [INFO] Event record #{i}"));
        }

        assert_eq!(logs.len(), MAX_LOG_BUFFER_LINES);
        assert_eq!(logs.len(), 500);

        // Oldest preserved should be #100, newest #599
        assert!(logs.buffer.front().unwrap().raw.contains("record #100"));
        assert!(logs.buffer.back().unwrap().raw.contains("record #599"));
    }

    #[test]
    fn test_scroll_and_auto_scroll() {
        let mut logs = LogsView::new();
        for i in 0..20 {
            logs.push_line(format!("Line #{i}"));
        }

        assert!(logs.auto_scroll);
        logs.scroll_up();
        assert!(!logs.auto_scroll);
        assert_eq!(logs.scroll_offset, 18);

        logs.scroll_to_bottom();
        assert!(logs.auto_scroll);
        assert_eq!(logs.scroll_offset, 19);
    }

    #[test]
    fn test_filter_cycling() {
        let mut logs = LogsView::new();
        assert_eq!(logs.min_severity, None);
        logs.cycle_filter();
        assert_eq!(logs.min_severity, Some(LogSeverity::Info));
        logs.cycle_filter();
        assert_eq!(logs.min_severity, Some(LogSeverity::Warn));
        logs.cycle_filter();
        assert_eq!(logs.min_severity, Some(LogSeverity::Error));
        logs.cycle_filter();
        assert_eq!(logs.min_severity, None);
    }

    #[test]
    fn test_logs_render_headless() {
        let mut logs = LogsView::new();
        logs.push_line("12:00:01 [INFO] Started assistant service");
        logs.push_line("12:00:02 [WARN] High memory buffer usage");
        logs.push_line("12:00:03 [ERROR] Subsystem crash recovered");

        let backend = TestBackend::new(100, 25);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                logs.render(f, f.area());
            })
            .unwrap();
    }
}
