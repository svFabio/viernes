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
    /// Viernes voice output volume boost controls.
    ViernesVolume,
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
    /// Target ducking volume percentage for background media (0..=100).
    pub ducking_volume_percent: u32,
    /// Playback volume percentage for Viernes assistant voice output (10..=200).
    pub viernes_volume_percent: u32,
    /// Whether ducking or device settings have unsaved modifications.
    pub has_unsaved_ducking_changes: bool,
    /// Preferred input device name loaded from config/env (if any).
    pub configured_input_device: Option<String>,
    /// Preferred output device name loaded from config/env (if any).
    pub configured_output_device: Option<String>,
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

        let (
            ducking_enabled,
            ducking_volume_percent,
            viernes_volume_percent,
            configured_input_device,
            configured_output_device,
        ) = match config {
            Some(c) => (
                c.audio_ducking_enabled,
                c.audio_ducking_volume_percent,
                c.viernes_volume_percent,
                c.audio_input_device.clone(),
                c.audio_output_device.clone(),
            ),
            None => {
                let enabled = std::env::var("AUDIO_DUCKING_ENABLED")
                    .map(|v| v.to_lowercase() == "true" || v == "1")
                    .unwrap_or(true);
                let pct = std::env::var("AUDIO_DUCKING_VOLUME_PERCENT")
                    .ok()
                    .and_then(|v| v.parse::<u32>().ok())
                    .unwrap_or(40)
                    .min(100);
                let v_pct = std::env::var("VIERNES_VOLUME_PERCENT")
                    .ok()
                    .and_then(|v| v.parse::<u32>().ok())
                    .unwrap_or(130)
                    .clamp(10, 200);
                let in_dev = std::env::var("AUDIO_INPUT_DEVICE")
                    .ok()
                    .map(|v| v.trim().to_string())
                    .filter(|v| !v.is_empty());
                let out_dev = std::env::var("AUDIO_OUTPUT_DEVICE")
                    .ok()
                    .map(|v| v.trim().to_string())
                    .filter(|v| !v.is_empty());
                (enabled, pct, v_pct, in_dev, out_dev)
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
            viernes_volume_percent,
            has_unsaved_ducking_changes: false,
            configured_input_device,
            configured_output_device,
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
            ducking_volume_percent: 40,
            viernes_volume_percent: 130,
            has_unsaved_ducking_changes: false,
            configured_input_device: None,
            configured_output_device: None,
            env_path: PathBuf::from("/tmp/test_viernes_mock.env"),
        };
        view.refresh_with_enumerator(&enumerator);
        view
    }

    /// Refreshes device collections using the provided enumerator.
    pub fn refresh_with_enumerator(&mut self, enumerator: &dyn DeviceEnumerator) {
        if let Ok(inputs) = enumerator.list_input_devices() {
            let configured_idx = self
                .configured_input_device
                .as_deref()
                .and_then(|name| inputs.iter().position(|d| d.name == name));
            self.selected_input = configured_idx
                .or_else(|| inputs.iter().position(|d| d.is_default))
                .unwrap_or(0);
            self.input_devices = inputs;
        }

        if let Ok(outputs) = enumerator.list_output_devices() {
            let configured_idx = self
                .configured_output_device
                .as_deref()
                .and_then(|name| outputs.iter().position(|d| d.name == name));
            self.selected_output = configured_idx
                .or_else(|| outputs.iter().position(|d| d.is_default))
                .unwrap_or(0);
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

    /// Cycles focus between Microphone, Speaker, AudioDucking, ViernesVolume, and MicTest.
    pub fn next_focus(&mut self) {
        self.focus = match self.focus {
            DeviceFocus::Microphone => DeviceFocus::Speaker,
            DeviceFocus::Speaker => DeviceFocus::AudioDucking,
            DeviceFocus::AudioDucking => DeviceFocus::ViernesVolume,
            DeviceFocus::ViernesVolume => DeviceFocus::MicTest,
            DeviceFocus::MicTest => DeviceFocus::Microphone,
        };
    }

    /// Cycles focus in reverse.
    pub fn prev_focus(&mut self) {
        self.focus = match self.focus {
            DeviceFocus::Microphone => DeviceFocus::MicTest,
            DeviceFocus::Speaker => DeviceFocus::Microphone,
            DeviceFocus::AudioDucking => DeviceFocus::Speaker,
            DeviceFocus::ViernesVolume => DeviceFocus::AudioDucking,
            DeviceFocus::MicTest => DeviceFocus::ViernesVolume,
        };
    }

    /// Moves selection up in the currently focused dropdown list or decreases volume percent.
    pub fn select_prev(&mut self) {
        match self.focus {
            DeviceFocus::Microphone => {
                if !self.input_devices.is_empty() {
                    if self.selected_input == 0 {
                        self.selected_input = self.input_devices.len() - 1;
                    } else {
                        self.selected_input -= 1;
                    }
                    if self.input_devices.len() > 1 {
                        self.has_unsaved_ducking_changes = true;
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
                    if self.output_devices.len() > 1 {
                        self.has_unsaved_ducking_changes = true;
                    }
                }
            }
            DeviceFocus::AudioDucking => {
                self.ducking_volume_percent = self.ducking_volume_percent.saturating_sub(5);
                self.has_unsaved_ducking_changes = true;
                self.status_message = Some(format!(
                    "Volumen de música atenuada: {}%",
                    self.ducking_volume_percent
                ));
            }
            DeviceFocus::ViernesVolume => {
                self.viernes_volume_percent = self.viernes_volume_percent.saturating_sub(5).max(10);
                self.has_unsaved_ducking_changes = true;
                self.status_message = Some(format!(
                    "Volumen de voz de Viernes: {}%",
                    self.viernes_volume_percent
                ));
            }
            DeviceFocus::MicTest => {}
        }
    }

    /// Moves selection down in the currently focused dropdown list or increases volume percent.
    pub fn select_next(&mut self) {
        match self.focus {
            DeviceFocus::Microphone => {
                if !self.input_devices.is_empty() {
                    self.selected_input = (self.selected_input + 1) % self.input_devices.len();
                    if self.input_devices.len() > 1 {
                        self.has_unsaved_ducking_changes = true;
                    }
                }
            }
            DeviceFocus::Speaker => {
                if !self.output_devices.is_empty() {
                    self.selected_output = (self.selected_output + 1) % self.output_devices.len();
                    if self.output_devices.len() > 1 {
                        self.has_unsaved_ducking_changes = true;
                    }
                }
            }
            DeviceFocus::AudioDucking => {
                self.ducking_volume_percent = (self.ducking_volume_percent + 5).min(100);
                self.has_unsaved_ducking_changes = true;
                self.status_message = Some(format!(
                    "Volumen de música atenuada: {}%",
                    self.ducking_volume_percent
                ));
            }
            DeviceFocus::ViernesVolume => {
                self.viernes_volume_percent = (self.viernes_volume_percent + 5).min(200);
                self.has_unsaved_ducking_changes = true;
                self.status_message = Some(format!(
                    "Volumen de voz de Viernes: {}%",
                    self.viernes_volume_percent
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
            DeviceFocus::ViernesVolume => {
                self.viernes_volume_percent = 130;
                self.has_unsaved_ducking_changes = true;
                self.status_message = Some("Volumen de Viernes restablecido a 130%".into());
            }
            DeviceFocus::MicTest => {
                self.is_testing_mic = !self.is_testing_mic;
                if !self.is_testing_mic {
                    self.mic_level = 0.0;
                }
            }
        }
    }

    /// Persists audio ducking, volume, and selected input/output devices to `.env` file.
    pub fn save_ducking_config(&mut self) -> Result<(), String> {
        let input_name = self.selected_input_device().map(|d| d.name.clone());
        let output_name = self.selected_output_device().map(|d| d.name.clone());
        save_audio_settings_to_env(
            &self.env_path,
            self.ducking_enabled,
            self.ducking_volume_percent,
            self.viernes_volume_percent,
            input_name.as_deref(),
            output_name.as_deref(),
        )?;
        self.configured_input_device = input_name;
        self.configured_output_device = output_name;
        self.has_unsaved_ducking_changes = false;
        self.status_message = Some(
            "Dispositivos, atenuación y volumen guardados en ~/.config/viernes/.env (0600)".into(),
        );
        Ok(())
    }

    /// Confirms current selection and closes open dropdowns.
    pub fn confirm_selection(&mut self) {
        let was_output_open = self.output_dropdown_open;
        self.input_dropdown_open = false;
        self.output_dropdown_open = false;
        if was_output_open || self.focus == DeviceFocus::Speaker {
            if let Some(dev) = self.output_devices.get(self.selected_output) {
                self.configured_output_device = Some(dev.name.clone());
                self.has_unsaved_ducking_changes = true;
                self.status_message = Some(format!("Altavoz seleccionado: {}", dev.name));
            }
        } else if let Some(dev) = self.input_devices.get(self.selected_input) {
            self.configured_input_device = Some(dev.name.clone());
            self.has_unsaved_ducking_changes = true;
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

        let clamped_level = if self.mic_level.is_finite() {
            self.mic_level.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let pct = (clamped_level * 100.0).round() as u16;
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

        // 3B. Audio Ducking & Assistant Voice Boost Controls (PipeWire / PulseAudio)
        let ducking_border_style = if self.focus == DeviceFocus::AudioDucking
            || self.focus == DeviceFocus::ViernesVolume
        {
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

        let ducking_title = match self.focus {
            DeviceFocus::AudioDucking => " Atenuación de Música [ACTIVA] & Volumen Viernes ",
            DeviceFocus::ViernesVolume => " Atenuación de Música & Volumen Viernes [ACTIVO] ",
            _ => " Atenuación de Música & Volumen Viernes ",
        };

        let music_prefix = if self.focus == DeviceFocus::AudioDucking {
            " ► Música Atenuada: "
        } else {
            "   Música Atenuada: "
        };
        let viernes_prefix = if self.focus == DeviceFocus::ViernesVolume {
            " ► Volumen Viernes: "
        } else {
            "   Volumen Viernes: "
        };

        let ducking_box = Paragraph::new(vec![
            Line::from(vec![Span::raw("Estado: "), duck_status_span, unsaved_badge]),
            Line::from(vec![
                Span::styled(
                    music_prefix,
                    if self.focus == DeviceFocus::AudioDucking {
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::White)
                    },
                ),
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
            Line::from(vec![
                Span::styled(
                    viernes_prefix,
                    if self.focus == DeviceFocus::ViernesVolume {
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::White)
                    },
                ),
                Span::styled(
                    format!("{}%", self.viernes_volume_percent),
                    Style::default()
                        .fg(Color::Magenta)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    " [MÁS ALTO / PRIORITARIO]",
                    Style::default().fg(Color::Green),
                ),
            ]),
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
    viernes_volume: u32,
) -> Result<(), String> {
    save_audio_settings_to_env(path, enabled, volume_percent, viernes_volume, None, None)
}

/// Atomically updates or appends audio ducking, volume, and optional device selections in `.env`
/// with restricted `0600` permissions while preserving unrelated keys and comments.
pub fn save_audio_settings_to_env(
    path: &std::path::Path,
    enabled: bool,
    volume_percent: u32,
    viernes_volume: u32,
    input_device: Option<&str>,
    output_device: Option<&str>,
) -> Result<(), String> {
    let lines: Vec<String> = if path.exists() {
        std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read env file: {e}"))?
            .lines()
            .map(ToString::to_string)
            .collect()
    } else {
        Vec::new()
    };

    let format_quoted = |val: &str| val.replace('"', "\\\"");

    let mut new_lines = Vec::with_capacity(lines.len() + 5);
    let mut enabled_found = false;
    let mut percent_found = false;
    let mut viernes_vol_found = false;
    let mut input_dev_found = false;
    let mut output_dev_found = false;

    for line in lines {
        let trimmed = line.trim();
        if !trimmed.starts_with('#') {
            if let Some((k, _)) = trimmed.split_once('=') {
                match k.trim() {
                    "AUDIO_DUCKING_ENABLED" => {
                        new_lines.push(format!("AUDIO_DUCKING_ENABLED={enabled}"));
                        enabled_found = true;
                        continue;
                    }
                    "AUDIO_DUCKING_VOLUME_PERCENT" => {
                        new_lines.push(format!("AUDIO_DUCKING_VOLUME_PERCENT={volume_percent}"));
                        percent_found = true;
                        continue;
                    }
                    "VIERNES_VOLUME_PERCENT" => {
                        new_lines.push(format!("VIERNES_VOLUME_PERCENT={viernes_volume}"));
                        viernes_vol_found = true;
                        continue;
                    }
                    "AUDIO_INPUT_DEVICE" => {
                        if let Some(in_dev) = input_device {
                            new_lines
                                .push(format!("AUDIO_INPUT_DEVICE=\"{}\"", format_quoted(in_dev)));
                            input_dev_found = true;
                            continue;
                        }
                    }
                    "AUDIO_OUTPUT_DEVICE" => {
                        if let Some(out_dev) = output_device {
                            new_lines.push(format!(
                                "AUDIO_OUTPUT_DEVICE=\"{}\"",
                                format_quoted(out_dev)
                            ));
                            output_dev_found = true;
                            continue;
                        }
                    }
                    _ => {}
                }
            }
        }
        new_lines.push(line);
    }

    if !enabled_found {
        new_lines.push(format!("AUDIO_DUCKING_ENABLED={enabled}"));
    }
    if !percent_found {
        new_lines.push(format!("AUDIO_DUCKING_VOLUME_PERCENT={volume_percent}"));
    }
    if !viernes_vol_found {
        new_lines.push(format!("VIERNES_VOLUME_PERCENT={viernes_volume}"));
    }
    if let Some(in_dev) = input_device {
        if !input_dev_found {
            new_lines.push(format!("AUDIO_INPUT_DEVICE=\"{}\"", format_quoted(in_dev)));
        }
    }
    if let Some(out_dev) = output_device {
        if !output_dev_found {
            new_lines.push(format!(
                "AUDIO_OUTPUT_DEVICE=\"{}\"",
                format_quoted(out_dev)
            ));
        }
    }

    let content = new_lines.join("\n") + "\n";

    crate::tui::ensure_parent_dir(path)?;

    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("env");
    let temp_path = path.with_file_name(format!(".{file_name}.tmp.{}", uuid::Uuid::new_v4()));

    use std::fs::OpenOptions;
    use std::io::Write;
    #[cfg(unix)]
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    let mut open_opts = OpenOptions::new();
    open_opts.write(true).create_new(true);
    #[cfg(unix)]
    open_opts.mode(0o600);

    let mut file = open_opts
        .open(&temp_path)
        .map_err(|e| format!("Failed to create temporary file: {e}"))?;

    let write_result = (|| -> Result<(), String> {
        #[cfg(unix)]
        {
            let mut permissions = file
                .metadata()
                .map_err(|e| format!("Failed to read metadata: {e}"))?
                .permissions();
            permissions.set_mode(0o600);
            file.set_permissions(permissions)
                .map_err(|e| format!("Failed to set 0600 permissions: {e}"))?;
        }

        file.write_all(content.as_bytes())
            .map_err(|e| format!("Failed to write content: {e}"))?;
        file.sync_all()
            .map_err(|e| format!("Failed to sync temporary file: {e}"))?;
        drop(file);

        std::fs::rename(&temp_path, path).map_err(|e| format!("Atomic rename failed: {e}"))?;
        Ok(())
    })();

    if let Err(err) = write_result {
        let _ = std::fs::remove_file(&temp_path);
        return Err(err);
    }

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
        assert_eq!(view.ducking_volume_percent, 40);
        assert_eq!(view.viernes_volume_percent, 130);
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
        assert_eq!(view.focus, DeviceFocus::ViernesVolume);
        view.next_focus();
        assert_eq!(view.focus, DeviceFocus::MicTest);
        view.next_focus();
        assert_eq!(view.focus, DeviceFocus::Microphone);

        // Reverse focus
        view.prev_focus();
        assert_eq!(view.focus, DeviceFocus::MicTest);
        view.prev_focus();
        assert_eq!(view.focus, DeviceFocus::ViernesVolume);
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
        assert_eq!(view.ducking_volume_percent, 40);

        // Toggle ducking
        view.toggle_action();
        assert!(!view.ducking_enabled);
        assert!(view.has_unsaved_ducking_changes);

        view.toggle_action();
        assert!(view.ducking_enabled);

        // Increase volume by 5%
        view.select_next();
        assert_eq!(view.ducking_volume_percent, 45);

        // Decrease volume by 5% twice
        view.select_prev();
        assert_eq!(view.ducking_volume_percent, 40);
        view.select_prev();
        assert_eq!(view.ducking_volume_percent, 35);

        // Test Viernes volume adjustment
        view.focus = DeviceFocus::ViernesVolume;
        assert_eq!(view.viernes_volume_percent, 130);
        view.select_next();
        assert_eq!(view.viernes_volume_percent, 135);
        view.select_prev();
        assert_eq!(view.viernes_volume_percent, 130);
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
        view.ducking_volume_percent = 45;
        view.viernes_volume_percent = 140;
        view.has_unsaved_ducking_changes = true;

        let res = view.save_ducking_config();
        assert!(res.is_ok());
        assert!(!view.has_unsaved_ducking_changes);

        let saved_content = std::fs::read_to_string(&temp_env).unwrap();
        assert!(saved_content.contains("ELEVENLABS_VOICE_ID=test123"));
        assert!(saved_content.contains("AUDIO_DUCKING_ENABLED=true"));
        assert!(saved_content.contains("AUDIO_DUCKING_VOLUME_PERCENT=45"));
        assert!(saved_content.contains("VIERNES_VOLUME_PERCENT=140"));

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

    #[test]
    fn test_devices_view_persists_input_and_output_devices_with_0600() {
        let dir = tempfile::tempdir().unwrap();
        let env_path = dir.path().join(".env");
        std::fs::write(&env_path, "ELEVENLABS_VOICE_ID=\"jessica123\"\n").unwrap();

        let (inputs, outputs) = make_test_devices();
        let mut view = DevicesView::new_mock(inputs, outputs);
        view.env_path = env_path.clone();

        // Select second input device and second output device
        view.focus = DeviceFocus::Microphone;
        view.select_next();
        view.confirm_selection();
        assert!(view.has_unsaved_ducking_changes);

        view.focus = DeviceFocus::Speaker;
        view.select_next();
        view.confirm_selection();

        view.save_ducking_config().unwrap();
        assert!(!view.has_unsaved_ducking_changes);

        let saved = std::fs::read_to_string(&env_path).unwrap();
        assert!(saved.contains("ELEVENLABS_VOICE_ID=\"jessica123\""));
        assert!(saved.contains("AUDIO_INPUT_DEVICE=\"USB Mic Studio\""));
        assert!(saved.contains("AUDIO_OUTPUT_DEVICE=\"Headphones Analog\""));

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&env_path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
    }

    #[test]
    fn test_devices_view_render_clamps_mic_level_over_one() {
        let (inputs, outputs) = make_test_devices();
        let mut view = DevicesView::new_mock(inputs, outputs);
        // Bypass set_mic_level to verify render() defensively clamps out-of-range values
        view.mic_level = 1.75;

        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                view.render(f, f.area());
            })
            .unwrap();

        let rendered = format!("{:?}", terminal.backend().buffer());
        assert!(rendered.contains("100%"));

        view.mic_level = f32::NAN;
        terminal
            .draw(|f| {
                view.render(f, f.area());
            })
            .unwrap();
        let rendered_nan = format!("{:?}", terminal.backend().buffer());
        assert!(rendered_nan.contains("0%"));
    }
}
