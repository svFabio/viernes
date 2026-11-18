//! Audio Hardware Explorer and Device Selector View (Tab 4).

use std::path::PathBuf;

use cpal::traits::{DeviceTrait, HostTrait};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, List, ListItem, Paragraph};
use ratatui::Frame;
use viernes_audio::devices::{
    AudioDeviceInfo, CpalDeviceEnumerator, DeviceEnumerator, MockDeviceEnumerator,
};
use viernes_core::config::ViernesConfig;

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
    /// Background media audio ducking attenuation controls.
    AudioDucking,
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
    /// Whether background audio ducking is enabled.
    pub ducking_enabled: bool,
    /// Target ducking volume percentage (0..=100).
    pub ducking_volume_percent: u32,
    /// Whether ducking settings have unsaved modifications.
    pub has_unsaved_ducking_changes: bool,
    /// Target path to environment variables file (.env).
    pub env_path: PathBuf,
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
        Self::with_config(None)
    }

    /// Creates a new `DevicesView` populated from optional [`ViernesConfig`].
    #[must_use]
    pub fn with_config(config: Option<&ViernesConfig>) -> Self {
        let env_path = std::env::var("HOME")
            .map(|h| PathBuf::from(h).join(".config/viernes/.env"))
            .unwrap_or_else(|_| PathBuf::from(".env"));

        let (ducking_enabled, ducking_volume_percent) = match config {
            Some(c) => (c.audio_ducking_enabled, c.audio_ducking_volume_percent),
            None => {
                let enabled = std::env::var("AUDIO_DUCKING_ENABLED")
                    .map(|v| v.to_lowercase() == "true" || v == "1")
                    .unwrap_or(true);
                let pct = std::env::var("AUDIO_DUCKING_VOLUME_PERCENT")
                    .ok()
                    .and_then(|v| v.parse::<u32>().ok())
                    .unwrap_or(20)
                    .min(100);
                (enabled, pct)
            }
        };

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
            ducking_enabled,
            ducking_volume_percent,
            has_unsaved_ducking_changes: false,
            env_path,
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
            ducking_enabled: true,
            ducking_volume_percent: 20,
            has_unsaved_ducking_changes: false,
            env_path: PathBuf::from("/tmp/test_viernes_mock.env"),
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

    /// Cycles focus between Microphone, Speaker, AudioDucking, and MicTest.
    pub fn next_focus(&mut self) {
        self.focus = match self.focus {
            DeviceFocus::Microphone => DeviceFocus::Speaker,
            DeviceFocus::Speaker => DeviceFocus::AudioDucking,
            DeviceFocus::AudioDucking => DeviceFocus::MicTest,
            DeviceFocus::MicTest => DeviceFocus::Microphone,
        };
    }

    /// Cycles focus in reverse.
    pub fn prev_focus(&mut self) {
        self.focus = match self.focus {
            DeviceFocus::Microphone => DeviceFocus::MicTest,
            DeviceFocus::Speaker => DeviceFocus::Microphone,
            DeviceFocus::AudioDucking => DeviceFocus::Speaker,
            DeviceFocus::MicTest => DeviceFocus::AudioDucking,
        };
    }

    /// Moves selection up in the currently focused dropdown list or decreases ducking volume.
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
            DeviceFocus::AudioDucking => {
                self.ducking_volume_percent = self.ducking_volume_percent.saturating_sub(5);
                self.has_unsaved_ducking_changes = true;
                self.status_message = Some(format!(
                    "Volumen de atenuación: {}%",
                    self.ducking_volume_percent
                ));
            }
            DeviceFocus::MicTest => {}
        }
    }

    /// Moves selection down in the currently focused dropdown list or increases ducking volume.
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
            DeviceFocus::AudioDucking => {
                self.ducking_volume_percent = (self.ducking_volume_percent + 5).min(100);
                self.has_unsaved_ducking_changes = true;
                self.status_message = Some(format!(
                    "Volumen de atenuación: {}%",
                    self.ducking_volume_percent
                ));
            }
            DeviceFocus::MicTest => {}
        }
    }

    /// Toggles the dropdown open/close state, ducking enabled state, or triggers mic test.
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
            DeviceFocus::AudioDucking => {
                self.ducking_enabled = !self.ducking_enabled;
                self.has_unsaved_ducking_changes = true;
                self.status_message = Some(format!(
                    "Atenuación de audio (Ducking): {}",
                    if self.ducking_enabled {
                        "Activada"
                    } else {
                        "Desactivada"
                    }
                ));
            }
            DeviceFocus::MicTest => {
                self.is_testing_mic = !self.is_testing_mic;
                if !self.is_testing_mic {
                    self.mic_level = 0.0;
                }
            }
        }
    }

    /// Persists audio ducking settings to `.env` file.
    pub fn save_ducking_config(&mut self) -> Result<(), String> {
        save_ducking_to_env(
            &self.env_path,
            self.ducking_enabled,
            self.ducking_volume_percent,
        )?;
        self.has_unsaved_ducking_changes = false;
        self.status_message = Some("Atenuación guardada en ~/.config/viernes/.env (0600)".into());
        Ok(())
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
                Constraint::Length(7), // Dropdown selectors (Mic & Speaker)
                Constraint::Length(6), // Row 3: VU meter (50%) & Audio Ducking (50%)
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

        // Row 3: Left = VU Meter (50%), Right = Audio Ducking Controls (50%)
        let row3_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(chunks[2]);

        // 3A. Audio Level Meter Widget (VU Meter)
        let vu_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Length(2)])
            .split(row3_chunks[0]);

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
                    .title(" Vúmetro de Entrada ")
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
            "● PRUEBA ACTIVA — Hable al micrófono"
        } else {
            "○ Presione [Espacio] o [m] para probar"
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

        // 3B. Background Audio Ducking Controls (PipeWire / PulseAudio)
        let ducking_border_style = if self.focus == DeviceFocus::AudioDucking {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let duck_status_span = if self.ducking_enabled {
            Span::styled(
                "[x] ACTIVADA (Música baja al hablar)",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            Span::styled(
                "[ ] DESACTIVADA (Volumen constante)",
                Style::default().fg(Color::DarkGray),
            )
        };

        let unsaved_badge = if self.has_unsaved_ducking_changes {
            Span::styled(" *[s: guardar]", Style::default().fg(Color::LightRed))
        } else {
            Span::raw("")
        };

        let ducking_title = if self.focus == DeviceFocus::AudioDucking {
            " Atenuación de Música (Ducking) [ACTIVO] "
        } else {
            " Atenuación de Música (Ducking) "
        };

        let ducking_box = Paragraph::new(vec![
            Line::from(vec![Span::raw("Estado: "), duck_status_span, unsaved_badge]),
            Line::from(vec![
                Span::raw("Volumen Música: "),
                Span::styled(
                    format!("{}%", self.ducking_volume_percent),
                    if self.ducking_enabled {
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    },
                ),
                Span::raw(" (Ajustar: [←/→] o [-/+])"),
            ]),
            Line::from(Span::styled(
                "Atenúa Spotify/navegador mientras Viernes escucha o habla",
                Style::default().fg(Color::DarkGray),
            )),
        ])
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(ducking_title)
                .border_style(ducking_border_style),
        );
        f.render_widget(ducking_box, row3_chunks[1]);

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
            Span::raw("Sección │ "),
            Span::styled("[↑/↓]: ", Style::default().fg(Color::Yellow)),
            Span::raw("Navegar │ "),
            Span::styled("[Enter/Espacio]: ", Style::default().fg(Color::Yellow)),
            Span::raw("Acción │ "),
            Span::styled("[←/→]: ", Style::default().fg(Color::Yellow)),
            Span::raw("Vol % │ "),
            Span::styled("[s]: ", Style::default().fg(Color::Yellow)),
            Span::raw("Guardar .env │ "),
            Span::styled("[r]: ", Style::default().fg(Color::Yellow)),
            Span::raw("Refrescar"),
        ]))
        .alignment(Alignment::Center);
        f.render_widget(help, chunks[4]);
    }
}

/// Helper to update or append audio ducking configuration in a `.env` file preserving comments and keys.
pub fn save_ducking_to_env(
    path: &std::path::Path,
    enabled: bool,
    volume_percent: u32,
) -> Result<(), String> {
    let lines: Vec<String> = if path.exists() {
        std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read env file: {e}"))?
            .lines()
            .map(|s| s.to_string())
            .collect()
    } else {
        Vec::new()
    };

    let mut new_lines = Vec::with_capacity(lines.len() + 2);
    let mut enabled_found = false;
    let mut percent_found = false;

    for line in lines {
        let trimmed = line.trim();
        if trimmed.starts_with("AUDIO_DUCKING_ENABLED=") {
            new_lines.push(format!("AUDIO_DUCKING_ENABLED={enabled}"));
            enabled_found = true;
        } else if trimmed.starts_with("AUDIO_DUCKING_VOLUME_PERCENT=") {
            new_lines.push(format!("AUDIO_DUCKING_VOLUME_PERCENT={volume_percent}"));
            percent_found = true;
        } else {
            new_lines.push(line);
        }
    }

    if !enabled_found {
        new_lines.push(format!("AUDIO_DUCKING_ENABLED={enabled}"));
    }
    if !percent_found {
        new_lines.push(format!("AUDIO_DUCKING_VOLUME_PERCENT={volume_percent}"));
    }

    let content = new_lines.join("\n") + "\n";
    let temp_path = path.with_extension("tmp");

    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    {
        use std::fs::OpenOptions;
        use std::io::Write;
        #[cfg(unix)]
        use std::os::unix::fs::PermissionsExt;

        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&temp_path)
            .map_err(|e| format!("Failed to create temporary file: {e}"))?;

        #[cfg(unix)]
        {
            if let Ok(metadata) = file.metadata() {
                let mut permissions = metadata.permissions();
                permissions.set_mode(0o600);
                let _ = file.set_permissions(permissions);
            }
        }

        file.write_all(content.as_bytes())
            .map_err(|e| format!("Failed to write content: {e}"))?;
        file.sync_all()
            .map_err(|e| format!("Failed to sync temporary file: {e}"))?;
    }

    std::fs::rename(&temp_path, path).map_err(|e| format!("Atomic rename failed: {e}"))?;

    Ok(())
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
        assert!(view.ducking_enabled);
        assert_eq!(view.ducking_volume_percent, 20);
    }

    #[test]
    fn test_devices_view_navigation_and_focus() {
        let (inputs, outputs) = make_test_devices();
        let mut view = DevicesView::new_mock(inputs, outputs);

        assert_eq!(view.focus, DeviceFocus::Microphone);
        view.next_focus();
        assert_eq!(view.focus, DeviceFocus::Speaker);
        view.next_focus();
        assert_eq!(view.focus, DeviceFocus::AudioDucking);
        view.next_focus();
        assert_eq!(view.focus, DeviceFocus::MicTest);
        view.next_focus();
        assert_eq!(view.focus, DeviceFocus::Microphone);

        // Reverse focus
        view.prev_focus();
        assert_eq!(view.focus, DeviceFocus::MicTest);
        view.prev_focus();
        assert_eq!(view.focus, DeviceFocus::AudioDucking);

        // Select next device in microphone list
        view.focus = DeviceFocus::Microphone;
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
    fn test_devices_view_audio_ducking_toggle_and_volume_adjustment() {
        let (inputs, outputs) = make_test_devices();
        let mut view = DevicesView::new_mock(inputs, outputs);

        view.focus = DeviceFocus::AudioDucking;
        assert!(view.ducking_enabled);
        assert_eq!(view.ducking_volume_percent, 20);

        // Toggle ducking
        view.toggle_action();
        assert!(!view.ducking_enabled);
        assert!(view.has_unsaved_ducking_changes);

        view.toggle_action();
        assert!(view.ducking_enabled);

        // Increase volume by 5%
        view.select_next();
        assert_eq!(view.ducking_volume_percent, 25);

        // Decrease volume by 5% twice
        view.select_prev();
        assert_eq!(view.ducking_volume_percent, 20);
        view.select_prev();
        assert_eq!(view.ducking_volume_percent, 15);
    }

    #[test]
    fn test_devices_view_audio_ducking_persistence_to_env() {
        let temp_env =
            std::env::temp_dir().join(format!("test_viernes_ducking_{}.env", std::process::id()));
        let _ = std::fs::remove_file(&temp_env);

        let initial_content =
            "# Test config\nELEVENLABS_VOICE_ID=test123\nAUDIO_DUCKING_ENABLED=false\n";
        std::fs::write(&temp_env, initial_content).unwrap();

        let (inputs, outputs) = make_test_devices();
        let mut view = DevicesView::new_mock(inputs, outputs);
        view.env_path = temp_env.clone();
        view.ducking_enabled = true;
        view.ducking_volume_percent = 35;
        view.has_unsaved_ducking_changes = true;

        let res = view.save_ducking_config();
        assert!(res.is_ok());
        assert!(!view.has_unsaved_ducking_changes);

        let saved_content = std::fs::read_to_string(&temp_env).unwrap();
        assert!(saved_content.contains("ELEVENLABS_VOICE_ID=test123"));
        assert!(saved_content.contains("AUDIO_DUCKING_ENABLED=true"));
        assert!(saved_content.contains("AUDIO_DUCKING_VOLUME_PERCENT=35"));

        let _ = std::fs::remove_file(&temp_env);
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
