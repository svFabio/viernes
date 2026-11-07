# HU-RUST-17: Scaffolding del Cargo Workspace modular con #![forbid(unsafe_code)]

**Puntos de Historia:** 5 | **Prioridad:** Highest | **Versión:** 1 | **Asignado a:** Orquestador / Infraestructura

---

### Descripción
El equipo de desarrollo cuenta con una base de código modular y 100% segura en Rust con separación estricta de responsabilidades entre crates.

---

### Mockup / Flujo de Interacción
```text
viernes-rust (Workspace)
  ├── crates/viernes-core  (Contratos, Eventos, StateMachine)
  ├── crates/viernes-audio (cpal, VAD, Whisper)
  ├── crates/viernes-brain (Antigravity NDJSON, Pipeline)
  └── crates/viernes-tts   (ElevenLabs, Edge-TTS, Rodio)
```

---

### Criterios de Aceptación
1. **Workspace Cargo multi-crate:** Estructura modular con crates independientes `viernes-core`, `viernes-audio`, `viernes-brain` y `viernes-tts`.
2. **Prohibición de código inseguro:** Se declara `#![forbid(unsafe_code)]` en la raíz de cada crate sin excepciones no aprobadas.
3. **Linter estricto:** Compilación limpia con `cargo clippy --all-targets -- -D warnings` (0 advertencias toleradas).
4. **Traits canónicos en viernes-core:** Define los contratos `AudioInput`, `SpeechToText`, `BrainService`, `TextToSpeech` y `AudioSink`.
5. **Manejo de errores idiomático con thiserror:** Cada crate expone sus tipos de error fuertemente tipados sin conversión genérica a String.
6. **CI automatizado:** Workflow de GitHub Actions configurado para compilar, verificar formato (`cargo fmt`) y pasar linter en cada PR.

---

### Subtareas Técnicas
- **ST-RUST-17.1:** Crear estructura de workspace Cargo con los 4 crates y dependencias base
- **ST-RUST-17.2:** Configurar linter clippy estricto y políticas de unsafe_code
- **ST-RUST-17.3:** Definir traits e interfaces comunes en viernes-core
