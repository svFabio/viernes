import logging
import threading
import tkinter as tk
from pathlib import Path
from typing import Any

import customtkinter as ctk

from viernes.gui.service import ConfigService, PRESET_VOICES

logger = logging.getLogger(__name__)


class ViernesControlCenter(ctk.CTk):
    """Modern Desktop Control Center for Viernes Voice Assistant."""

    def __init__(self, config_service: ConfigService | None = None) -> None:
        super().__init__()
        self.config_service = config_service or ConfigService()

        # Window settings
        self.title("Viernes — Centro de Control")
        self.geometry("680x760")
        self.minsize(620, 680)

        ctk.set_appearance_mode("Dark")
        ctk.set_default_color_theme("blue")

        # Load existing config
        self.current_config = self.config_service.read_config()

        # Build UI layout
        self._build_header()
        self._build_tabs()
        self._build_footer()

        # Populate initial values
        self._load_config_values()
        self._update_service_status_badge()

    def _build_header(self) -> None:
        """Constructs application top header and status indicator."""
        header_frame = ctk.CTkFrame(self, corner_radius=10, fg_color=("gray85", "gray17"))
        header_frame.pack(fill="x", padx=16, pady=(16, 8))

        title_label = ctk.CTkLabel(
            header_frame,
            text="VIERNES",
            font=ctk.CTkFont(size=24, weight="bold"),
            text_color="#38bdf8",
        )
        title_label.pack(anchor="w", padx=16, pady=(10, 0))

        subtitle_label = ctk.CTkLabel(
            header_frame,
            text="SISTEMA DE ASISTENCIA POR VOZ // CENTRO DE CONTROL",
            font=ctk.CTkFont(size=11, weight="bold"),
            text_color="gray60",
        )
        subtitle_label.pack(anchor="w", padx=16, pady=(0, 8))

        # Status indicator
        self.status_badge = ctk.CTkLabel(
            header_frame,
            text="○ Comprobando servicio...",
            font=ctk.CTkFont(size=12, weight="bold"),
            text_color="gray60",
        )
        self.status_badge.pack(anchor="e", padx=16, pady=(0, 10))

    def _build_tabs(self) -> None:
        """Constructs tab navigation."""
        self.tabview = ctk.CTkTabview(self, corner_radius=10)
        self.tabview.pack(fill="both", expand=True, padx=16, pady=8)

        self.tab_voice = self.tabview.add("Credenciales & Voz")
        self.tab_audio = self.tabview.add("Dispositivos de Audio")
        self.tab_fillers = self.tabview.add("Frases de Relleno")
        self.tab_service = self.tabview.add("Servicio & Estado")

        self._build_voice_tab()
        self._build_audio_tab()
        self._build_fillers_tab()
        self._build_service_tab()

    def _build_voice_tab(self) -> None:
        """Builds credentials and TTS voice selection tab."""
        container = ctk.CTkScrollableFrame(self.tab_voice, fg_color="transparent")
        container.pack(fill="both", expand=True, padx=8, pady=8)

        # ElevenLabs API Key
        key_label = ctk.CTkLabel(
            container,
            text="ElevenLabs API Key(s) [Pool Multicuenta]:",
            font=ctk.CTkFont(size=13, weight="bold"),
        )
        key_label.pack(anchor="w", pady=(8, 4))

        key_frame = ctk.CTkFrame(container, fg_color="transparent")
        key_frame.pack(fill="x", pady=(0, 12))

        self.api_key_entry = ctk.CTkEntry(
            key_frame,
            placeholder_text="sk_..., sk_... (separadas por coma para rotación automática)",
            show="*",
            height=36,
        )
        self.api_key_entry.pack(side="left", fill="x", expand=True, padx=(0, 8))

        self.show_key_var = tk.BooleanVar(value=False)
        self.toggle_key_btn = ctk.CTkButton(
            key_frame,
            text="Ver",
            width=60,
            height=36,
            command=self._toggle_api_key_visibility,
        )
        self.toggle_key_btn.pack(side="right")

        # Preset Voices
        voice_label = ctk.CTkLabel(
            container,
            text="Voz Seleccionada:",
            font=ctk.CTkFont(size=13, weight="bold"),
        )
        voice_label.pack(anchor="w", pady=(8, 4))

        voice_options = list(PRESET_VOICES.keys()) + ["Personalizada (Custom ID)"]
        self.voice_combo = ctk.CTkOptionMenu(
            container,
            values=voice_options,
            command=self._on_voice_preset_selected,
            height=36,
        )
        self.voice_combo.pack(fill="x", pady=(0, 8))

        # Custom Voice ID entry
        id_label = ctk.CTkLabel(
            container,
            text="Voice ID de ElevenLabs:",
            font=ctk.CTkFont(size=12),
            text_color="gray70",
        )
        id_label.pack(anchor="w", pady=(4, 2))

        self.voice_id_entry = ctk.CTkEntry(
            container,
            placeholder_text="ID de la voz (ej: cgSgspJ2msm6clMCkdW9)",
            height=36,
        )
        self.voice_id_entry.pack(fill="x", pady=(0, 16))

        # Audition Test Button & Status
        self.test_voice_btn = ctk.CTkButton(
            container,
            text="▶ Probar Voz (Audición en Vivo)",
            height=40,
            fg_color="#0284c7",
            hover_color="#0369a1",
            font=ctk.CTkFont(size=13, weight="bold"),
            command=self._on_test_voice_clicked,
        )
        self.test_voice_btn.pack(fill="x", pady=(8, 4))

        self.voice_test_status = ctk.CTkLabel(
            container,
            text="",
            font=ctk.CTkFont(size=12),
            text_color="gray70",
        )
        self.voice_test_status.pack(anchor="w", pady=(0, 8))

    def _build_audio_tab(self) -> None:
        """Builds audio input/output and VAD sensitivity tab."""
        container = ctk.CTkScrollableFrame(self.tab_audio, fg_color="transparent")
        container.pack(fill="both", expand=True, padx=8, pady=8)

        # Microphones
        input_label = ctk.CTkLabel(
            container,
            text="Micrófono de Entrada:",
            font=ctk.CTkFont(size=13, weight="bold"),
        )
        input_label.pack(anchor="w", pady=(8, 4))

        self.input_devices = self.config_service.get_audio_input_devices()
        input_displays = [disp for disp, _ in self.input_devices]
        self.input_combo = ctk.CTkOptionMenu(
            container,
            values=input_displays or ["Por defecto del sistema"],
            height=36,
        )
        self.input_combo.pack(fill="x", pady=(0, 14))

        # Speakers
        output_label = ctk.CTkLabel(
            container,
            text="Dispositivo de Salida (Altavoces):",
            font=ctk.CTkFont(size=13, weight="bold"),
        )
        output_label.pack(anchor="w", pady=(8, 4))

        self.output_devices = self.config_service.get_audio_output_devices()
        output_displays = [disp for disp, _ in self.output_devices]
        self.output_combo = ctk.CTkOptionMenu(
            container,
            values=output_displays or ["Por defecto del sistema"],
            height=36,
        )
        self.output_combo.pack(fill="x", pady=(0, 16))

        # VAD Energy Threshold Slider
        self.vad_label = ctk.CTkLabel(
            container,
            text="Sensibilidad de Detección VAD (Umbral de Energía):",
            font=ctk.CTkFont(size=13, weight="bold"),
        )
        self.vad_label.pack(anchor="w", pady=(8, 4))

        self.vad_slider = ctk.CTkSlider(
            container,
            from_=0.002,
            to=0.05,
            number_of_steps=48,
            command=self._on_vad_slider_changed,
        )
        self.vad_slider.pack(fill="x", pady=(0, 2))

        self.vad_val_label = ctk.CTkLabel(
            container,
            text="0.010 (Recomendado)",
            font=ctk.CTkFont(size=12),
            text_color="gray70",
        )
        self.vad_val_label.pack(anchor="w", pady=(0, 14))

        # Silence Cutout Slider
        self.silence_label = ctk.CTkLabel(
            container,
            text="Tiempo de Silencio para Corte (ms):",
            font=ctk.CTkFont(size=13, weight="bold"),
        )
        self.silence_label.pack(anchor="w", pady=(8, 4))

        self.silence_slider = ctk.CTkSlider(
            container,
            from_=300,
            to=2000,
            number_of_steps=17,
            command=self._on_silence_slider_changed,
        )
        self.silence_slider.pack(fill="x", pady=(0, 2))

        self.silence_val_label = ctk.CTkLabel(
            container,
            text="800 ms",
            font=ctk.CTkFont(size=12),
            text_color="gray70",
        )
        self.silence_val_label.pack(anchor="w", pady=(0, 8))

    def _build_fillers_tab(self) -> None:
        """Builds conversational filler phrase configuration tab."""
        container = ctk.CTkScrollableFrame(self.tab_fillers, fg_color="transparent")
        container.pack(fill="both", expand=True, padx=8, pady=8)

        intro_label = ctk.CTkLabel(
            container,
            text="Personalización de Frases de Relleno (Conversational Fillers):",
            font=ctk.CTkFont(size=14, weight="bold"),
        )
        intro_label.pack(anchor="w", pady=(4, 2))

        desc_label = ctk.CTkLabel(
            container,
            text=(
                "Estas frases son dichas por Viernes de inmediato para enmascarar la latencia de red "
                "mientras Antigravity procesa o ejecuta herramientas. Modifíquelas (una frase por línea)."
            ),
            font=ctk.CTkFont(size=11),
            text_color="gray70",
            wraplength=700,
            justify="left",
        )
        desc_label.pack(anchor="w", pady=(0, 12))

        from viernes.audio.fillers import load_custom_fillers

        generic, tools, patience = load_custom_fillers()

        # 1. Generic fillers
        gen_label = ctk.CTkLabel(
            container,
            text="Frases Inmediatas / Genéricas (al detectar voz, < 200ms):",
            font=ctk.CTkFont(size=12, weight="bold"),
        )
        gen_label.pack(anchor="w", pady=(4, 2))
        self.fillers_generic_txt = ctk.CTkTextbox(container, height=90)
        self.fillers_generic_txt.pack(fill="x", pady=(0, 10))
        self.fillers_generic_txt.insert("1.0", "\n".join(generic))

        # 2. Patience fillers
        pat_label = ctk.CTkLabel(
            container,
            text="Frases de Paciencia (si la respuesta tarda más de 6.5s):",
            font=ctk.CTkFont(size=12, weight="bold"),
        )
        pat_label.pack(anchor="w", pady=(4, 2))
        self.fillers_patience_txt = ctk.CTkTextbox(container, height=60)
        self.fillers_patience_txt.pack(fill="x", pady=(0, 10))
        self.fillers_patience_txt.insert("1.0", "\n".join(patience))

        # 3. Tool fillers - Files
        files_label = ctk.CTkLabel(
            container,
            text="Herramientas: Lectura y Búsqueda de Archivos:",
            font=ctk.CTkFont(size=12, weight="bold"),
        )
        files_label.pack(anchor="w", pady=(4, 2))
        self.fillers_files_txt = ctk.CTkTextbox(container, height=55)
        self.fillers_files_txt.pack(fill="x", pady=(0, 10))
        self.fillers_files_txt.insert("1.0", "\n".join(tools.get("files", [])))

        # 4. Tool fillers - Terminal
        term_label = ctk.CTkLabel(
            container,
            text="Herramientas: Ejecución en Terminal:",
            font=ctk.CTkFont(size=12, weight="bold"),
        )
        term_label.pack(anchor="w", pady=(4, 2))
        self.fillers_term_txt = ctk.CTkTextbox(container, height=55)
        self.fillers_term_txt.pack(fill="x", pady=(0, 10))
        self.fillers_term_txt.insert("1.0", "\n".join(tools.get("terminal", [])))

        # 5. Tool fillers - Search
        search_label = ctk.CTkLabel(
            container,
            text="Herramientas: Búsqueda en la Web:",
            font=ctk.CTkFont(size=12, weight="bold"),
        )
        search_label.pack(anchor="w", pady=(4, 2))
        self.fillers_search_txt = ctk.CTkTextbox(container, height=45)
        self.fillers_search_txt.pack(fill="x", pady=(0, 12))
        self.fillers_search_txt.insert("1.0", "\n".join(tools.get("search", [])))

        # Buttons
        btn_row = ctk.CTkFrame(container, fg_color="transparent")
        btn_row.pack(fill="x", pady=(4, 8))

        self.btn_save_fillers = ctk.CTkButton(
            btn_row,
            text="✔ Guardar Frases de Relleno",
            height=36,
            fg_color="#16a34a",
            hover_color="#15803d",
            font=ctk.CTkFont(size=13, weight="bold"),
            command=self._on_save_fillers_clicked,
        )
        self.btn_save_fillers.pack(side="left", fill="x", expand=True, padx=(0, 6))

        self.btn_reset_fillers = ctk.CTkButton(
            btn_row,
            text="↺ Restablecer Predeterminadas",
            height=36,
            fg_color="#475569",
            hover_color="#334155",
            command=self._on_reset_fillers_clicked,
        )
        self.btn_reset_fillers.pack(side="right", padx=(6, 0))

        self.fillers_status_label = ctk.CTkLabel(
            container,
            text="",
            font=ctk.CTkFont(size=12),
            text_color="gray70",
        )
        self.fillers_status_label.pack(anchor="w", pady=(2, 6))

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
            self.fillers_status_label.configure(
                text="✔ Frases de relleno guardadas exitosamente.",
                text_color="#22c55e",
            )
        except Exception as exc:
            self.fillers_status_label.configure(
                text=f"Error al guardar: {exc}",
                text_color="#ef4444",
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
            text_color="#38bdf8",
        )

    def _build_service_tab(self) -> None:
        """Builds background service and system management tab."""
        container = ctk.CTkScrollableFrame(self.tab_service, fg_color="transparent")
        container.pack(fill="both", expand=True, padx=8, pady=8)

        status_card = ctk.CTkFrame(container, fg_color=("gray80", "gray20"), corner_radius=8)
        status_card.pack(fill="x", pady=(8, 16), padx=4)

        self.service_info_label = ctk.CTkLabel(
            status_card,
            text="Servicio Systemd: viernes.service",
            font=ctk.CTkFont(size=13, weight="bold"),
        )
        self.service_info_label.pack(anchor="w", padx=12, pady=(12, 4))

        self.service_state_desc = ctk.CTkLabel(
            status_card,
            text="Estado: Consultando...",
            font=ctk.CTkFont(size=12),
            text_color="gray70",
        )
        self.service_state_desc.pack(anchor="w", padx=12, pady=(0, 12))

        # Service control buttons
        btn_frame = ctk.CTkFrame(container, fg_color="transparent")
        btn_frame.pack(fill="x", pady=(0, 12))

        self.btn_start = ctk.CTkButton(
            btn_frame,
            text="Iniciar Servicio",
            command=lambda: self._trigger_service_action("start"),
            height=36,
        )
        self.btn_start.pack(side="left", fill="x", expand=True, padx=(0, 4))

        self.btn_restart = ctk.CTkButton(
            btn_frame,
            text="Reiniciar Servicio",
            command=lambda: self._trigger_service_action("restart"),
            height=36,
        )
        self.btn_restart.pack(side="left", fill="x", expand=True, padx=4)

        self.btn_stop = ctk.CTkButton(
            btn_frame,
            text="Detener Servicio",
            command=lambda: self._trigger_service_action("stop"),
            fg_color="#dc2626",
            hover_color="#b91c1c",
            height=36,
        )
        self.btn_stop.pack(side="right", fill="x", expand=True, padx=(4, 0))

        self.service_feedback_label = ctk.CTkLabel(
            container,
            text="",
            font=ctk.CTkFont(size=12),
            text_color="gray70",
        )
        self.service_feedback_label.pack(anchor="w", pady=(4, 8))

        # Brain Conversation & Memory Management Card
        memory_card = ctk.CTkFrame(container, fg_color=("gray80", "gray20"), corner_radius=8)
        memory_card.pack(fill="x", pady=(12, 16), padx=4)

        mem_title = ctk.CTkLabel(
            memory_card,
            text="Memoria de Antigravity (Conversación Activa):",
            font=ctk.CTkFont(size=13, weight="bold"),
        )
        mem_title.pack(anchor="w", padx=12, pady=(12, 4))

        from viernes.brain.session_manager import ConversationSessionManager

        self.session_manager = ConversationSessionManager()
        active_id = self.session_manager.get_current_conversation_id()
        preview_id = active_id if active_id else "(Nueva / No inicializada aún)"

        self.conv_id_label = ctk.CTkLabel(
            memory_card,
            text=f"ID: {preview_id}",
            font=ctk.CTkFont(size=11),
            text_color="gray70",
        )
        self.conv_id_label.pack(anchor="w", padx=12, pady=(0, 8))

        self.btn_new_conv = ctk.CTkButton(
            memory_card,
            text="🔄 Iniciar Nueva Conversación (Rotar Memoria)",
            command=self._on_rotate_conversation_clicked,
            fg_color="#475569",
            hover_color="#334155",
            height=36,
        )
        self.btn_new_conv.pack(fill="x", padx=12, pady=(0, 12))

        self.conv_feedback_label = ctk.CTkLabel(
            container,
            text="",
            font=ctk.CTkFont(size=12),
            text_color="gray70",
        )
        self.conv_feedback_label.pack(anchor="w", pady=(0, 8))

    def _on_rotate_conversation_clicked(self) -> None:
        """Rotates the active Antigravity conversation and updates UI."""
        archived = self.session_manager.rotate_conversation()
        self.conv_id_label.configure(text="ID: (Nueva / No inicializada aún)")
        if archived:
            self.conv_feedback_label.configure(
                text=f"Conversación archivada: {archived[:8]}... Lista para iniciar limpia.",
                text_color="#22c55e",
            )
        else:
            self.conv_feedback_label.configure(
                text="Lista para iniciar una conversación nueva y limpia.",
                text_color="#22c55e",
            )

    def _build_footer(self) -> None:
        """Builds bottom action bar with persistent Save button."""
        footer_frame = ctk.CTkFrame(self, corner_radius=10, fg_color=("gray85", "gray17"))
        footer_frame.pack(fill="x", padx=16, pady=(8, 16))

        self.save_btn = ctk.CTkButton(
            footer_frame,
            text="✔ Guardar Cambios en .env",
            height=44,
            fg_color="#16a34a",
            hover_color="#15803d",
            font=ctk.CTkFont(size=14, weight="bold"),
            command=self._on_save_clicked,
        )
        self.save_btn.pack(fill="x", padx=16, pady=(12, 6))

        self.footer_status = ctk.CTkLabel(
            footer_frame,
            text=f"Archivo: {self.config_service.env_path}",
            font=ctk.CTkFont(size=11),
            text_color="gray60",
        )
        self.footer_status.pack(anchor="w", padx=16, pady=(0, 10))

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
            self.input_combo.set("Por defecto del sistema")

        # 4. Audio output device
        output_dev = cfg.get("AUDIO_OUTPUT_DEVICE", "")
        for disp, raw_val in self.output_devices:
            if raw_val and (raw_val == output_dev or output_dev in disp):
                self.output_combo.set(disp)
                break
        else:
            self.output_combo.set("Por defecto del sistema")

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
        for disp, raw_val in self.input_devices:
            if disp == selected_in:
                in_id = raw_val
                break
        updates["AUDIO_INPUT_DEVICE"] = in_id

        # Output device
        selected_out = self.output_combo.get()
        out_id = ""
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
            self.footer_status.configure(
                text="✔ Cambios guardados exitosamente en .env",
                text_color="#4ade80",
            )
            # Re-read status
            self._update_service_status_badge()
        except Exception as exc:
            logger.error("Failed saving configuration: %s", exc)
            self.footer_status.configure(
                text=f"✖ Error al guardar: {exc}",
                text_color="#f87171",
            )

    def _on_test_voice_clicked(self) -> None:
        """Synthesizes and plays back audition audio asynchronously."""
        api_key = self.api_key_entry.get().strip()
        voice_id = self.voice_id_entry.get().strip()

        if not api_key:
            self.voice_test_status.configure(
                text="⚠ Ingresa una API Key válida primero.",
                text_color="#f87171",
            )
            return

        self.voice_test_status.configure(
            text="Sintetizando y reproduciendo con Jessica...",
            text_color="#38bdf8",
        )
        self.test_voice_btn.configure(state="disabled")

        def _worker() -> None:
            try:
                self.config_service.test_voice(api_key=api_key, voice_id=voice_id)
                self.after(
                    0,
                    lambda: self.voice_test_status.configure(
                        text="✔ Reproducción completada.",
                        text_color="#4ade80",
                    ),
                )
            except Exception as exc:
                err_msg = str(exc)
                self.after(
                    0,
                    lambda: self.voice_test_status.configure(
                        text=f"✖ Error: {err_msg}",
                        text_color="#f87171",
                    ),
                )
            finally:
                self.after(0, lambda: self.test_voice_btn.configure(state="normal"))

        threading.Thread(target=_worker, daemon=True).start()

    def _update_service_status_badge(self) -> None:
        """Refreshes systemd service state indicator."""
        is_active, status_desc = self.config_service.get_service_status()
        if is_active:
            self.status_badge.configure(text="● Servicio Activo", text_color="#4ade80")
            self.service_state_desc.configure(
                text=f"Estado: {status_desc.upper()}", text_color="#4ade80"
            )
        else:
            self.status_badge.configure(text="○ Servicio Inactivo", text_color="gray60")
            self.service_state_desc.configure(
                text=f"Estado: {status_desc.upper()}", text_color="gray60"
            )

    def _trigger_service_action(self, action: str) -> None:
        """Executes a systemctl service command and updates display."""
        self.service_feedback_label.configure(text=f"Ejecutando {action}...", text_color="#38bdf8")

        def _worker() -> None:
            success, msg = self.config_service.control_service(action)
            color = "#4ade80" if success else "#f87171"

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
