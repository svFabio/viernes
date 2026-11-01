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

        self._is_closing = False
        self.protocol("WM_DELETE_WINDOW", self._on_close)

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
        self.tabview = ctk.CTkTabview(self, corner_radius=10, command=self._on_tab_changed)
        self.tabview.pack(fill="both", expand=True, padx=16, pady=8)

        self.tab_voice = self.tabview.add("Credenciales & Voz")
        self.tab_audio = self.tabview.add("Hardware de Audio")
        self.tab_service = self.tabview.add("Servicio & Estado")

        self._build_voice_tab()
        self._build_audio_tab()
        self._build_service_tab()

    def _on_tab_changed(self) -> None:
        """Invoked when user switches tabs."""
        current_tab = self.tabview.get()
        if current_tab == "Hardware de Audio":
            self.refresh_audio_devices()
        elif current_tab == "Servicio & Estado":
            self._update_service_status_badge()

    def refresh_audio_devices(self) -> None:
        """Dynamically re-enumerates audio devices and updates option menus."""
        self.input_devices = self.config_service.get_audio_input_devices()
        input_displays = [disp for disp, _ in self.input_devices]
        current_in = self.input_combo.get()
        self.input_combo.configure(values=input_displays or ["Por defecto del sistema"])
        if current_in in input_displays:
            self.input_combo.set(current_in)
        else:
            input_dev = self.current_config.get("AUDIO_INPUT_DEVICE", "")
            for disp, raw_val in self.input_devices:
                if raw_val and (raw_val == input_dev or input_dev in disp):
                    self.input_combo.set(disp)
                    break
            else:
                self.input_combo.set("Por defecto del sistema")

        self.output_devices = self.config_service.get_audio_output_devices()
        output_displays = [disp for disp, _ in self.output_devices]
        current_out = self.output_combo.get()
        self.output_combo.configure(values=output_displays or ["Por defecto del sistema"])
        if current_out in output_displays:
            self.output_combo.set(current_out)
        else:
            output_dev = self.current_config.get("AUDIO_OUTPUT_DEVICE", "")
            for disp, raw_val in self.output_devices:
                if raw_val and (raw_val == output_dev or output_dev in disp):
                    self.output_combo.set(disp)
                    break
            else:
                self.output_combo.set("Por defecto del sistema")

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
        rounded = int(round(value / 100.0) * 100)
        self.silence_val_label.configure(text=f"{rounded} ms")

    def _on_save_clicked(self) -> None:
        """Collects GUI values and persists them to the .env file."""
        updates: dict[str, str] = {}

        # API Key
        key = self.api_key_entry.get().strip()
        updates["ELEVENLABS_API_KEY"] = key

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
        updates["SILENCE_TIMEOUT_MS"] = str(int(round(self.silence_slider.get() / 100.0) * 100))

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
                text="⚠ Ingresa una API Key válida primero",
                text_color="#f87171",
            )
            return

        choice = self.voice_combo.get()
        if choice and "Personalizada" not in choice and "Custom" not in choice:
            voice_name = choice.split(" (")[0]
        else:
            voice_name = "Jessica"

        self.voice_test_status.configure(
            text=f"Sintetizando y reproduciendo con {voice_name}...",
            text_color="#38bdf8",
        )
        self.test_voice_btn.configure(state="disabled")

        selected_out = self.output_combo.get()
        out_device = ""
        for disp, raw_val in self.output_devices:
            if disp == selected_out:
                out_device = raw_val
                break

        def _worker() -> None:
            try:
                self.config_service.test_voice(
                    api_key=api_key,
                    voice_id=voice_id,
                    device=out_device,
                )
                self._safe_after(
                    0,
                    lambda: self.voice_test_status.configure(
                        text="✔ Reproducción completada.",
                        text_color="#4ade80",
                    ),
                )
            except Exception as exc:
                logger.warning("Voice audition failed: %s", exc)
                err_msg = self.format_audition_error(exc)

                self._safe_after(
                    0,
                    lambda msg=err_msg: self.voice_test_status.configure(
                        text=msg,
                        text_color="#f87171",
                    ),
                )
            finally:
                self._safe_after(
                    0,
                    lambda: self.test_voice_btn.configure(state="normal"),
                )

        threading.Thread(target=_worker, daemon=True).start()

    @staticmethod
    def format_audition_error(exc: Exception) -> str:
        """Translates backend audition exceptions into user-friendly localized messages."""
        raw_err = str(exc)
        err_lower = raw_err.lower()
        if "401" in raw_err or "unauthorized" in err_lower:
            return "⚠ Error 401: API Key no autorizada o inválida."
        if "402" in raw_err or "429" in raw_err or "quota" in err_lower or "rate limit" in err_lower:
            return "⚠ Error 429: Cuota de ElevenLabs agotada o rate limit."
        if (
            isinstance(exc, (TimeoutError, ConnectionError))
            or "timeout" in err_lower
            or "timed out" in err_lower
            or "connect" in err_lower
        ):
            return "⚠ Error de red: Tiempo de espera agotado o sin conexión."
        return f"✖ Error: {raw_err}"

    def _safe_after(self, ms: int, func: Any) -> None:
        """Schedules callback safely if window is still active."""
        if getattr(self, "_is_closing", False):
            return
        try:
            self.after(ms, func)
        except (tk.TclError, RuntimeError):
            pass

    def _on_close(self) -> None:
        """Terminates active audio playback and closes GUI cleanly."""
        self._is_closing = True
        try:
            self.config_service.stop_playback()
        except Exception as exc:
            logger.debug("Error stopping playback on close: %s", exc)
        self.destroy()

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
        self.service_feedback_label.configure(
            text=f"Ejecutando {action}...", text_color="#38bdf8"
        )

        def _worker() -> None:
            success, msg = self.config_service.control_service(action)
            color = "#4ade80" if success else "#f87171"

            def _update_ui() -> None:
                self.service_feedback_label.configure(text=msg, text_color=color)
                self._update_service_status_badge()

            self._safe_after(0, _update_ui)

        threading.Thread(target=_worker, daemon=True).start()


def main() -> None:
    """Entry point for the Viernes GUI application."""
    app = ViernesControlCenter()
    app.mainloop()


if __name__ == "__main__":
    main()
