# HU-08: Configurar dispositivo de entrada de micrófono y umbrales de captura

**Puntos de Historia:** 2 | **Prioridad:** Low | **Versión:** 1 | **Asignado a:** Agente Audio / Config

---

### Descripción
El usuario selecciona el micrófono de entrada y calibra la sensibilidad de captura y umbrales de silencio en la configuración para adaptarse a micrófonos internos, auriculares bluetooth o micrófonos USB dedicados.

---

### Mockup / Flujo de Interacción
```text
.env:
├── AUDIO_INPUT_DEVICE="default" # o índice numérico / nombre parcial
├── VAD_THRESHOLD=0.5            # sensibilidad de voz (0.1 a 0.9)
├── SILENCE_TIMEOUT_MS=800       # milisegundos de silencio para cortar
└── STT_MODEL_SIZE="base"        # tiny, base, small
```

---

### Criterios de Aceptación
1. **Listado de dispositivos disponibles:** El asistente provee un comando CLI (`python -m viernes.main --list-audio-devices`) que enumera los micrófonos y altavoces detectados por el sistema con sus nombres e índices.
2. **Selección por índice o nombre:** Si `AUDIO_INPUT_DEVICE` se define como un entero o una subcadena de texto, el sistema selecciona el dispositivo correspondiente sin requerir nombres exactos de PipeWire.
3. **Dispositivo no encontrado:** Si el dispositivo indicado no existe o está desconectado, el sistema emite un log de `WARNING` y utiliza el dispositivo por defecto del sistema operativo sin fallar.
4. **Calibración de umbral VAD:** La variable `VAD_THRESHOLD` valida que el valor numérico esté en el rango `[0.0, 1.0]`; valores fuera de rango son ajustados al valor por defecto (0.5) con una advertencia en logs.
5. **Calibración de tiempo de silencio:** La variable `SILENCE_TIMEOUT_MS` acepta un valor entre 300 y 3000 ms, determinando con precisión la pausa tras la cual se corta la frase.
6. **Soporte de cambio en caliente:** Al reiniciar el servicio tras modificar `.env`, la nueva configuración de audio se aplica en la siguiente captura.
