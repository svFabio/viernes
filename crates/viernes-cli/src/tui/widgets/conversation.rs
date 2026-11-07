//! Scrollable live conversation turn and transcription history widget.

use chrono::Local;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Widget};

/// Identifies the speaker of a conversation turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Speaker {
    /// Human operator / user.
    User,
    /// Viernes AI assistant.
    Viernes,
    /// System / diagnostic event.
    System,
}

/// Discrete conversational interaction turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationTurn {
    /// Speaker entity.
    pub speaker: Speaker,
    /// Spoken or synthesized textual content.
    pub text: String,
    /// Formatted wall-clock timestamp (HH:MM:SS).
    pub timestamp: String,
}

impl ConversationTurn {
    /// Creates a new conversation turn with the current timestamp.
    #[must_use]
    pub fn new(speaker: Speaker, text: impl Into<String>) -> Self {
        let timestamp = Local::now().format("%H:%M:%S").to_string();
        Self {
            speaker,
            text: text.into(),
            timestamp,
        }
    }
}

/// Widget rendering historical conversation turns with automatic scrolling.
pub struct ConversationWidget<'a> {
    turns: &'a [ConversationTurn],
}

impl<'a> ConversationWidget<'a> {
    /// Creates a new conversation turn viewer.
    #[must_use]
    pub fn new(turns: &'a [ConversationTurn]) -> Self {
        Self { turns }
    }
}

impl<'a> Widget for ConversationWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Transcripción & Historial Conversacional ")
            .border_style(Style::default().fg(Color::Blue));

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.width == 0 || inner.height == 0 || self.turns.is_empty() {
            return;
        }

        let mut items = Vec::new();

        for turn in self.turns {
            let (prefix, prefix_style) = match turn.speaker {
                Speaker::User => (
                    "[Usuario]",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Speaker::Viernes => (
                    "[Viernes]",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Speaker::System => (
                    "[Sistema]",
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::ITALIC),
                ),
            };

            let timestamp_span = Span::styled(
                format!(" {} ", turn.timestamp),
                Style::default().fg(Color::DarkGray),
            );

            let content_span = Span::styled(
                format!(": {}", turn.text),
                Style::default().fg(Color::White),
            );

            let line = Line::from(vec![
                Span::styled(prefix, prefix_style),
                timestamp_span,
                content_span,
            ]);

            items.push(ListItem::new(line));
        }

        // Auto-scroll calculation: keep the bottom turns visible
        let max_visible = inner.height as usize;
        let start_idx = if items.len() > max_visible {
            items.len() - max_visible
        } else {
            0
        };

        let visible_items = items[start_idx..].to_vec();
        let list = List::new(visible_items);
        list.render(inner, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn test_conversation_widget_rendering() {
        let backend = TestBackend::new(80, 10);
        let mut terminal = Terminal::new(backend).unwrap();

        let turns = vec![
            ConversationTurn::new(Speaker::User, "Hola Viernes"),
            ConversationTurn::new(Speaker::Viernes, "Hola señor, ¿en qué puedo ayudarle?"),
        ];

        terminal
            .draw(|f| {
                let widget = ConversationWidget::new(&turns);
                f.render_widget(widget, f.area());
            })
            .unwrap();

        let buffer_str = format!("{:?}", terminal.backend().buffer());
        assert!(buffer_str.contains("Transcripción & Historial Conversacional"));
        assert!(buffer_str.contains("[Usuario]"));
        assert!(buffer_str.contains("[Viernes]"));
        assert!(buffer_str.contains("Hola Viernes"));
    }
}
