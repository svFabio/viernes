# HU-RUST-06: Comunicación asíncrona NDJSON vía subproceso streaming con Antigravity

**Puntos de Historia:** 8 | **Prioridad:** Highest | **Versión:** 1 | **Asignado a:** Agente 2 (Brain & Coordinator)

---

### Descripción
El usuario interactúa con la inteligencia de Antigravity mediante un cliente de subproceso NDJSON asíncrono para recibir razonamientos, respuestas habladas y llamadas a herramientas en tiempo real.

---

### Mockup / Flujo de Interacción
```text
[Prompt transcripto: "Hola Viernes"]
                   │
                   ▼
       ┌────────────────────────┐
       │ AntigravityProcessPipe │ <── Subproceso tokio::process (stdin/stdout)
       └────────────────────────┘
                   │
                   ▼  (Líneas NDJSON en tiempo real)
       [Línea: {"type":"thought", "content":"Pensando..."}]
       [Línea: {"type":"text_chunk", "content":"Hola Señor."}]
       [Línea: {"type":"tool_call", "name":"run_command"}]
                   │
                   ▼
       [Despacho de eventos al pipeline de Viernes]
```

---

### Criterios de Aceptación
1. **Spawn asíncrono del daemon:** El cliente inicia el proceso `antigravity-cli` con pipes no bloqueantes en `stdin`, `stdout` y `stderr`.
2. **Streaming línea por línea:** Utiliza `tokio::io::BufReader` y `LinesStream` para procesar cada objeto JSON sin buffering masivo en memoria.
3. **Deserialización tipada con Serde:** Cada mensaje NDJSON se deserializa en un enum tipado `BrainEvent` con validación estricta de esquemas.
4. **Manejo de errores de proceso:** Si el subproceso falla o termina prematuramente con código de error, el cliente captura el stderr y notifica la anomalía.
5. **Timeout de inactividad:** Si transcurren más de 15 segundos sin recibir tokens desde el envío del prompt, se emite una alerta de timeout.
6. **Cierre limpio:** Al finalizar la aplicación, el proceso hijo recibe `SIGTERM` y se espera hasta 3 segundos antes de forzar `SIGKILL`.
7. **Mock subproceso para CI:** Existe un simulador de NDJSON para ejecutar pruebas sin requerir el binario de Antigravity instalado.

---

### Subtareas Técnicas
- **ST-RUST-06.1:** Implementar spawn y gestión de subproceso con tokio::process
- **ST-RUST-06.2:** Modelar esquemas Serde para eventos NDJSON y parser de líneas en stream
- **ST-RUST-06.3:** Implementar MockAntigravityServer y tests de integración de streaming
