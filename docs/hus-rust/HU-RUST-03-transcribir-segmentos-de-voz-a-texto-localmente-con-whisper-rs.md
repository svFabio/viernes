# HU-RUST-03: Transcribir segmentos de voz a texto localmente con whisper-rs

**Puntos de Historia:** 8 | **Prioridad:** Highest | **Versión:** 1 | **Asignado a:** Agente 1 (Audio Input & STT)

---

### Descripción
El usuario obtiene la transcripción textual exacta de sus comandos hablados de forma 100% privada y local mediante bindings optimizados de Whisper para ejecutar instrucciones sin depender de APIs cloud.

---

### Mockup / Flujo de Interacción
```text
[Buffer PCM Float32 (16kHz)]
                   │
                   ▼
       ┌────────────────────────┐
       │   WhisperTranscriber   │ <── Modelo local whisper.bin (tiny/base)
       └────────────────────────┘
                   │  (Inferencia nativa C++/AVX)
                   ▼
       [String transcripto: "Viernes, ¿cuál es el estado?"]
                   │
                   ▼
       [Canal mpsc hacia Agente 2 Cerebro]
```

---

### Criterios de Aceptación
1. **Carga del modelo local:** El transcriptor carga modelos compatibles con `whisper.cpp` (`ggml-base.bin` o `ggml-small.bin`) desde el directorio de modelos local.
2. **Aceleración por hardware:** La inferencia compila con soporte de instrucciones vectoriales nativas (AVX2 / FMA o CUDA/Vulkan si está disponible).
3. **Normalización a 16 kHz mono:** El componente valida que los buffers de entrada estén a 16,000 Hz Float32 antes de llamar al contexto de inferencia.
4. **Parámetros de idioma:** La transcripción se parametriza por defecto en español (`es`) con supresión de tokens no deseados.
5. **Inferencia no bloqueante:** El procesamiento neuronal se ejecuta en un threadpool de cómputo (`tokio::task::spawn_blocking`), sin bloquear el runtime async.
6. **Filtrado de transcripciones alucinadas:** Cadenas repetitivas o ruidos residuales (ej. `[Música]`, `Gracias`) son filtrados y retornan texto vacío.
7. **MockProvider para tests:** Existe una implementación `FakeTranscriber` para la suite de CI con 0 tiempo de inicialización.

---

### Subtareas Técnicas
- **ST-RUST-03.1:** Configurar crate whisper-rs y build script para compilación de whisper.cpp
- **ST-RUST-03.2:** Implementar trait SpeechToText con spawn_blocking y normalización de texto
- **ST-RUST-03.3:** Implementar FakeSpeechToText y tests de integración sin dependencias neuronales
