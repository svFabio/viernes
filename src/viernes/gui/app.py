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
        self._is_destroyed = False

        # Build UI layout
        self._build_header()
        self._build_tabs()
        self._build_footer()

        # Handle window close protocol safely
        self.protocol("WM_DELETE_WINDOW", self.destroy)

        # Populate initial values
        self._load_config_values()
        self._update_service_status_badge()

    def safe_after(self, ms: int, func: Any, *args: Any) -> None:
        """Schedules a callback safely, suppressing execution if window is destroyed."""
        if getattr(self, "_is_destroyed", False):
            return
        try:
            if self.winfo_exists():
                self.after(ms, func, *args)
        except (tk.TclError, RuntimeError):
            pass

    def destroy(self) -> None:
        """Cleans up resources and marks window as destroyed."""
        self._is_destroyed = True
        super().destroy()

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
        self.tab_service = self.tabview.add("Servicio & Estado")

        self._build_voice_tab()
        self._build_audio_tab()
        self._build_service_tab()

    def _build_voice_tab(self) -> None:
        """Builds credentials and TTS voice selection tab."""
        container = ctk.CTkScrollableFrame(self.tab_voice, fg_color="transparent")
        container.pack(fill="both", expand=True, padx=8, pady=8)

        # ElevenLabs API Key
        key_label = ctk.CTkLabel(
            container,
            text="ElevenLabs API Key:",
            font=ctk.CTkFont(size=13, weight="bold"),
        )
        key_label.pack(anchor="w", pady=(8, 4))

        key_frame = ctk.CTkFrame(container, fg_color="transparent")
        key_frame.pack(fill="x", pady=(0, 12))

        self.api_key_entry = ctk.CTkEntry(
            key_frame,
            placeholder_text="sk_...",
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
        self.voice_id_entry.bind("<KeyRelease>", self._on_voice_id_entry_changed)

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

        # 1. ElevenLabs API Key
        key = cfg.get("ELEVENLABS_API_KEY", "")
        self.api_key_entry.delete(0, "end")
        self.api_key_entry.insert(0, key)

        # 2. Voice ID & Preset (Defaulting to Jessica)
        voice_id = cfg.get("ELEVENLABS_VOICE_ID", "").strip() or PRESET_VOICES["Jessica (Oficial)"]
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
        self.voice_combo.set(choice)
        if choice in PRESET_VOICES:
            vid = PRESET_VOICES[choice]
            self.voice_id_entry.delete(0, "end")
            self.voice_id_entry.insert(0, vid)
        elif choice == "Personalizada (Custom ID)":
            self.voice_id_entry.focus_set()

    def _on_voice_id_entry_changed(self, event: Any = None) -> None:
        """Dynamically matches entered Voice ID to presets or sets to Custom."""
        current_id = self.voice_id_entry.get().strip()
        for preset_name, vid in PRESET_VOICES.items():
            if vid == current_id:
                self.voice_combo.set(preset_name)
                return
        self.voice_combo.set("Personalizada (Custom ID)")

    def _on_vad_slider_changed(self, value: float) -> None:
        """Updates live VAD sensitivity label."""
        self.vad_val_label.configure(text=f"{value:.4f}")

    def _on_silence_slider_changed(self, value: float) -> None:
        """Updates live silence timeout label."""
        self.silence_val_label.configure(text=f"{int(value)} ms")

    def _on_save_clicked(self) -> None:
        """Collects GUI values, validates credentials, persists to .env, and triggers service reload if active."""
        # 1. Validate & sanitize API Key
        raw_key = self.api_key_entry.get()
        _, clean_key, key_warning = self.config_service.validate_api_key(raw_key)

        # 2. Validate & sanitize Voice ID
        raw_vid = self.voice_id_entry.get()
        _, clean_vid, vid_warning = self.config_service.validate_voice_id(raw_vid)

        # Reflect sanitized values
        self.api_key_entry.delete(0, "end")
        self.api_key_entry.insert(0, clean_key)
        self.voice_id_entry.delete(0, "end")
        self.voice_id_entry.insert(0, clean_vid)

        updates: dict[str, str] = {
            "ELEVENLABS_API_KEY": clean_key,
            "ELEVENLABS_VOICE_ID": clean_vid,
        }

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

            warning_note = f" ({key_warning})" if key_warning else ""

            # SUB-20.4: Check if viernes.service is active and trigger reload/restart
            is_active, _ = self.config_service.get_service_status()
            if is_active:
                self.footer_status.configure(
                    text="✔ Cambios guardados en .env. Recargando servicio...",
                    text_color="#38bdf8",
                )

                def _reload_worker() -> None:
                    reloaded, r_msg = self.config_service.control_service("try-restart")
                    if not reloaded:
                        reloaded, r_msg = self.config_service.control_service("reload-or-restart")

                    def _after_reload() -> None:
                        if reloaded:
                            self.footer_status.configure(
                                text=f"✔ Cambios guardados exitosamente en .env (Servicio reiniciado){warning_note}",
                                text_color="#4ade80",
                            )
                        else:
                            self.footer_status.configure(
                                text=f"✔ Guardado en .env (Aviso: {r_msg}){warning_note}",
                                text_color="#fbbf24",
                            )
                        self._update_service_status_badge()

                    self.safe_after(0, _after_reload)

                threading.Thread(target=_reload_worker, daemon=True).start()
            else:
                self.footer_status.configure(
                    text=f"✔ Cambios guardados exitosamente en .env{warning_note}",
                    text_color="#4ade80" if not key_warning else "#fbbf24",
                )
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
                self.safe_after(
                    0,
                    lambda: self.voice_test_status.configure(
                        text="✔ Reproducción completada.",
                        text_color="#4ade80",
                    ),
                )
            except Exception as exc:
                err_msg = str(exc)
                self.safe_after(
                    0,
                    lambda: self.voice_test_status.configure(
                        text=f"✖ Error: {err_msg}",
                        text_color="#f87171",
                    ),
                )
            finally:
                self.safe_after(0, lambda: self.test_voice_btn.configure(state="normal"))

        threading.Thread(target=_worker, daemon=True).start()

    def _apply_service_status(self, is_active: bool, status_desc: str) -> None:
        """Applies status text and semantic colors to header badge and service tab."""
        stat_lower = status_desc.lower()
        if is_active:
            self.status_badge.configure(text="● Servicio Activo", text_color="#4ade80")
            self.service_state_desc.configure(
                text=f"Estado: {status_desc.upper()} (En ejecución)", text_color="#4ade80"
            )
        elif stat_lower == "failed":
            self.status_badge.configure(text="● Servicio Fallido", text_color="#f87171")
            self.service_state_desc.configure(
                text=f"Estado: {status_desc.upper()} (Fallido)", text_color="#f87171"
            )
        elif stat_lower == "no disponible":
            self.status_badge.configure(text="○ Servicio No Disponible", text_color="gray60")
            self.service_state_desc.configure(
                text="Estado: NO DISPONIBLE (systemctl no detectado)", text_color="gray60"
            )
        else:
            self.status_badge.configure(text="○ Servicio Inactivo", text_color="gray60")
            self.service_state_desc.configure(
                text=f"Estado: {status_desc.upper()} (Detenido)", text_color="gray60"
            )

    def _update_service_status_badge(
        self,
        async_call: bool = True,
        callback: Any | None = None,
    ) -> None:
        """Refreshes systemd service state indicator non-blockingly."""
        if not async_call:
            is_active, status_desc = self.config_service.get_service_status()
            self._apply_service_status(is_active, status_desc)
            if callback:
                callback(is_active, status_desc)
            return

        def _worker() -> None:
            is_active, status_desc = self.config_service.get_service_status()

            def _apply() -> None:
                self._apply_service_status(is_active, status_desc)
                if callback:
                    callback(is_active, status_desc)

            self.safe_after(0, _apply)

        threading.Thread(target=_worker, daemon=True).start()

    def _trigger_service_action(self, action: str) -> None:
        """Executes a systemctl service command asynchronously and updates display."""
        self.btn_start.configure(state="disabled")
        self.btn_restart.configure(state="disabled")
        self.btn_stop.configure(state="disabled")

        self.service_feedback_label.configure(
            text=f"Ejecutando {action}...", text_color="#38bdf8"
        )

        def _worker() -> None:
            success, msg = self.config_service.control_service(action)
            color = "#4ade80" if success else "#f87171"

            def _update_ui() -> None:
                self.service_feedback_label.configure(text=msg, text_color=color)
                self.btn_start.configure(state="normal")
                self.btn_restart.configure(state="normal")
                self.btn_stop.configure(state="normal")
                self._update_service_status_badge()

            self.safe_after(0, _update_ui)

        threading.Thread(target=_worker, daemon=True).start()


def main() -> None:
    """Entry point for the Viernes GUI application."""
    app = ViernesControlCenter()
    app.mainloop()


if __name__ == "__main__":
    main()
