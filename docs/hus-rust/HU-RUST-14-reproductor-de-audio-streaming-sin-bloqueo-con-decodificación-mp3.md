# HU-RUST-14: Reproductor de audio streaming sin bloqueo con decodificación MP3

**Puntos de Historia:** 5 | **Prioridad:** High | **Versión:** 1 | **Asignado a:** Agente 3 (TTS & Audio Output)

---

### Descripción
El usuario escucha las respuestas de voz con reproducción suave e ininterrumpida sin que la interfaz o los hilos de red se congelen.

---

### Mockup / Flujo de Interacción
```text
[Chunks MP3 de ElevenLabs / Edge-TTS]
                   │
                   ▼
       ┌────────────────────────┐
       │   StreamingAudioPlayer │ <── Backend rodio / symphonia
       └────────────────────────┘
                   │
                   ▼
       [Cola FIFO de fragmentos en hilo de reproducción dedicado]
                   │
                   ▼
       [Emisión continua hacia los altavoces seleccionados]
```

---

### Criterios de Aceptación
1. **Decodificación MP3 en memoria:** Decodifica streams de bytes MP3 en tiempo real utilizando `symphonia` o decodificadores nativos de `rodio`.
2. **Hilo de reproducción dedicado:** El reproductor opera en su propio hilo de alta prioridad para evitar glitches o saltos de audio.
3. **Cola FIFO continua:** Admite encolar múltiples oraciones secuenciales, reproduciéndolas fluidamente sin silencios entre fragmentos.
4. **Evento de finalización:** Emite el evento `PlaybackFinished` cuando el último chunk de la cola concluye.
5. **Selección de dispositivo:** Permite enrutar el stream de salida hacia el dispositivo configurado en `AUDIO_OUTPUT_DEVICE`.
6. **Pruebas sin servidor de audio:** Dispone de un mock `SilentAudioSink` para pruebas en entornos headless de CI.

---

### Subtareas Técnicas
- **ST-RUST-14.1:** Implementar StreamingAudioPlayer basado en rodio Sink y symphonia
- **ST-RUST-14.2:** Implementar cola FIFO concurrente con notificación de fin de reproducción
- **ST-RUST-14.3:** Tests unitarios con SilentAudioSink en entornos headless
