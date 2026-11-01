# HU-14: Interrumpir reproducción de audio al detectar nueva voz del usuario (Barge-in)

**Puntos de Historia:** 5 | **Prioridad:** Highest | **Versión:** 1 | **Asignado a:** Agente Pipeline / Audio

---

### Descripción
El usuario empieza a hablar mientras el asistente está reproduciendo una respuesta y el sistema detiene de inmediato la salida de audio para escuchar y procesar la nueva instrucción sin obligar al usuario a escuchar toda la respuesta previa.

---

### Mockup / Flujo de Interacción
```text
Viernes hablando: [=== Audio Chunk en curso ===]
                                │
   Usuario interrumpe: "¡Viernes, cancela eso!" (VAD detecta voz)
                                │
                                ▼
                      ┌──────────────────┐
                      │ Barge-in Event   │
                      └──────────────────┘
                        │              │
       [SIGKILL / Stop al reproductor] [Vaciar cola de síntesis TTS]
                        │              │
                        ▼              ▼
                 [Silencio inmediato (< 150ms)]
                        │
                        ▼
           [Capturar y procesar nueva locución]
```

---

### Criterios de Aceptación
1. **Detección continua durante playback:** Mientras el reproductor de audio está activo, el stream del micrófono permanece abierto analizando actividad de voz mediante VAD o umbral de energía.
2. **Latencia de corte inmediata:** Al confirmarse voz humana por parte del usuario, la reproducción de audio se detiene en menos de 150 milisegundos enviando una señal de terminación (`SIGTERM`/`SIGKILL`) al subproceso del reproductor.
3. **Vaciado de cola de síntesis:** Todas las oraciones pendientes de sintetizar o chunks de audio encolados se descartan inmediatamente de la memoria.
4. **Cancelación del generador del LLM:** Si el agente aún estaba generando tokens para la respuesta previa, la corrutina o stream del LLM se cancela para ahorrar cómputo y ancho de banda.
5. **No feedback en bucle (Acoustic Echo Cancellation):** El sistema discrimina el audio emitido por los propios altavoces del asistente mediante descarte de amplitud o cancelación de eco básica para evitar que el asistente se interrumpa a sí mismo.
6. **Transición limpia a escucha:** Tras la detención del audio, el sistema pasa inmediatamente a estado `RECORDING` para procesar la nueva orden que el usuario está emitiendo.
7. **Pruebas de estrés de interrupción:** Pruebas automatizadas confirman que el reproductor no deja descriptores de audio bloqueados ni procesos zombie tras múltiples interrupciones consecutivas.
