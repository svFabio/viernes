# HU-RUST-02: Detección de Actividad de Voz (VAD) en tiempo real con ventana deslizante

**Puntos de Historia:** 5 | **Prioridad:** High | **Versión:** 1 | **Asignado a:** Agente 1 (Audio Input & STT)

---

### Descripción
El usuario modula su voz libremente mediante el detector de actividad de voz en tiempo real para que el asistente diferencie automáticamente cuándo está hablando y cuándo se detiene.

---

### Mockup / Flujo de Interacción
```text
[Stream Float32 PCM] ──► ┌─────────────────────────┐
                           │    VoiceActivityDetector│ <── Umbral RMS & Silencio
                           └─────────────────────────┘
                                        │
                         ┌──────────────┴──────────────┐
                         ▼                             ▼
                [Evento VoiceStarted]        [Evento VoiceFinished]
                         │                             │
                         ▼                             ▼
                [Activar Grabación]          [Enviar Buffer a STT]
```

---

### Criterios de Aceptación
1. **Cálculo RMS en ventana deslizante:** El sistema procesa tramas de 20 ms a 30 ms calculando la energía RMS (Root Mean Square) en tiempo real.
2. **Disparo de inicio de voz:** Cuando la energía supera el umbral configurado (`VAD_THRESHOLD`) durante al menos 2 tramas consecutivas, se emite el evento `VoiceStarted`.
3. **Detección de silencio sostenido:** Cuando la energía cae por debajo del umbral durante un tiempo acumulado (`SILENCE_TIMEOUT_MS`, ej. 800 ms), se emite el evento `VoiceFinished` con el buffer completo.
4. **Rechazo de chasquidos residuales:** Muestras de audio con duración de voz inferior a 250 ms son descartadas como ruido espurio.
5. **Límite máximo de seguridad:** Si el usuario habla continuamente por más de 20 segundos sin pausas, el VAD fuerza el corte y emite el audio acumulado para evitar buffers infinitos.
6. **Ajuste dinámico:** El umbral de sensibilidad puede reconfigurarse en caliente sin recrear el stream de audio.
7. **Pruebas con audio sintético:** Pruebas unitarias verifican la detección precisa de voz y silencio con archivos de onda pregrabados.

---

### Subtareas Técnicas
- **ST-RUST-02.1:** Implementar cálculo vectorial RMS con ventana deslizante y máquina de estados VAD
- **ST-RUST-02.2:** Implementar temporizador de silencio y límite de seguridad de 20 segundos
- **ST-RUST-02.3:** Pruebas unitarias de segmentación con datasets de audio y casos borde
