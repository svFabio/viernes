"""Modern Desktop Control Center for Viernes Voice Assistant.

Refactored with executive design system tokens, Montserrat typography,
card-based layout architecture, and zero hardcoded HEX values.
"""

from __future__ import annotations

import logging
import threading
import tkinter as tk
from typing import Final

import customtkinter as ctk

from viernes.gui.service import PRESET_VOICES, ConfigService
from viernes.gui.theme import Colors, Radius, Spacing, TypographyManager, create_card

logger = logging.getLogger(__name__)

WINDOW_TITLE: Final[str] = "Viernes — Centro de Control"
WINDOW_GEOMETRY: Final[str] = "720x820"
WINDOW_MINSIZE: Final[tuple[int, int]] = (660, 720)


class ViernesControlCenter(ctk.CTk):
    """Executive Control Center GUI for Viernes Voice Assistant."""

    def __init__(self, config_service: ConfigService | None = None) -> None:
        super().__init__()
        self.config_service = config_service or ConfigService()

        # Appearance & Window Configuration
        ctk.set_appearance_mode("Dark")
        ctk.set_default_color_theme("blue")

        self.title(WINDOW_TITLE)
        self.geometry(WINDOW_GEOMETRY)
        self.minsize(*WINDOW_MINSIZE)
        self.configure(fg_color=Colors.BG_APP)

        # Initialize Typography Engine
        TypographyManager.initialize_fonts()

        # Load existing config
        self.current_config = self.config_service.read_config()

        # Construct Interface Hierarchy
        self._build_header()
        self._build_tabs()
        self._build_footer()

        # Populate initial values
        self._load_config_values()
        self._update_service_status_badge()

    # -------------------------------------------------------------------------
    # Header Section
    # -------------------------------------------------------------------------
    def _build_header(self) -> None:
        """Constructs modern executive top header and live status badge."""
        self.header_card = create_card(
            self,
            fg_color=Colors.BG_HEADER,
            border_color=Colors.BORDER_DEFAULT,
            border_width=1,
            corner_radius=Radius.HEADER,
        )
        self.header_card.pack(
            fill="x",
            padx=Spacing.MD,
            pady=(Spacing.MD, Spacing.SM),
        )

        # Header inner horizontal container
        top_row = ctk.CTkFrame(self.header_card, fg_color="transparent")
        top_row.pack(fill="x", padx=Spacing.MD, pady=(Spacing.MD, Spacing.XS))

        # Brand Title
        title_label = ctk.CTkLabel(
            top_row,
            text="VIERNES",
            font=TypographyManager.display(),
            text_color=Colors.TEXT_PRIMARY,
        )
        title_label.pack(side="left", anchor="w")

        # Pill Status Badge Container
        self.status_pill = ctk.CTkFrame(
            top_row,
            fg_color=Colors.BG_CARD_SUBTLE,
            border_width=1,
            border_color=Colors.BORDER_MUTED,
            corner_radius=Radius.PILL,
        )
        self.status_pill.pack(side="right", anchor="e")

        self.status_badge = ctk.CTkLabel(
            self.status_pill,
            text="○ Comprobando...",
            font=TypographyManager.badge(),
            text_color=Colors.TEXT_MUTED,
            padx=10,
            pady=4,
        )
        self.status_badge.pack()

        # Architecture Subtitle
        subtitle_label = ctk.CTkLabel(
            self.header_card,
            text="SISTEMA DE ASISTENCIA POR VOZ // CENTRO DE CONTROL",
            font=TypographyManager.caption_bold(),
            text_color=Colors.TEXT_MUTED,
        )
        subtitle_label.pack(anchor="w", padx=Spacing.MD, pady=(0, Spacing.MD))

    # -------------------------------------------------------------------------
    # Tab Navigation
    # -------------------------------------------------------------------------
    def _build_tabs(self) -> None:
        """Constructs tab navigation using design tokens."""
        self.tabview = ctk.CTkTabview(
            self,
            corner_radius=Radius.CARD,
            fg_color=Colors.BG_APP,
            segmented_button_fg_color=Colors.BG_CARD_SUBTLE,
            segmented_button_selected_color=Colors.PRIMARY,
            segmented_button_selected_hover_color=Colors.PRIMARY_HOVER,
            segmented_button_unselected_color=Colors.BG_CARD,
            segmented_button_unselected_hover_color=Colors.NEUTRAL_BTN_HOVER,
            border_width=1,
            border_color=Colors.BORDER_DEFAULT,
        )
        self.tabview.pack(
            fill="both",
            expand=True,
            padx=Spacing.MD,
            pady=Spacing.SM,
        )

        self.tab_voice = self.tabview.add("Credenciales & Voz")
        self.tab_audio = self.tabview.add("Dispositivos de Audio")
        self.tab_fillers = self.tabview.add("Frases de Relleno")
        self.tab_service = self.tabview.add("Servicio & Estado")

        self._build_voice_tab()
        self._build_audio_tab()
        self._build_fillers_tab()
        self._build_service_tab()

    # -------------------------------------------------------------------------
    # Tab 1: Voz & Credenciales
    # -------------------------------------------------------------------------
    def _build_voice_tab(self) -> None:
        """Builds credentials and TTS voice selection tab."""
        container = ctk.CTkScrollableFrame(self.tab_voice, fg_color="transparent")
        container.pack(fill="both", expand=True, padx=Spacing.SM, pady=Spacing.SM)

        # Card 1: API Keys (Pool Multicuenta)
        key_card = create_card(container)
        key_card.pack(fill="x", pady=(0, Spacing.MD))

        key_title = ctk.CTkLabel(
            key_card,
            text="ElevenLabs API Key(s) [Pool Multicuenta]:",
            font=TypographyManager.card_title(),
            text_color=Colors.TEXT_PRIMARY,
        )
        key_title.pack(anchor="w", padx=Spacing.MD, pady=(Spacing.MD, Spacing.XS))

        key_desc = ctk.CTkLabel(
            key_card,
            text="Ingrese una clave o múltiples claves separadas por coma para rotación automática anti-cuotas.",
            font=TypographyManager.caption(),
            text_color=Colors.TEXT_MUTED,
        )
        key_desc.pack(anchor="w", padx=Spacing.MD, pady=(0, Spacing.SM))

        key_frame = ctk.CTkFrame(key_card, fg_color="transparent")
        key_frame.pack(fill="x", padx=Spacing.MD, pady=(0, Spacing.MD))

        self.api_key_entry = ctk.CTkEntry(
            key_frame,
            placeholder_text="sk_..., sk_... (separadas por coma)",
            show="*",
            height=38,
            corner_radius=Radius.INPUT,
            border_color=Colors.ENTRY_BORDER,
            fg_color=Colors.ENTRY_BG,
            text_color=Colors.TEXT_PRIMARY,
            font=TypographyManager.mono(),
        )
        self.api_key_entry.pack(side="left", fill="x", expand=True, padx=(0, Spacing.SM))

        self.show_key_var = tk.BooleanVar(value=False)
        self.toggle_key_btn = ctk.CTkButton(
            key_frame,
            text="Ver",
            width=70,
            height=38,
            corner_radius=Radius.BUTTON,
            fg_color=Colors.NEUTRAL_BTN,
            hover_color=Colors.NEUTRAL_BTN_HOVER,
            text_color=Colors.NEUTRAL_TEXT,
            font=TypographyManager.subheading(),
            command=self._toggle_api_key_visibility,
        )
        self.toggle_key_btn.pack(side="right")

        # Card 2: Preset Voices & Live Audition
        voice_card = create_card(container)
        voice_card.pack(fill="x", pady=(0, Spacing.MD))

        voice_title = ctk.CTkLabel(
            voice_card,
            text="Voz Seleccionada & Audición en Vivo:",
            font=TypographyManager.card_title(),
            text_color=Colors.TEXT_PRIMARY,
        )
        voice_title.pack(anchor="w", padx=Spacing.MD, pady=(Spacing.MD, Spacing.XS))

        voice_options = list(PRESET_VOICES.keys()) + ["Personalizada (Custom ID)"]
        self.voice_combo = ctk.CTkOptionMenu(
            voice_card,
            values=voice_options,
            command=self._on_voice_preset_selected,
            height=38,
            corner_radius=Radius.BUTTON,
            fg_color=Colors.BG_CARD_SUBTLE,
            button_color=Colors.PRIMARY,
            button_hover_color=Colors.PRIMARY_HOVER,
            text_color=Colors.TEXT_PRIMARY,
            font=TypographyManager.subheading(),
        )
        self.voice_combo.pack(fill="x", padx=Spacing.MD, pady=(0, Spacing.SM))

        # Custom Voice ID entry
        id_label = ctk.CTkLabel(
            voice_card,
            text="Voice ID de ElevenLabs:",
            font=TypographyManager.caption_bold(),
            text_color=Colors.TEXT_SECONDARY,
        )
        id_label.pack(anchor="w", padx=Spacing.MD, pady=(Spacing.XS, 2))

        self.voice_id_entry = ctk.CTkEntry(
            voice_card,
            placeholder_text="ID de la voz (ej: cgSgspJ2msm6clMCkdW9)",
            height=38,
            corner_radius=Radius.INPUT,
            border_color=Colors.ENTRY_BORDER,
            fg_color=Colors.ENTRY_BG,
            text_color=Colors.TEXT_PRIMARY,
            font=TypographyManager.mono(),
        )
        self.voice_id_entry.pack(fill="x", padx=Spacing.MD, pady=(0, Spacing.MD))

        # Audition Test Button & Status
        self.test_voice_btn = ctk.CTkButton(
            voice_card,
            text="▶ Probar Voz (Audición en Vivo)",
            height=42,
            corner_radius=Radius.BUTTON,
            fg_color=Colors.PRIMARY,
            hover_color=Colors.PRIMARY_HOVER,
            text_color=Colors.TEXT_PRIMARY,
            font=TypographyManager.body_bold(),
            command=self._on_test_voice_clicked,
        )
        self.test_voice_btn.pack(fill="x", padx=Spacing.MD, pady=(0, Spacing.SM))

        self.voice_test_status = ctk.CTkLabel(
            voice_card,
            text="",
            font=TypographyManager.caption(),
            text_color=Colors.TEXT_MUTED,
        )
        self.voice_test_status.pack(anchor="w", padx=Spacing.MD, pady=(0, Spacing.MD))

    # -------------------------------------------------------------------------
    # Tab 2: Dispositivos de Audio & VAD
    # -------------------------------------------------------------------------
    def _build_audio_tab(self) -> None:
        """Builds audio input/output and VAD sensitivity tab."""
        container = ctk.CTkScrollableFrame(self.tab_audio, fg_color="transparent")
        container.pack(fill="both", expand=True, padx=Spacing.SM, pady=Spacing.SM)

        # Card 1: Hardware Devices
        dev_card = create_card(container)
        dev_card.pack(fill="x", pady=(0, Spacing.MD))

        dev_title = ctk.CTkLabel(
            dev_card,
            text="Enrutamiento de Audio Hardware:",
            font=TypographyManager.card_title(),
            text_color=Colors.TEXT_PRIMARY,
        )
        dev_title.pack(anchor="w", padx=Spacing.MD, pady=(Spacing.MD, Spacing.SM))

        # Microphones
        input_label = ctk.CTkLabel(
            dev_card,
            text="Micrófono de Entrada:",
            font=TypographyManager.subheading(),
            text_color=Colors.TEXT_SECONDARY,
        )
        input_label.pack(anchor="w", padx=Spacing.MD, pady=(Spacing.XS, 2))

        self.input_devices = self.config_service.get_audio_input_devices()
        input_displays = [disp for disp, _ in self.input_devices]
        self.input_combo = ctk.CTkOptionMenu(
            dev_card,
            values=input_displays or ["Por defecto del sistema"],
            height=38,
            corner_radius=Radius.BUTTON,
            fg_color=Colors.BG_CARD_SUBTLE,
            button_color=Colors.PRIMARY,
            button_hover_color=Colors.PRIMARY_HOVER,
            text_color=Colors.TEXT_PRIMARY,
            font=TypographyManager.body(),
        )
        self.input_combo.pack(fill="x", padx=Spacing.MD, pady=(0, Spacing.MD))

        # Speakers
        output_label = ctk.CTkLabel(
            dev_card,
            text="Dispositivo de Salida (Altavoces):",
            font=TypographyManager.subheading(),
            text_color=Colors.TEXT_SECONDARY,
        )
        output_label.pack(anchor="w", padx=Spacing.MD, pady=(Spacing.XS, 2))

        self.output_devices = self.config_service.get_audio_output_devices()
        output_displays = [disp for disp, _ in self.output_devices]
        self.output_combo = ctk.CTkOptionMenu(
            dev_card,
            values=output_displays or ["Por defecto del sistema"],
            height=38,
            corner_radius=Radius.BUTTON,
            fg_color=Colors.BG_CARD_SUBTLE,
            button_color=Colors.PRIMARY,
            button_hover_color=Colors.PRIMARY_HOVER,
            text_color=Colors.TEXT_PRIMARY,
            font=TypographyManager.body(),
        )
        self.output_combo.pack(fill="x", padx=Spacing.MD, pady=(0, Spacing.MD))

        # Card 2: VAD Sensitivity & Silence Cutout
        tuning_card = create_card(container)
        tuning_card.pack(fill="x", pady=(0, Spacing.MD))

        tuning_title = ctk.CTkLabel(
            tuning_card,
            text="Detección de Actividad de Voz (VAD):",
            font=TypographyManager.card_title(),
            text_color=Colors.TEXT_PRIMARY,
        )
        tuning_title.pack(anchor="w", padx=Spacing.MD, pady=(Spacing.MD, Spacing.SM))

        # VAD Energy Threshold Slider
        self.vad_label = ctk.CTkLabel(
            tuning_card,
            text="Umbral de Energía VAD:",
            font=TypographyManager.subheading(),
            text_color=Colors.TEXT_SECONDARY,
        )
        self.vad_label.pack(anchor="w", padx=Spacing.MD, pady=(Spacing.XS, 2))

        self.vad_slider = ctk.CTkSlider(
            tuning_card,
            from_=0.002,
            to=0.05,
            number_of_steps=48,
            progress_color=Colors.SLIDER_PROGRESS,
            button_color=Colors.SLIDER_BUTTON,
            button_hover_color=Colors.SLIDER_BUTTON_HOVER,
            command=self._on_vad_slider_changed,
        )
        self.vad_slider.pack(fill="x", padx=Spacing.MD, pady=(0, Spacing.XS))

        self.vad_val_label = ctk.CTkLabel(
            tuning_card,
            text="0.0100 (Recomendado)",
            font=TypographyManager.caption_bold(),
            text_color=Colors.TEXT_MUTED,
        )
        self.vad_val_label.pack(anchor="w", padx=Spacing.MD, pady=(0, Spacing.MD))

        # Silence Cutout Slider
        self.silence_label = ctk.CTkLabel(
            tuning_card,
            text="Tiempo de Silencio para Corte (ms):",
            font=TypographyManager.subheading(),
            text_color=Colors.TEXT_SECONDARY,
        )
        self.silence_label.pack(anchor="w", padx=Spacing.MD, pady=(Spacing.XS, 2))

        self.silence_slider = ctk.CTkSlider(
            tuning_card,
            from_=300,
            to=2000,
            number_of_steps=17,
            progress_color=Colors.SLIDER_PROGRESS,
            button_color=Colors.SLIDER_BUTTON,
            button_hover_color=Colors.SLIDER_BUTTON_HOVER,
            command=self._on_silence_slider_changed,
        )
        self.silence_slider.pack(fill="x", padx=Spacing.MD, pady=(0, Spacing.XS))

        self.silence_val_label = ctk.CTkLabel(
            tuning_card,
            text="800 ms",
            font=TypographyManager.caption_bold(),
            text_color=Colors.TEXT_MUTED,
        )
        self.silence_val_label.pack(anchor="w", padx=Spacing.MD, pady=(0, Spacing.MD))

    # -------------------------------------------------------------------------
    # Tab 3: Frases de Relleno (Conversational Fillers)
    # -------------------------------------------------------------------------
    def _build_fillers_tab(self) -> None:
        """Builds conversational filler phrase configuration tab."""
        container = ctk.CTkScrollableFrame(self.tab_fillers, fg_color="transparent")
        container.pack(fill="both", expand=True, padx=Spacing.SM, pady=Spacing.SM)

        # Overview Card
        intro_card = create_card(container)
        intro_card.pack(fill="x", pady=(0, Spacing.MD))

        intro_title = ctk.CTkLabel(
            intro_card,
            text="Frases de Relleno (Conversational Fillers):",
            font=TypographyManager.card_title(),
            text_color=Colors.TEXT_PRIMARY,
        )
        intro_title.pack(anchor="w", padx=Spacing.MD, pady=(Spacing.MD, Spacing.XS))

        desc_label = ctk.CTkLabel(
            intro_card,
            text=(
                "Estas frases son vocalizadas por Viernes de inmediato para enmascarar la latencia de red "
                "mientras Antigravity procesa o ejecuta herramientas. Modifíquelas (una frase por línea)."
            ),
            font=TypographyManager.caption(),
            text_color=Colors.TEXT_MUTED,
            wraplength=640,
            justify="left",
        )
        desc_label.pack(anchor="w", padx=Spacing.MD, pady=(0, Spacing.MD))

        from viernes.audio.fillers import load_custom_fillers

        generic, tools, patience = load_custom_fillers()

        # Fillers Form Card
        form_card = create_card(container)
        form_card.pack(fill="x", pady=(0, Spacing.MD))

        # 1. Generic fillers
        gen_label = ctk.CTkLabel(
            form_card,
            text="Frases Inmediatas / Genéricas (al detectar voz, < 200ms):",
            font=TypographyManager.subheading(),
            text_color=Colors.TEXT_SECONDARY,
        )
        gen_label.pack(anchor="w", padx=Spacing.MD, pady=(Spacing.MD, 2))
        self.fillers_generic_txt = ctk.CTkTextbox(
            form_card,
            height=85,
            corner_radius=Radius.INPUT,
            border_width=1,
            border_color=Colors.ENTRY_BORDER,
            fg_color=Colors.ENTRY_BG,
            font=TypographyManager.mono(),
        )
        self.fillers_generic_txt.pack(fill="x", padx=Spacing.MD, pady=(0, Spacing.MD))
        self.fillers_generic_txt.insert("1.0", "\n".join(generic))

        # 2. Patience fillers
        pat_label = ctk.CTkLabel(
            form_card,
            text="Frases de Paciencia (si la respuesta tarda más de 6.5s):",
            font=TypographyManager.subheading(),
            text_color=Colors.TEXT_SECONDARY,
        )
        pat_label.pack(anchor="w", padx=Spacing.MD, pady=(0, 2))
        self.fillers_patience_txt = ctk.CTkTextbox(
            form_card,
            height=60,
            corner_radius=Radius.INPUT,
            border_width=1,
            border_color=Colors.ENTRY_BORDER,
            fg_color=Colors.ENTRY_BG,
            font=TypographyManager.mono(),
        )
        self.fillers_patience_txt.pack(fill="x", padx=Spacing.MD, pady=(0, Spacing.MD))
        self.fillers_patience_txt.insert("1.0", "\n".join(patience))

        # 3. Tool fillers - Files
        files_label = ctk.CTkLabel(
            form_card,
            text="Herramientas: Lectura y Búsqueda de Archivos:",
            font=TypographyManager.subheading(),
            text_color=Colors.TEXT_SECONDARY,
        )
        files_label.pack(anchor="w", padx=Spacing.MD, pady=(0, 2))
        self.fillers_files_txt = ctk.CTkTextbox(
            form_card,
            height=55,
            corner_radius=Radius.INPUT,
            border_width=1,
            border_color=Colors.ENTRY_BORDER,
            fg_color=Colors.ENTRY_BG,
            font=TypographyManager.mono(),
        )
        self.fillers_files_txt.pack(fill="x", padx=Spacing.MD, pady=(0, Spacing.MD))
        self.fillers_files_txt.insert("1.0", "\n".join(tools.get("files", [])))

        # 4. Tool fillers - Terminal
        term_label = ctk.CTkLabel(
            form_card,
            text="Herramientas: Ejecución en Terminal:",
            font=TypographyManager.subheading(),
            text_color=Colors.TEXT_SECONDARY,
        )
        term_label.pack(anchor="w", padx=Spacing.MD, pady=(0, 2))
        self.fillers_term_txt = ctk.CTkTextbox(
            form_card,
            height=55,
            corner_radius=Radius.INPUT,
            border_width=1,
            border_color=Colors.ENTRY_BORDER,
            fg_color=Colors.ENTRY_BG,
            font=TypographyManager.mono(),
        )
        self.fillers_term_txt.pack(fill="x", padx=Spacing.MD, pady=(0, Spacing.MD))
        self.fillers_term_txt.insert("1.0", "\n".join(tools.get("terminal", [])))

        # 5. Tool fillers - Search
        search_label = ctk.CTkLabel(
            form_card,
            text="Herramientas: Búsqueda en la Web:",
            font=TypographyManager.subheading(),
            text_color=Colors.TEXT_SECONDARY,
        )
        search_label.pack(anchor="w", padx=Spacing.MD, pady=(0, 2))
        self.fillers_search_txt = ctk.CTkTextbox(
            form_card,
            height=50,
            corner_radius=Radius.INPUT,
            border_width=1,
            border_color=Colors.ENTRY_BORDER,
            fg_color=Colors.ENTRY_BG,
            font=TypographyManager.mono(),
        )
        self.fillers_search_txt.pack(fill="x", padx=Spacing.MD, pady=(0, Spacing.MD))
        self.fillers_search_txt.insert("1.0", "\n".join(tools.get("search", [])))

        # Fillers Actions
        btn_row = ctk.CTkFrame(form_card, fg_color="transparent")
        btn_row.pack(fill="x", padx=Spacing.MD, pady=(0, Spacing.MD))

        self.btn_save_fillers = ctk.CTkButton(
            btn_row,
            text="✔ Guardar Frases de Relleno",
            height=38,
            corner_radius=Radius.BUTTON,
            fg_color=Colors.SUCCESS,
            hover_color=Colors.SUCCESS_HOVER,
            text_color=Colors.TEXT_PRIMARY,
            font=TypographyManager.body_bold(),
            command=self._on_save_fillers_clicked,
        )
        self.btn_save_fillers.pack(side="left", fill="x", expand=True, padx=(0, Spacing.SM))

        self.btn_reset_fillers = ctk.CTkButton(
            btn_row,
            text="↺ Restablecer Predeterminadas",
            height=38,
            corner_radius=Radius.BUTTON,
            fg_color=Colors.NEUTRAL_BTN,
            hover_color=Colors.NEUTRAL_BTN_HOVER,
            text_color=Colors.NEUTRAL_TEXT,
            font=TypographyManager.body_bold(),
            command=self._on_reset_fillers_clicked,
        )
        self.btn_reset_fillers.pack(side="right", padx=(Spacing.SM, 0))

        self.fillers_status_label = ctk.CTkLabel(
            form_card,
            text="",
            font=TypographyManager.caption_bold(),
            text_color=Colors.TEXT_MUTED,
        )
        self.fillers_status_label.pack(anchor="w", padx=Spacing.MD, pady=(0, Spacing.MD))

    def _on_save_fillers_clicked(self) -> None:
        """Saves custom filler phrases from textboxes into fillers.json."""
        from viernes.audio.fillers import save_custom_fillers

        def _lines(txt_widget: ctk.CTkTextbox) -> list[str]:
            raw = txt_widget.get("1.0", "end")
            return [line.strip() for line in raw.splitlines() if line.strip()]

        generic = _lines(self.fillers_generic_txt)
        patience = _lines(self.fillers_patience_txt)
        tools = {
            "files": _lines(self.fillers_files_txt),
            "terminal": _lines(self.fillers_term_txt),
            "search": _lines(self.fillers_search_txt),
        }

        try:
            save_custom_fillers(generic=generic, tool_phrases=tools, patience=patience)
            is_active, _ = self.config_service.get_service_status()
            if is_active:
                self.config_service.control_service("restart")
                self.fillers_status_label.configure(
                    text="✔ Frases guardadas y servicio reiniciado con el nuevo caché.",
                    text_color=Colors.SUCCESS_TEXT,
                )
            else:
                self.fillers_status_label.configure(
                    text="✔ Frases de relleno guardadas exitosamente.",
                    text_color=Colors.SUCCESS_TEXT,
                )
        except Exception as exc:
            self.fillers_status_label.configure(
                text=f"Error al guardar: {exc}",
                text_color=Colors.DANGER_TEXT,
            )

    def _on_reset_fillers_clicked(self) -> None:
        """Resets textboxes to the default built-in filler phrases."""
        from viernes.audio.fillers import (
            DEFAULT_FILLER_PHRASES,
            PATIENCE_FILLER_PHRASES,
            TOOL_FILLER_PHRASES,
        )

        self.fillers_generic_txt.delete("1.0", "end")
        self.fillers_generic_txt.insert("1.0", "\n".join(DEFAULT_FILLER_PHRASES))

        self.fillers_patience_txt.delete("1.0", "end")
        self.fillers_patience_txt.insert("1.0", "\n".join(PATIENCE_FILLER_PHRASES))

        self.fillers_files_txt.delete("1.0", "end")
        self.fillers_files_txt.insert("1.0", "\n".join(TOOL_FILLER_PHRASES.get("files", [])))

        self.fillers_term_txt.delete("1.0", "end")
        self.fillers_term_txt.insert("1.0", "\n".join(TOOL_FILLER_PHRASES.get("terminal", [])))

        self.fillers_search_txt.delete("1.0", "end")
        self.fillers_search_txt.insert("1.0", "\n".join(TOOL_FILLER_PHRASES.get("search", [])))

        self.fillers_status_label.configure(
            text="Frases predeterminadas restauradas en el formulario. Haga clic en Guardar para confirmar.",
            text_color=Colors.INFO_TEXT,
        )

    # -------------------------------------------------------------------------
    # Tab 4: Servicio Systemd & Memoria
    # -------------------------------------------------------------------------
    def _build_service_tab(self) -> None:
        """Builds background service and system management tab."""
        container = ctk.CTkScrollableFrame(self.tab_service, fg_color="transparent")
        container.pack(fill="both", expand=True, padx=Spacing.SM, pady=Spacing.SM)

        # Card 1: Systemd Service Status & Controls
        service_card = create_card(container)
        service_card.pack(fill="x", pady=(0, Spacing.MD))

        self.service_info_label = ctk.CTkLabel(
            service_card,
            text="Servicio Systemd: viernes.service",
            font=TypographyManager.card_title(),
            text_color=Colors.TEXT_PRIMARY,
        )
        self.service_info_label.pack(anchor="w", padx=Spacing.MD, pady=(Spacing.MD, Spacing.XS))

        self.service_state_desc = ctk.CTkLabel(
            service_card,
            text="Estado: Consultando...",
            font=TypographyManager.caption(),
            text_color=Colors.TEXT_MUTED,
        )
        self.service_state_desc.pack(anchor="w", padx=Spacing.MD, pady=(0, Spacing.MD))

        # Control Buttons
        btn_frame = ctk.CTkFrame(service_card, fg_color="transparent")
        btn_frame.pack(fill="x", padx=Spacing.MD, pady=(0, Spacing.SM))

        self.btn_start = ctk.CTkButton(
            btn_frame,
            text="Iniciar Servicio",
            command=lambda: self._trigger_service_action("start"),
            height=38,
            corner_radius=Radius.BUTTON,
            fg_color=Colors.SUCCESS,
            hover_color=Colors.SUCCESS_HOVER,
            text_color=Colors.TEXT_PRIMARY,
            font=TypographyManager.body_bold(),
        )
        self.btn_start.pack(side="left", fill="x", expand=True, padx=(0, Spacing.XS))

        self.btn_restart = ctk.CTkButton(
            btn_frame,
            text="Reiniciar Servicio",
            command=lambda: self._trigger_service_action("restart"),
            height=38,
            corner_radius=Radius.BUTTON,
            fg_color=Colors.PRIMARY,
            hover_color=Colors.PRIMARY_HOVER,
            text_color=Colors.TEXT_PRIMARY,
            font=TypographyManager.body_bold(),
        )
        self.btn_restart.pack(side="left", fill="x", expand=True, padx=Spacing.XS)

        self.btn_stop = ctk.CTkButton(
            btn_frame,
            text="Detener Servicio",
            command=lambda: self._trigger_service_action("stop"),
            height=38,
            corner_radius=Radius.BUTTON,
            fg_color=Colors.DANGER,
            hover_color=Colors.DANGER_HOVER,
            text_color=Colors.TEXT_PRIMARY,
            font=TypographyManager.body_bold(),
        )
        self.btn_stop.pack(side="right", fill="x", expand=True, padx=(Spacing.XS, 0))

        self.service_feedback_label = ctk.CTkLabel(
            service_card,
            text="",
            font=TypographyManager.caption_bold(),
            text_color=Colors.TEXT_MUTED,
        )
        self.service_feedback_label.pack(anchor="w", padx=Spacing.MD, pady=(0, Spacing.MD))

        # Card 2: Antigravity Session & Dynamic Pointer Card
        memory_card = create_card(container)
        memory_card.pack(fill="x", pady=(0, Spacing.MD))

        mem_title = ctk.CTkLabel(
            memory_card,
            text="Memoria de Antigravity (Conversación Activa):",
            font=TypographyManager.card_title(),
            text_color=Colors.TEXT_PRIMARY,
        )
        mem_title.pack(anchor="w", padx=Spacing.MD, pady=(Spacing.MD, Spacing.XS))

        from viernes.brain.session_manager import ConversationSessionManager

        self.session_manager = ConversationSessionManager()
        active_id = self.session_manager.get_current_conversation_id()
        preview_id = active_id if active_id else "(Nueva / No inicializada aún)"

        self.conv_id_label = ctk.CTkLabel(
            memory_card,
            text=f"ID: {preview_id}",
            font=TypographyManager.mono(),
            text_color=Colors.TEXT_MUTED,
        )
        self.conv_id_label.pack(anchor="w", padx=Spacing.MD, pady=(0, Spacing.MD))

        self.btn_new_conv = ctk.CTkButton(
            memory_card,
            text="🔄 Iniciar Nueva Conversación (Rotar Memoria)",
            command=self._on_rotate_conversation_clicked,
            height=38,
            corner_radius=Radius.BUTTON,
            fg_color=Colors.NEUTRAL_BTN,
            hover_color=Colors.NEUTRAL_BTN_HOVER,
            text_color=Colors.NEUTRAL_TEXT,
            font=TypographyManager.body_bold(),
        )
        self.btn_new_conv.pack(fill="x", padx=Spacing.MD, pady=(0, Spacing.MD))

        self.conv_feedback_label = ctk.CTkLabel(
            memory_card,
            text="",
            font=TypographyManager.caption_bold(),
            text_color=Colors.TEXT_MUTED,
        )
        self.conv_feedback_label.pack(anchor="w", padx=Spacing.MD, pady=(0, Spacing.MD))

    def _on_rotate_conversation_clicked(self) -> None:
        """Rotates the active Antigravity conversation and updates UI."""
        archived = self.session_manager.rotate_conversation()
        self.conv_id_label.configure(text="ID: (Nueva / No inicializada aún)")
        if archived:
            self.conv_feedback_label.configure(
                text=f"Conversación archivada: {archived[:8]}... Lista para iniciar limpia.",
                text_color=Colors.SUCCESS_TEXT,
            )
        else:
            self.conv_feedback_label.configure(
                text="Lista para iniciar una conversación nueva y limpia.",
                text_color=Colors.SUCCESS_TEXT,
            )

    # -------------------------------------------------------------------------
    # Persistent Footer
    # -------------------------------------------------------------------------
    def _build_footer(self) -> None:
        """Builds bottom action bar with persistent Save button."""
        footer_card = create_card(
            self,
            fg_color=Colors.BG_HEADER,
            border_color=Colors.BORDER_DEFAULT,
            border_width=1,
            corner_radius=Radius.HEADER,
        )
        footer_card.pack(
            fill="x",
            padx=Spacing.MD,
            pady=(Spacing.SM, Spacing.MD),
        )

        self.save_btn = ctk.CTkButton(
            footer_card,
            text="✔ Guardar Cambios en .env",
            height=44,
            corner_radius=Radius.BUTTON,
            fg_color=Colors.PRIMARY,
            hover_color=Colors.PRIMARY_HOVER,
            text_color=Colors.TEXT_PRIMARY,
            font=TypographyManager.body_bold(),
            command=self._on_save_clicked,
        )
        self.save_btn.pack(fill="x", padx=Spacing.MD, pady=(Spacing.MD, Spacing.XS))

        self.footer_status = ctk.CTkLabel(
            footer_card,
            text=f"Archivo: {self.config_service.env_path}",
            font=TypographyManager.caption(),
            text_color=Colors.TEXT_MUTED,
        )
        self.footer_status.pack(anchor="w", padx=Spacing.MD, pady=(0, Spacing.MD))

    # -------------------------------------------------------------------------
    # State Synchronization & Callbacks
    # -------------------------------------------------------------------------
    def _load_config_values(self) -> None:
        """Populates widgets with values loaded from the configuration file."""
        cfg = self.current_config

        # 1. ElevenLabs API Key(s)
        pool = cfg.get("ELEVENLABS_API_KEYS", "")
        single = cfg.get("ELEVENLABS_API_KEY", "")
        key_val = pool if pool else single
        self.api_key_entry.delete(0, "end")
        self.api_key_entry.insert(0, key_val)

        # 2. Voice ID & Preset
        voice_id = cfg.get("ELEVENLABS_VOICE_ID", PRESET_VOICES["Jessica (Oficial)"])
        self.voice_id_entry.delete(0, "end")
        self.voice_id_entry.insert(0, voice_id)

        preset_found = False
        for preset_name, vid in PRESET_VOICES.items():
            if vid == voice_id:
                self.voice_combo.set(preset_name)
                preset_found = True
                break
        if not preset_found:
            self.voice_combo.set("Personalizada (Custom ID)")

        # 3. Audio input device
        input_dev = cfg.get("AUDIO_INPUT_DEVICE", "")
        for disp, raw_val in self.input_devices:
            if raw_val and (raw_val == input_dev or input_dev in disp):
                self.input_combo.set(disp)
                break
        else:
            self.input_combo.set("Por defecto del sistema (Recomendado)")

        # 4. Audio output device
        output_dev = cfg.get("AUDIO_OUTPUT_DEVICE", "")
        for disp, raw_val in self.output_devices:
            if raw_val and (raw_val == output_dev or output_dev in disp):
                self.output_combo.set(disp)
                break
        else:
            self.output_combo.set("Por defecto del sistema (Recomendado)")

        # 5. VAD Threshold
        try:
            vad_val = float(cfg.get("VAD_THRESHOLD", "0.010"))
        except ValueError:
            vad_val = 0.010
        self.vad_slider.set(vad_val)
        self._on_vad_slider_changed(vad_val)

        # 6. Silence timeout
        try:
            silence_val = float(cfg.get("SILENCE_TIMEOUT_MS", "800"))
        except ValueError:
            silence_val = 800.0
        self.silence_slider.set(silence_val)
        self._on_silence_slider_changed(silence_val)

    def _toggle_api_key_visibility(self) -> None:
        """Toggles masking on the API key entry."""
        if self.show_key_var.get():
            self.api_key_entry.configure(show="*")
            self.toggle_key_btn.configure(text="Ver")
            self.show_key_var.set(False)
        else:
            self.api_key_entry.configure(show="")
            self.toggle_key_btn.configure(text="Ocultar")
            self.show_key_var.set(True)

    def _on_voice_preset_selected(self, choice: str) -> None:
        """Fills Voice ID entry when a preset voice is picked."""
        if choice in PRESET_VOICES:
            vid = PRESET_VOICES[choice]
            self.voice_id_entry.delete(0, "end")
            self.voice_id_entry.insert(0, vid)

    def _on_vad_slider_changed(self, value: float) -> None:
        """Updates live VAD sensitivity label."""
        self.vad_val_label.configure(text=f"{value:.4f}")

    def _on_silence_slider_changed(self, value: float) -> None:
        """Updates live silence timeout label."""
        self.silence_val_label.configure(text=f"{int(value)} ms")

    def _on_save_clicked(self) -> None:
        """Collects GUI values and persists them to the .env file."""
        updates: dict[str, str] = {}

        # API Keys (Pool or single key)
        raw_keys = self.api_key_entry.get().strip()
        parsed_keys = [k.strip() for k in raw_keys.replace("\n", ",").split(",") if k.strip()]
        if len(parsed_keys) > 1:
            updates["ELEVENLABS_API_KEYS"] = ",".join(parsed_keys)
            updates["ELEVENLABS_API_KEY"] = parsed_keys[0]
        elif len(parsed_keys) == 1:
            updates["ELEVENLABS_API_KEYS"] = parsed_keys[0]
            updates["ELEVENLABS_API_KEY"] = parsed_keys[0]
        else:
            updates["ELEVENLABS_API_KEYS"] = ""
            updates["ELEVENLABS_API_KEY"] = ""

        # Voice ID
        vid = self.voice_id_entry.get().strip()
        if vid:
            updates["ELEVENLABS_VOICE_ID"] = vid

        # Input device
        selected_in = self.input_combo.get()
        in_id = ""
        if not selected_in.startswith("Por defecto"):
            for disp, raw_val in self.input_devices:
                if disp == selected_in:
                    in_id = raw_val
                    break
        updates["AUDIO_INPUT_DEVICE"] = in_id

        # Output device
        selected_out = self.output_combo.get()
        out_id = ""
        if not selected_out.startswith("Por defecto"):
            for disp, raw_val in self.output_devices:
                if disp == selected_out:
                    out_id = raw_val
                    break
        updates["AUDIO_OUTPUT_DEVICE"] = out_id

        # VAD & Silence
        updates["VAD_THRESHOLD"] = f"{self.vad_slider.get():.4f}"
        updates["SILENCE_TIMEOUT_MS"] = str(int(self.silence_slider.get()))

        try:
            self.config_service.save_config(updates)
            self.current_config = self.config_service.read_config()
            is_active, _ = self.config_service.get_service_status()
            if is_active:
                self.config_service.control_service("restart")
                self.footer_status.configure(
                    text="✔ Cambios guardados y servicio reiniciado exitosamente.",
                    text_color=Colors.SUCCESS_TEXT,
                )
            else:
                self.footer_status.configure(
                    text="✔ Cambios guardados exitosamente en .env",
                    text_color=Colors.SUCCESS_TEXT,
                )
            self._update_service_status_badge()
        except Exception as exc:
            logger.error("Failed saving configuration: %s", exc)
            self.footer_status.configure(
                text=f"✖ Error al guardar: {exc}",
                text_color=Colors.DANGER_TEXT,
            )

    def _on_test_voice_clicked(self) -> None:
        """Synthesizes and plays back audition audio asynchronously."""
        api_key = self.api_key_entry.get().strip()
        voice_id = self.voice_id_entry.get().strip()

        if not api_key:
            self.voice_test_status.configure(
                text="⚠ Ingresa una API Key válida primero.",
                text_color=Colors.WARNING_TEXT,
            )
            return

        self.voice_test_status.configure(
            text="Sintetizando y reproduciendo audición...",
            text_color=Colors.INFO_TEXT,
        )
        self.test_voice_btn.configure(state="disabled")

        def _worker() -> None:
            try:
                self.config_service.test_voice(api_key=api_key, voice_id=voice_id)
                self.after(
                    0,
                    lambda: self.voice_test_status.configure(
                        text="✔ Reproducción completada.",
                        text_color=Colors.SUCCESS_TEXT,
                    ),
                )
            except Exception as exc:
                err_msg = str(exc)
                self.after(
                    0,
                    lambda: self.voice_test_status.configure(
                        text=f"✖ Error: {err_msg}",
                        text_color=Colors.DANGER_TEXT,
                    ),
                )
            finally:
                self.after(0, lambda: self.test_voice_btn.configure(state="normal"))

        threading.Thread(target=_worker, daemon=True).start()

    def _update_service_status_badge(self) -> None:
        """Refreshes systemd service state indicator and pill badge."""
        is_active, status_desc = self.config_service.get_service_status()
        if is_active:
            self.status_pill.configure(
                fg_color=Colors.SUCCESS_BG,
                border_color=Colors.SUCCESS,
            )
            self.status_badge.configure(
                text="● Activo",
                text_color=Colors.SUCCESS_TEXT,
            )
            self.service_state_desc.configure(
                text=f"Estado: {status_desc.upper()}",
                text_color=Colors.SUCCESS_TEXT,
            )
        else:
            self.status_pill.configure(
                fg_color=Colors.BG_CARD_SUBTLE,
                border_color=Colors.BORDER_MUTED,
            )
            self.status_badge.configure(
                text="○ Inactivo",
                text_color=Colors.TEXT_MUTED,
            )
            self.service_state_desc.configure(
                text=f"Estado: {status_desc.upper()}",
                text_color=Colors.TEXT_MUTED,
            )

    def _trigger_service_action(self, action: str) -> None:
        """Executes a systemctl service command and updates display."""
        self.service_feedback_label.configure(
            text=f"Ejecutando {action}...",
            text_color=Colors.INFO_TEXT,
        )

        def _worker() -> None:
            success, msg = self.config_service.control_service(action)
            color = Colors.SUCCESS_TEXT if success else Colors.DANGER_TEXT

            def _update_ui() -> None:
                self.service_feedback_label.configure(text=msg, text_color=color)
                self._update_service_status_badge()

            self.after(0, _update_ui)

        threading.Thread(target=_worker, daemon=True).start()


def main() -> None:
    """Entry point for the Viernes GUI application."""
    app = ViernesControlCenter()
    app.mainloop()


if __name__ == "__main__":
    main()
