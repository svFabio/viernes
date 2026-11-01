# HU-20: Auditoría y resolución de defectos de concurrencia, notificaciones apiladas y recarga de configuración

**Puntos de Historia:** 5 | **Prioridad:** Highest | **Versión:** 1 | **Asignado a:** Agente QA & Core Architecture

---

### Descripción
El usuario opera el asistente y el Centro de Control sin experimentar apilamiento masivo de notificaciones visuales en el escritorio, sin procesos de reproducción de audio colgados al cerrar la aplicación y con recarga automática de las configuraciones modificadas para garantizar la estabilidad absoluta del sistema.

---

### Mockup / Flujo de Interacción
```text
Antes del fix (Defecto reportado por el usuario):
Desktop Screen (Hyprland / Mako):
  ┌───────────────────────────────┐
  │ 🎙  Viernes - Escuchando...   │
  └───────────────────────────────┘
  ┌───────────────────────────────┐
  │ 🎙  Viernes - Escuchando...   │  <-- [BUG: Notificaciones se apilan!]
  └───────────────────────────────┘
  ┌───────────────────────────────┐
  │ 🎙  Viernes - Escuchando...   │
  └───────────────────────────────┘

Después del fix (Comportamiento corregido):
Desktop Screen (Hyprland / Mako):
  ┌───────────────────────────────┐
  │ 🎙  Viernes - Escuchando...   │  <-- [Notificación única que se reemplaza dinámicamente con -r / ID]
  └───────────────────────────────┘
```

---

### Criterios de Aceptación
1. **Reemplazo unívoco de notificaciones en Wayland/Hyprland:** La función `send_desktop_notification` debe utilizar un ID de reemplazo persistente (`notify-send -r <id>` o archivo de ID temporal) para garantizar que en gestores de notificaciones como `mako`, `dunst` o `swaync` nunca se apilen múltiples tarjetas de estado.
2. **Despacho de dispositivo en AudioPlayer:** El reproductor `AudioPlayer` debe leer y respetar la variable `AUDIO_OUTPUT_DEVICE` del archivo de configuración, pasando el parámetro correspondiente a `mpv` (`--audio-device=...`) o `pw-play` (`--target=...`).
3. **Cierre seguro del Centro de Control (Clean Exit):** Al cerrar la ventana del Centro de Control (`WM_DELETE_WINDOW`), el sistema debe abortar cualquier hilo activo de síntesis de audio, detener procesos de reproducción hijos y evitar llamadas a `self.after` que causen excepciones `TclError`.
4. **Validación robusta de credenciales:** Si la API key ingresada en la GUI es errónea o inválida (código HTTP 401 de ElevenLabs), la interfaz debe interceptar la excepción y reportar claramente "Credenciales inválidas (401)" en vez de volcar un traceback largo o crashear.
5. **Señalización al servicio tras guardar configuración:** Al guardar exitosamente en `.env` desde la GUI, si el servicio `viernes.service` está activo, se debe emitir una señal de recarga o reinicio rápido (`systemctl --user reload-or-restart viernes.service`) para que el demonio en ejecución tome los nuevos valores sin intervención manual.
6. **Manejo de rutas .env independientes del directorio de ejecución:** `ConfigService` debe almacenar y resolver la ruta canónica absoluta del proyecto (`/home/svfabio/Projects/09-viernes/.env`) sin importar desde qué directorio se invoque `viernes-gui`.
7. **Pruebas de estrés y no regresión:** Se deben implementar pruebas automatizadas que simulen múltiples notificaciones consecutivas, verificación de argumentos pasados a `mpv`, y cierre concurrente de la ventana durante la reproducción.

---

### Subtareas de Implementación
- **SUB-20.1:** Modificar `src/viernes/input/push_to_talk.py` para almacenar el último ID de notificación emitido e invocar `notify-send -r <id>` o `-p` (print ID) para reemplazo estricto.
- **SUB-20.2:** Modificar `src/viernes/audio/player.py` para aceptar y despachar el dispositivo de salida configurado en `AUDIO_OUTPUT_DEVICE`.
- **SUB-20.3:** Implementar el manejador `WM_DELETE_WINDOW` en `src/viernes/gui/app.py` con detención de procesos y cancelación de threads.
- **SUB-20.4:** Conectar el guardado de configuración en la GUI con la recarga opcional del servicio systemd.
- **SUB-20.5:** Diseñar suite de pruebas en `tests/test_audio_input/test_push_to_talk.py` y `tests/test_tts/test_player.py` comprobando las correcciones.
