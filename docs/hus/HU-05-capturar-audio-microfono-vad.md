# HU-05: Capturar audio de voz desde el micrófono con detección de actividad (VAD)

**Puntos de Historia:** 5 | **Prioridad:** Highest | **Versión:** 1 | **Asignado a:** Agente Audio / STT

---

### Descripción
El usuario habla de manera natural frente al micrófono y el sistema detecta automáticamente cuándo empieza y cuándo termina la frase mediante detección de actividad de voz (VAD) y corte por silencio, entregando un archivo de audio limpio para su transcripción sin capturar ruido ambiental previo o posterior.

---

### Mockup / Flujo de Interacción
```text
[Micrófono Input] ──► [Stream de Audio PCM 16kHz]
                             │
                             ▼
                    ┌─────────────────┐
                    │    VAD Filter   │
                    └─────────────────┘
                      │             │
        (Voz detectada)             (Silencio continuo > 800ms)
              ▼                             ▼
       [Acumular Buffer]             [Cortar segmento y guardar WAV]
```

---

### Criterios de Aceptación
1. **Captura en tiempo real:** Al activar el modo escucha, el sistema abre un stream de entrada de audio a 16.000 Hz, 1 canal (mono) y formato PCM de 16 bits usando `sounddevice` o PipeWire.
2. **Detección de voz (VAD):** El sistema procesa los frames de audio en bloques de 20-30 ms evaluando la probabilidad de voz humana.
3. **Inicio automático de buffer:** Al detectarse voz por encima del umbral de actividad, el sistema comienza a almacenar los frames de audio y marca el estado como `RECORDING`.
4. **Corte por silencio:** Cuando se detecta un silencio ininterrumpido superior a 800 ms tras una locución activa, el sistema finaliza la captura del segmento y cambia a estado `PROCESSING`.
5. **Duración mínima de locución:** Si el segmento capturado dura menos de 300 ms (ruidos secos, chasquidos, toses breves), el sistema descarta el audio y vuelve a estado de reposo sin enviar a transcripción.
6. **Límite máximo de seguridad:** Si el usuario habla continuamente por más de 30 segundos, el sistema corta la grabación para evitar desbordes de memoria y procesa el segmento acumulado.
7. **Generación de archivo de audio:** El segmento capturado se escribe como archivo temporal `.wav` válido con su correspondiente cabecera RIFF.
8. **Liberación de recursos:** Al detener la escucha o ante una interrupción de teclado (`Ctrl+C`), el stream de audio se cierra inmediatamente y se liberan los descriptores de archivo.
9. **Pruebas sin micrófono real:** El módulo cuenta con tests unitarios que inyectan arrays de NumPy simulados para validar la lógica de detección de voz y corte por silencio sin requerir hardware de audio activo.
