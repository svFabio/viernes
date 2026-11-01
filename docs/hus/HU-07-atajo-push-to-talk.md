# HU-07: Activar captura de audio mediante atajo global Push-to-Talk en Linux

**Puntos de Historia:** 3 | **Prioridad:** Medium | **Versión:** 1 | **Asignado a:** Agente Entrada / UI

---

### Descripción
El usuario presiona y mantiene un atajo de teclado global en su entorno de escritorio Linux para forzar la escucha inmediata del asistente, permitiendo interactuar con precisión en entornos ruidosos donde la detección por VAD automática podría activarse por error.

---

### Mockup / Flujo de Interacción
```text
[Presionar Atajo (ej. Super+V)] ──► Estado: LISTENING (Tono o Notificación)
                                            │
                                            ▼
                               [Habla el usuario...]
                                            │
                                            ▼
[Soltar Atajo (Key Release)]   ──► Estado: PROCESSING ──► [Transcribir audio]
```

---

### Criterios de Aceptación
1. **Configuración de combinación de teclas:** La combinación de teclas del atajo se lee desde `.env` (ej. `PUSH_TO_TALK_KEY=Super+V` o `Ctrl+Alt+Space`).
2. **Activación al pulsar:** Al detectar el evento *Key Down* del atajo global, el sistema cambia inmediatamente su estado a `LISTENING` y comienza la captura de audio.
3. **Indicador de escucha:** Al iniciar la captura, el sistema emite una señal sonora breve o envía una notificación desktop (`notify-send`) indicando "Escuchando...".
4. **Finalización al soltar:** Al detectar el evento *Key Up* (liberación de la combinación), se finaliza la grabación de inmediato sin esperar umbrales de silencio de VAD.
5. **Compatibilidad con Hyprland / Wayland / X11:** El módulo se integra con atajos vía IPC o herramientas estándar de Linux (`hyprctl bind`, `evdev` o sockets de comando).
6. **Cancelación por Escape:** Si el usuario presiona `Escape` mientras mantiene presionado el atajo de hablar, la grabación actual se descarta de forma segura y se emite un sonido de cancelación.
7. **Control de concurrencia:** Si se presiona el atajo mientras el asistente ya se encuentra procesando una orden anterior, el sistema descarta la petición anterior o activa barge-in según la política configurada.
