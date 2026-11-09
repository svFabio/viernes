//! Real-time soundwave and frequency spectrum visualizer widget.
//!
//! Renders vertical frequency bars utilizing Unicode block elements ( ▂▃▄▅▆▇█)
//! reacting to assistant speech at 30 FPS.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Widget};
use viernes_core::state::SystemState;

/// Unicode vertical sub-block characters for progressive height resolution.
pub const BLOCK_CHARS: [char; 8] = [' ', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// Widget displaying an animated audio frequency spectrum.
pub struct SpectrumWidget<'a> {
    amplitudes: &'a [f32],
    state: SystemState,
}

impl<'a> SpectrumWidget<'a> {
    /// Creates a new `SpectrumWidget` with the given frequency amplitudes and current system state.
    #[must_use]
    pub fn new(amplitudes: &'a [f32], state: SystemState) -> Self {
        Self { amplitudes, state }
    }
}

impl<'a> Widget for SpectrumWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let title = match self.state {
            SystemState::Speaking => " Espectro de Voz (Reproduciendo audio) ",
            SystemState::Listening => " Espectro de Voz (Capturando audio) ",
            SystemState::Thinking => " Espectro de Voz (Procesando) ",
            SystemState::Idle => " Espectro de Voz (Reposo) ",
        };

        let border_color = match self.state {
            SystemState::Speaking => Color::Cyan,
            SystemState::Listening => Color::Yellow,
            SystemState::Thinking => Color::Magenta,
            SystemState::Idle => Color::DarkGray,
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(Style::default().fg(border_color));

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.width == 0 || inner.height == 0 || self.amplitudes.is_empty() {
            return;
        }

        let has_axis = inner.height >= 3;
        let bar_height = if has_axis {
            inner.height - 1
        } else {
            inner.height
        };
        let base_y = if has_axis {
            inner.bottom() - 2
        } else {
            inner.bottom() - 1
        };

        let bar_width: u16 = if inner.width > 100 { 2 } else { 1 };
        let bar_gap: u16 = 1;
        let step = bar_width + bar_gap;
        let num_bars = ((inner.width.saturating_sub(1)) / step).max(1) as usize;
        let total_drawn_width = (num_bars as u16 * step).saturating_sub(bar_gap);
        let x_offset = inner.x + ((inner.width.saturating_sub(total_drawn_width)) / 2);
        let max_height = bar_height as f32;

        for bar_idx in 0..num_bars {
            let amp = if !self.amplitudes.is_empty() {
                let norm = bar_idx as f32 / num_bars as f32;
                let src_idx = (norm * (self.amplitudes.len() - 1) as f32).round() as usize;
                self.amplitudes[src_idx.min(self.amplitudes.len() - 1)]
            } else {
                0.05
            };

            let col_x_start = x_offset + (bar_idx as u16 * step);
            let bar_height_units = amp.clamp(0.0, 1.0) * max_height;
            let full_cells = bar_height_units.floor() as u16;
            let remainder = bar_height_units - full_cells as f32;

            for w in 0..bar_width {
                let col_x = col_x_start + w;
                if col_x >= inner.right() {
                    continue;
                }

                // Render from bottom up
                for cell_y in 0..bar_height {
                    let y = base_y.saturating_sub(cell_y);

                    if let Some(cell) = buf.cell_mut((col_x, y)) {
                        if cell_y < full_cells {
                            // Full block
                            let style = bar_style(cell_y, bar_height, self.state);
                            cell.set_char('█').set_style(style);
                        } else if cell_y == full_cells && remainder > 0.05 {
                            // Fractional sub-block
                            let block_idx = ((remainder * 8.0).floor() as usize).min(7);
                            let char_to_draw = BLOCK_CHARS[block_idx];
                            let style = bar_style(cell_y, bar_height, self.state);
                            cell.set_char(char_to_draw).set_style(style);
                        } else {
                            // Empty space above bar
                            cell.set_char(' ');
                        }
                    }
                }
            }
        }

        // Render frequency markers at the bottom line if sufficient height exists
        if has_axis {
            let axis_y = inner.bottom() - 1;
            let axis_style = Style::default().fg(Color::DarkGray);

            let labels: &[(&str, f32)] = &[
                ("20Hz", 0.05),
                ("1kHz", 0.35),
                ("8kHz", 0.68),
                ("20kHz", 0.92),
            ];

            for &(label, pos_norm) in labels {
                let label_len = label.len() as u16;
                let target_x = inner.x + (inner.width as f32 * pos_norm) as u16;
                let clamped_x = target_x.min(inner.right().saturating_sub(label_len));

                for (idx, ch) in label.chars().enumerate() {
                    let cx = clamped_x + idx as u16;
                    if cx < inner.right() {
                        if let Some(cell) = buf.cell_mut((cx, axis_y)) {
                            cell.set_char(ch).set_style(axis_style);
                        }
                    }
                }
            }
        }
    }
}

/// Computes gradient color style for frequency bar cells based on height and state.
fn bar_style(row: u16, max_rows: u16, state: SystemState) -> Style {
    let ratio = if max_rows > 1 {
        (row as f32) / (max_rows as f32 - 1.0)
    } else {
        0.0
    };

    match state {
        SystemState::Speaking => {
            let color = if ratio < 0.4 {
                Color::Cyan
            } else if ratio < 0.75 {
                Color::Rgb(100, 149, 237) // Cornflower blue
            } else {
                Color::Magenta
            };
            Style::default().fg(color).add_modifier(Modifier::BOLD)
        }
        SystemState::Listening => {
            let color = if ratio < 0.5 {
                Color::Green
            } else {
                Color::Yellow
            };
            Style::default().fg(color)
        }
        SystemState::Thinking => Style::default().fg(Color::Magenta),
        SystemState::Idle => Style::default().fg(Color::DarkGray),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn test_spectrum_widget_rendering_idle_vs_speaking() {
        let backend = TestBackend::new(80, 10);
        let mut terminal = Terminal::new(backend).unwrap();

        // 1. Render in Idle state
        let idle_amps = vec![0.05; 32];
        terminal
            .draw(|f| {
                let widget = SpectrumWidget::new(&idle_amps, SystemState::Idle);
                f.render_widget(widget, f.area());
            })
            .unwrap();

        let buffer_str = format!("{:?}", terminal.backend().buffer());
        assert!(buffer_str.contains("Espectro de Voz (Reposo)"));
        assert!(buffer_str.contains("20Hz"));
        assert!(buffer_str.contains("1kHz"));

        // 2. Render in Speaking state with high amplitudes
        let speaking_amps = vec![0.85; 32];
        terminal
            .draw(|f| {
                let widget = SpectrumWidget::new(&speaking_amps, SystemState::Speaking);
                f.render_widget(widget, f.area());
            })
            .unwrap();

        let buffer_speaking = format!("{:?}", terminal.backend().buffer());
        assert!(buffer_speaking.contains("Espectro de Voz (Reproduciendo audio)"));
        assert!(buffer_speaking.contains('█'));
    }
}
