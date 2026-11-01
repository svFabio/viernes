# HU-09: Procesar intención del usuario mediante el agente Antigravity

**Puntos de Historia:** 5 | **Prioridad:** Highest | **Versión:** 1 | **Asignado a:** Agente Cerebro / Orquestación

---

### Descripción
El usuario envía solicitudes en lenguaje natural y el agente de Antigravity (a través de su SDK de Python o interfaz headless) razona sobre la intención, mantiene el contexto de la conversación y formula la respuesta o selecciona las herramientas pertinentes.

---

### Mockup / Flujo de Interacción
```text
[Texto transcrito: "¿Qué tareas tengo para hoy?"]
                        │
                        ▼
             ┌─────────────────────┐
             │    Brain / Agent    │ <─── [System Prompt & Memory]
             └─────────────────────┘
                        │
            (Razonamiento / Tool Call)
                        │
                        ▼
  "Tienes 3 tareas en TickTick pendientes para hoy..."
```

---

### Criterios de Aceptación
1. **Contrato de interfaz Brain:** El componente implementa la clase base abstracta `Agent` con el método `ask(prompt: str) -> str` (o streaming async `ask_stream`).
2. **Integración con Antigravity:** El orquestador se conecta al runtime de Antigravity (SDK `google-antigravity` o CLI en modo headless con pipes IPC).
3. **Prompt de sistema de Viernes:** Al enviar la petición, se inyecta un System Prompt que define la personalidad de Viernes: conciso, directo, enfocado en respuestas habladas (sin Markdown pesado, sin listas largas ni emojis).
4. **Manejo de historial y memoria:** El agente conserva el contexto de los últimos turnos de conversación de la sesión para resolver referencias anafóricas (ej. "¿y cuánto dura?").
5. **Control de timeout de razonamiento:** Si el agente no genera respuesta en menos de 10 segundos, se aborta la petición y se entrega una respuesta de degradación airosa.
6. **Agente Fake para pruebas locales:** Existe una implementación `EchoAgent` o `MockAgent` que responde textos predecibles para tests unitarios y de integración sin conexión a internet ni consumo de tokens.
7. **Pruebas unitarias de Brain:** Se prueba con tests automatizados la correcta serialización de mensajes, manejo de contexto y captura de timeouts.
