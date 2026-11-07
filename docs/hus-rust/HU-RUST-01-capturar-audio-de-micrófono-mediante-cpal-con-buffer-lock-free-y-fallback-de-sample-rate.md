# HU-RUST-01: Capturar audio de micrófono mediante cpal con buffer lock-free y fallback de sample rate

**Puntos de Historia:** 5 | **Prioridad:** Highest | **Versión:** 1 | **Asignado a:** Agente 1 (Audio Input & STT)

---

### Descripción
El usuario habla de manera natural frente al micrófono del sistema mediante el capturador nativo cpal para registrar su señal de voz con latencia sub-milisegundo y sin degradación de rendimiento.

---

### Mockup / Flujo de Interacción
```text
[Micrófono Hardware (ALSA / PipeWire)]
                     │
                     ▼
         ┌─────────────────────────┐
         │ cpal AudioCaptureStream │ <── Fallback automático de sample rate
         └─────────────────────────┘
                     │ (Float32 PCM Ring-Buffer)
                     ▼
         ┌─────────────────────────┐
         │  crossbeam lock-free    │
         └─────────────────────────┘
                     │
                     ▼
         [Chunk de audio listo para VAD]
```

---

### Criterios de Aceptación
1. **Inicialización cpal:** El sistema inicializa el stream de entrada utilizando el dispositivo de captura predeterminado del host o el especificado en configuración.
2. **Fallback defensivo de Sample Rate:** Si el hardware rechaza una frecuencia de 16,000 Hz, el capturador conmuta automáticamente a la frecuencia nativa del dispositivo (44.1 kHz o 48 kHz) y aplica submuestreo por software.
3. **Buffer libre de bloqueos:** Las muestras de audio capturadas en el callback de tiempo real se transmiten a través de un ring-buffer lock-free (`crossbeam-channel`) sin alocaciones dinámicas en el bucle caliente.
4. **Formato estándar PCM:** El stream de audio produce muestras en formato Float32 normalizadas en el rango [-1.0, 1.0].
5. **Manejo de desconexión:** Si el dispositivo de audio es desconectado durante la captura, el sistema captura el error sin entrar en pánico y emite un evento `AudioDeviceDisconnected`.
6. **Cero fugas de memoria:** La ejecución continua durante 1 hora de captura no incrementa el consumo de memoria residente (RSS < 30 MB).
7. **Pruebas herméticas:** El módulo cuenta con un `FakeAudioSource` para ejecutar pruebas unitarias y de integración sin depender de hardware físico ni servidores de sonido.

---

### Subtareas Técnicas
- **ST-RUST-01.1:** Diseñar abstracción AudioCaptureStream y ring-buffer concurrente con crossbeam
- **ST-RUST-01.2:** Implementar algoritmo de resampleo defensivo y fallback de sample rate
- **ST-RUST-01.3:** Pruebas unitarias con FakeAudioDevice y verificación de memoria con Valgrind
