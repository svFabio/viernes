# HU-RUST-27: Motor TTS Piper local offline con fallback ultrarrápido sin conexión

**Puntos de Historia:** 5 | **Prioridad:** High | **Versión:** 1 | **Asignado a:** Agente 3 (TTS & Output)

---

### Descripción
El usuario escucha las respuestas de Viernes incluso sin conexión a internet mediante el motor de síntesis local Piper ejecutado como fallback nativo.

---

### Mockup / Flujo de Interacción
```text
[Texto de oración listo para TTS]
                 │
                 ▼
     ElevenLabs disponible? ── SÍ ──► [ElevenLabs Client]
                 │ NO
                 ▼
     Internet disponible? ── SÍ ──► [Edge-TTS Client]
                 │ NO
                 ▼
     ┌────────────────────────┐
     │ PiperLocalProvider     │ ── Inferencia ONNX / binario local en < 150ms
     └────────────────────────┘
                 │ (Audio WAV/PCM)
                 ▼
     [Reproductor de audio local]
```

---

### Criterios de Aceptación
1. **Inferencia local Piper:** Ejecuta el binario o runtime de Piper local apuntando al modelo ONNX en español configurado.
2. **Integración en ResilientTts:** Actúa como el nivel 3 de fallback automático ante pérdida total de conectividad.
3. **Baja latencia:** Genera el primer chunk de audio en < 200 ms desde la entrega del texto.
4. **Manejo de ausencia de modelo:** Si el archivo ONNX no existe en disco, emite diagnóstico claro sin pánico.
5. **Pruebas unitarias:** Mock provider para CI sin requerir modelos ONNX binarios pesados.

---

### Subtareas Técnicas
- **ST-RUST-27.1:** Implementar PiperLocalProvider invocando modelo ONNX o CLI de Piper
- **ST-RUST-27.2:** Incorporar Piper como tercer escalón en la cadena ResilientTtsEngine
- **ST-RUST-27.3:** Pruebas unitarias con streams simulados y benchmarks de latencia
