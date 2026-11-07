//! Audio Hardware Explorer and Device Selector View (Tab 4).

use cpal::traits::{DeviceTrait, HostTrait};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, List, ListItem, Paragraph};
use ratatui::Frame;
use viernes_audio::devices::{
    AudioDeviceInfo, CpalDeviceEnumerator, DeviceEnumerator, MockDeviceEnumerator,
};

/// Formats an audio device for descriptive list summary presentation.
#[must_use]
pub fn format_device_summary(device: &AudioDeviceInfo) -> String {
    let default_badge = if device.is_default {
        " [PREDETERMINADO]"
    } else {
        ""
    };
    let ch_label = if device.max_channels == 1 {
        "mono"
    } else {
        "estéreo"
    };
    format!(
        "[{}] {} ({} ch, {}){}",
        device.index, device.name, device.max_channels, ch_label, default_badge
    )
}

/// Active focused section within the Audio Devices tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DeviceFocus {
    /// Microphone / Input devices dropdown list.
    #[default]
    Microphone,
    /// Speaker / Output devices dropdown list.
    Speaker,
    /// Audio level test controls.
    MicTest,
}

/// State and rendering engine for Tab 4: Audio Hardware & Devices.
#[derive(Debug, Clone)]
pub struct DevicesView {
    /// Discovered audio input endpoints (microphones).
    pub input_devices: Vec<AudioDeviceInfo>,
    /// Discovered audio output endpoints (speakers/headphones).
    pub output_devices: Vec<AudioDeviceInfo>,
    /// Selected index for microphone.
    pub selected_input: usize,
    /// Selected index for speaker.
    pub selected_output: usize,
    /// Whether the microphone selection dropdown is currently open.
    pub input_dropdown_open: bool,
    /// Whether the speaker selection dropdown is currently open.
    pub output_dropdown_open: bool,
    /// Active interactive focus.
    pub focus: DeviceFocus,
    /// Current normalized mic energy level (0.0 to 1.0) for VU meter widget.
    pub mic_level: f32,
    /// Indicates whether mic testing stream is active.
    pub is_testing_mic: bool,
    /// Transient status or error message.
    pub status_message: Option<String>,
}

impl Default for DevicesView {
    fn default() -> Self {
        Self::new()
    }
}

impl DevicesView {
    /// Creates a new `DevicesView` and enumerates hardware devices via default host.
    #[must_use]
    pub fn new() -> Self {
        let mut view = Self {
            input_devices: Vec::new(),
            output_devices: Vec::new(),
            selected_input: 0,
            selected_output: 0,
            input_dropdown_open: false,
            output_dropdown_open: false,
            focus: DeviceFocus::Microphone,
            mic_level: 0.0,
            is_testing_mic: false,
            status_message: None,
        };
        view.refresh_with_enumerator(&CpalDeviceEnumerator::new());
        view
    }

    /// Creates a mock `DevicesView` for hermetic testing.
    #[must_use]
    pub fn new_mock(inputs: Vec<AudioDeviceInfo>, outputs: Vec<AudioDeviceInfo>) -> Self {
        let enumerator = MockDeviceEnumerator::new(inputs, outputs);
        let mut view = Self {
            input_devices: Vec::new(),
            output_devices: Vec::new(),
            selected_input: 0,
            selected_output: 0,
            input_dropdown_open: false,
            output_dropdown_open: false,
            focus: DeviceFocus::Microphone,
            mic_level: 0.0,
            is_testing_mic: false,
            status_message: None,
        };
        view.refresh_with_enumerator(&enumerator);
        view
    }

    /// Refreshes device collections using the provided enumerator.
    pub fn refresh_with_enumerator(&mut self, enumerator: &dyn DeviceEnumerator) {
        if let Ok(inputs) = enumerator.list_input_devices() {
            self.selected_input = inputs.iter().position(|d| d.is_default).unwrap_or(0);
            self.input_devices = inputs;
        }

        if let Ok(outputs) = enumerator.list_output_devices() {
            self.selected_output = outputs.iter().position(|d| d.is_default).unwrap_or(0);
            self.output_devices = outputs;
        }

        self.status_message = Some(format!(
            "Dispositivos detectados: {} entradas, {} salidas",
            self.input_devices.len(),
            self.output_devices.len()
        ));
    }

    /// Refreshes devices directly from host cpal devices.
    pub fn refresh_from_host(&mut self) {
        self.refresh_with_enumerator(&CpalDeviceEnumerator::new());
    }

    /// Enumerates raw ALSA/PipeWire devices using `cpal::traits::HostTrait::devices()`.
    pub fn enumerate_raw_host_devices() -> Result<Vec<String>, String> {
        let host = cpal::default_host();
        let devices = host
            .devices()
            .map_err(|e| format!("Failed querying cpal host devices: {e}"))?;

        let mut list = Vec::new();
        for dev in devices {
            if let Ok(name) = dev.name() {
                list.push(name);
            }
        }
        Ok(list)
    }

    /// Cycles focus between Microphone, Speaker, and MicTest.
    pub fn next_focus(&mut self) {
        self.focus = match self.focus {
            DeviceFocus::Microphone => DeviceFocus::Speaker,
            DeviceFocus::Speaker => DeviceFocus::MicTest,
            DeviceFocus::MicTest => DeviceFocus::Microphone,
        };
    }

    /// Cycles focus in reverse.
    pub fn prev_focus(&mut self) {
        self.focus = match self.focus {
            DeviceFocus::Microphone => DeviceFocus::MicTest,
            DeviceFocus::Speaker => DeviceFocus::Microphone,
            DeviceFocus::MicTest => DeviceFocus::Speaker,
        };
    }

    /// Moves selection up in the currently focused dropdown list.
    pub fn select_prev(&mut self) {
        match self.focus {
            DeviceFocus::Microphone => {
                if !self.input_devices.is_empty() {
                    if self.selected_input == 0 {
                        self.selected_input = self.input_devices.len() - 1;
                    } else {
                        self.selected_input -= 1;
                    }
                }
            }
            DeviceFocus::Speaker => {
                if !self.output_devices.is_empty() {
                    if self.selected_output == 0 {
                        self.selected_output = self.output_devices.len() - 1;
                    } else {
                        self.selected_output -= 1;
                    }
                }
            }
            DeviceFocus::MicTest => {}
        }
    }

    /// Moves selection down in the currently focused dropdown list.
    pub fn select_next(&mut self) {
        match self.focus {
            DeviceFocus::Microphone => {
                if !self.input_devices.is_empty() {
                    self.selected_input = (self.selected_input + 1) % self.input_devices.len();
                }
            }
            DeviceFocus::Speaker => {
                if !self.output_devices.is_empty() {
                    self.selected_output = (self.selected_output + 1) % self.output_devices.len();
                }
            }
            DeviceFocus::MicTest => {}
        }
    }

    /// Toggles the dropdown open/close state or triggers test.
    pub fn toggle_action(&mut self) {
        match self.focus {
            DeviceFocus::Microphone => {
                self.input_dropdown_open = !self.input_dropdown_open;
                if self.input_dropdown_open {
                    self.output_dropdown_open = false;
                }
            }
            DeviceFocus::Speaker => {
                self.output_dropdown_open = !self.output_dropdown_open;
                if self.output_dropdown_open {
                    self.input_dropdown_open = false;
                }
            }
            DeviceFocus::MicTest => {
                self.is_testing_mic = !self.is_testing_mic;
                if !self.is_testing_mic {
                    self.mic_level = 0.0;
                }
            }
        }
    }

    /// Confirms current selection and closes open dropdowns.
    pub fn confirm_selection(&mut self) {
        self.input_dropdown_open = false;
        self.output_dropdown_open = false;
        if let Some(dev) = self.input_devices.get(self.selected_input) {
            self.status_message = Some(format!("Micrófono seleccionado: {}", dev.name));
        }
    }

    /// Sets the mic level for the VU meter (clamped to 0.0 ..= 1.0).
    pub fn set_mic_level(&mut self, level: f32) {
        self.mic_level = level.clamp(0.0, 1.0);
    }

    /// Returns the currently selected input device.
    #[must_use]
    pub fn selected_input_device(&self) -> Option<&AudioDeviceInfo> {
        self.input_devices.get(self.selected_input)
    }

    /// Returns the currently selected output device.
    #[must_use]
    pub fn selected_output_device(&self) -> Option<&AudioDeviceInfo> {
        self.output_devices.get(self.selected_output)
    }

    /// Renders Tab 4 view inside the specified frame and area.
    pub fn render(&self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(3), // Title / Subtitle
                Constraint::Length(7), // Dropdown selectors
                Constraint::Length(6), // VU meter audio level widget
                Constraint::Min(3),    // Device details or dropdown expand
                Constraint::Length(2), // Help / shortcuts
            ])
            .split(area);

        // Header block
        let header = Paragraph::new(vec![
            Line::from(vec![Span::styled(
                " [4] HARDWARE DE AUDIO (ALSA / PipeWire / PulseAudio) ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from(vec![
                Span::raw("Estado: "),
                Span::styled(
                    self.status_message.as_deref().unwrap_or("Hardware listo"),
                    Style::default().fg(Color::DarkGray),
                ),
            ]),
        ])
        .alignment(Alignment::Left);
        f.render_widget(header, chunks[0]);

        // Selectors row
        let selector_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(chunks[1]);

        // 1. Microphone Dropdown Box
        let mic_name = self
            .selected_input_device()
            .map(|d| d.name.as_str())
            .unwrap_or("Ningún dispositivo encontrado");
        let mic_style = if self.focus == DeviceFocus::Microphone {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        let mic_border_style = if self.focus == DeviceFocus::Microphone {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let mic_indicator = if self.input_dropdown_open {
            "▲"
        } else {
            "▼"
        };
        let mic_box = Paragraph::new(vec![
            Line::from(Span::styled(
                "Entrada Predeterminada:",
                Style::default().fg(Color::Gray),
            )),
            Line::from(vec![
                Span::styled(
                    format!("[{mic_indicator}] "),
                    Style::default().fg(Color::Cyan),
                ),
                Span::styled(mic_name, mic_style),
            ]),
            Line::from(Span::styled(
                format!("Total entradas: {}", self.input_devices.len()),
                Style::default().fg(Color::DarkGray),
            )),
        ])
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Micrófono (Captura VAD / STT) ")
                .border_style(mic_border_style),
        );
        f.render_widget(mic_box, selector_chunks[0]);

        // 2. Speaker Dropdown Box
        let spk_name = self
            .selected_output_device()
            .map(|d| d.name.as_str())
            .unwrap_or("Ningún dispositivo encontrado");
        let spk_style = if self.focus == DeviceFocus::Speaker {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        let spk_border_style = if self.focus == DeviceFocus::Speaker {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let spk_indicator = if self.output_dropdown_open {
            "▲"
        } else {
            "▼"
        };
        let spk_box = Paragraph::new(vec![
            Line::from(Span::styled(
                "Salida Predeterminada:",
                Style::default().fg(Color::Gray),
            )),
            Line::from(vec![
                Span::styled(
                    format!("[{spk_indicator}] "),
                    Style::default().fg(Color::Cyan),
                ),
                Span::styled(spk_name, spk_style),
            ]),
            Line::from(Span::styled(
                format!("Total salidas: {}", self.output_devices.len()),
                Style::default().fg(Color::DarkGray),
            )),
        ])
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Altavoces (Salida TTS) ")
                .border_style(spk_border_style),
        );
        f.render_widget(spk_box, selector_chunks[1]);

        // 3. Audio Level Meter Widget (VU Meter)
        let vu_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Length(2)])
            .split(chunks[2]);

        let pct = (self.mic_level * 100.0) as u16;
        let gauge_color = if pct > 80 {
            Color::Red
        } else if pct > 50 {
            Color::Yellow
        } else {
            Color::Green
        };

        let vu_label = format!("{}% (Energía RMS)", pct);
        let vu_gauge = Gauge::default()
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Vúmetro de Entrada (Nivel de Micrófono en Tiempo Real) ")
                    .border_style(if self.focus == DeviceFocus::MicTest {
                        Style::default().fg(Color::Yellow)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    }),
            )
            .gauge_style(Style::default().fg(gauge_color).bg(Color::Black))
            .percent(pct)
            .label(vu_label);
        f.render_widget(vu_gauge, vu_chunks[0]);

        let test_status_str = if self.is_testing_mic {
            "● PRUEBA ACTIVA — Hable al micrófono para verificar energía"
        } else {
            "○ PRUEBA DETENIDA — Presione [Espacio] o [m] para activar monitoreo de entrada"
        };
        let test_status_color = if self.is_testing_mic {
            Color::Green
        } else {
            Color::DarkGray
        };
        let test_status_p = Paragraph::new(Line::from(vec![Span::styled(
            test_status_str,
            Style::default()
                .fg(test_status_color)
                .add_modifier(Modifier::BOLD),
        )]));
        f.render_widget(test_status_p, vu_chunks[1]);

        // 4. Detail / Expanded Dropdown section
        if self.input_dropdown_open {
            let items: Vec<ListItem> = self
                .input_devices
                .iter()
                .enumerate()
                .map(|(i, d)| {
                    let prefix = if i == self.selected_input {
                        " > "
                    } else {
                        "   "
                    };
                    let style = if i == self.selected_input {
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::White)
                    };
                    ListItem::new(format!("{prefix}{d}")).style(style)
                })
                .collect();

            let list = List::new(items).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Seleccionar Micrófono [↑/↓ para navegar, Enter para confirmar] ")
                    .border_style(Style::default().fg(Color::Yellow)),
            );
            f.render_widget(list, chunks[3]);
        } else if self.output_dropdown_open {
            let items: Vec<ListItem> = self
                .output_devices
                .iter()
                .enumerate()
                .map(|(i, d)| {
                    let prefix = if i == self.selected_output {
                        " > "
                    } else {
                        "   "
                    };
                    let style = if i == self.selected_output {
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::White)
                    };
                    ListItem::new(format!("{prefix}{d}")).style(style)
                })
                .collect();

            let list = List::new(items).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Seleccionar Altavoces [↑/↓ para navegar, Enter para confirmar] ")
                    .border_style(Style::default().fg(Color::Yellow)),
            );
            f.render_widget(list, chunks[3]);
        } else {
            // General device specs display
            let mut info_lines = Vec::new();
            if let Some(inp) = self.selected_input_device() {
                info_lines.push(Line::from(vec![
                    Span::styled("Entrada: ", Style::default().fg(Color::Cyan)),
                    Span::raw(format!(
                        "Canales: {} | Frecuencias: {:?}",
                        inp.max_channels, inp.sample_rates
                    )),
                ]));
            }
            if let Some(out) = self.selected_output_device() {
                info_lines.push(Line::from(vec![
                    Span::styled("Salida:  ", Style::default().fg(Color::Cyan)),
                    Span::raw(format!(
                        "Canales: {} | Frecuencias: {:?}",
                        out.max_channels, out.sample_rates
                    )),
                ]));
            }

            let details = Paragraph::new(info_lines).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Propiedades del Dispositivo Activo ")
                    .border_style(Style::default().fg(Color::DarkGray)),
            );
            f.render_widget(details, chunks[3]);
        }

        // 5. Shortcuts help footer
        let help = Paragraph::new(Line::from(vec![
            Span::styled("[Tab]: ", Style::default().fg(Color::Yellow)),
            Span::raw("Cambiar sección │ "),
            Span::styled("[↑/↓]: ", Style::default().fg(Color::Yellow)),
            Span::raw("Navegar │ "),
            Span::styled("[Enter]: ", Style::default().fg(Color::Yellow)),
            Span::raw("Desplegar/Confirmar │ "),
            Span::styled("[m]: ", Style::default().fg(Color::Yellow)),
            Span::raw("Probar Micrófono │ "),
            Span::styled("[r]: ", Style::default().fg(Color::Yellow)),
            Span::raw("Re-enumerar"),
        ]))
        .alignment(Alignment::Center);
        f.render_widget(help, chunks[4]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn make_test_devices() -> (Vec<AudioDeviceInfo>, Vec<AudioDeviceInfo>) {
        let inputs = vec![
            AudioDeviceInfo {
                index: 0,
                name: "PipeWire Capture (Default)".into(),
                is_default: true,
                max_channels: 2,
                sample_rates: vec![44100, 48000],
            },
            AudioDeviceInfo {
                index: 1,
                name: "USB Mic Studio".into(),
                is_default: false,
                max_channels: 1,
                sample_rates: vec![16000, 48000],
            },
        ];

        let outputs = vec![
            AudioDeviceInfo {
                index: 0,
                name: "PipeWire Playback (Default)".into(),
                is_default: true,
                max_channels: 2,
                sample_rates: vec![48000],
            },
            AudioDeviceInfo {
                index: 1,
                name: "Headphones Analog".into(),
                is_default: false,
                max_channels: 2,
                sample_rates: vec![44100, 48000],
            },
        ];

        (inputs, outputs)
    }

    #[test]
    fn test_device_enumeration_formatting() {
        let (inputs, _outputs) = make_test_devices();
        let formatted_default = format_device_summary(&inputs[0]);
        assert_eq!(
            formatted_default,
            "[0] PipeWire Capture (Default) (2 ch, estéreo) [PREDETERMINADO]"
        );

        let formatted_mono = format_device_summary(&inputs[1]);
        assert_eq!(formatted_mono, "[1] USB Mic Studio (1 ch, mono)");
    }

    #[test]
    fn test_devices_view_initialization_with_mock() {
        let (inputs, outputs) = make_test_devices();
        let view = DevicesView::new_mock(inputs, outputs);

        assert_eq!(view.input_devices.len(), 2);
        assert_eq!(view.output_devices.len(), 2);
        assert_eq!(view.selected_input, 0); // PipeWire is default
        assert_eq!(view.selected_output, 0);
        assert_eq!(
            view.selected_input_device().unwrap().name,
            "PipeWire Capture (Default)"
        );
    }

    #[test]
    fn test_devices_view_navigation_and_focus() {
        let (inputs, outputs) = make_test_devices();
        let mut view = DevicesView::new_mock(inputs, outputs);

        assert_eq!(view.focus, DeviceFocus::Microphone);
        view.next_focus();
        assert_eq!(view.focus, DeviceFocus::Speaker);
        view.next_focus();
        assert_eq!(view.focus, DeviceFocus::MicTest);
        view.next_focus();
        assert_eq!(view.focus, DeviceFocus::Microphone);

        // Select next device in microphone list
        view.select_next();
        assert_eq!(view.selected_input, 1);
        assert_eq!(view.selected_input_device().unwrap().name, "USB Mic Studio");

        view.select_prev();
        assert_eq!(view.selected_input, 0);
    }

    #[test]
    fn test_devices_view_dropdown_toggle_and_confirm() {
        let (inputs, outputs) = make_test_devices();
        let mut view = DevicesView::new_mock(inputs, outputs);

        assert!(!view.input_dropdown_open);
        view.toggle_action();
        assert!(view.input_dropdown_open);

        view.confirm_selection();
        assert!(!view.input_dropdown_open);
        assert!(view.status_message.as_ref().unwrap().contains("PipeWire"));
    }

    #[test]
    fn test_devices_view_mic_meter_clamping() {
        let (inputs, outputs) = make_test_devices();
        let mut view = DevicesView::new_mock(inputs, outputs);

        view.set_mic_level(1.5);
        assert!((view.mic_level - 1.0).abs() < f32::EPSILON);

        view.set_mic_level(-0.2);
        assert!((view.mic_level - 0.0).abs() < f32::EPSILON);

        view.set_mic_level(0.42);
        assert!((view.mic_level - 0.42).abs() < 1e-4);
    }

    #[test]
    fn test_devices_view_render_headless() {
        let (inputs, outputs) = make_test_devices();
        let view = DevicesView::new_mock(inputs, outputs);

        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                view.render(f, f.area());
            })
            .unwrap();
    }
}
