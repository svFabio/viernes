# HU-RUST-08: Máquina de estados central (Idle, Listening, Thinking, Speaking) y bus de eventos

**Puntos de Historia:** 5 | **Prioridad:** Highest | **Versión:** 1 | **Asignado a:** Agente 2 (Brain & Coordinator)

---

### Descripción
El usuario experimenta un asistente predecible y coherente mediante una máquina de estados determinista para coordinar cuándo escuchar, cuándo pensar y cuándo hablar sin estados inconsistentes.

---

### Mockup / Flujo de Interacción
```text
      ┌───────────┐
      │   Idle    │ ◄─────────────────────────┐
      └─────┬─────┘                           │
            │ (VoiceStarted / PTT)            │ (PlaybackFinished)
            ▼                                 │
      ┌───────────┐                     ┌─────┴─────┐
      │ Listening │ ──► Thinking ───►   │ Speaking  │
      └───────────┘    (VoiceFinished)  └───────────┘
```

---

### Criterios de Aceptación
1. **Definición de estados:** Modela estrictamente los estados `Idle`, `Listening`, `Thinking` y `Speaking` con tipos cerrados.
2. **Transiciones deterministas:** Cualquier evento inesperado (ej. audio entrante mientras ya está en `Listening`) se maneja con descarte limpio o transición segura.
3. **Bus de eventos broadcast:** El estado se distribuye a todos los subsistemas mediante un canal `tokio::sync::broadcast` con latencia sub-microsegundo.
4. **Telemetría de transiciones:** Cada cambio de estado registra timestamp, estado previo, estado nuevo y causa disparadora en logs estructurados.
5. **Soporte de interrupción:** Una transición de `Speaking` a `Listening` cancela inmediatamente la reproducción activa.
6. **Pruebas de propiedad:** Se valida con `proptest` que ninguna secuencia aleatoria de eventos deje al sistema en estado bloqueado o zombi.

---

### Subtareas Técnicas
- **ST-RUST-08.1:** Modelar StateMachine y canal broadcast de eventos del sistema
- **ST-RUST-08.2:** Implementar lógica de transición de estados y despacho concurrente
- **ST-RUST-08.3:** Suite de pruebas property-based para robustez de la máquina de estados
