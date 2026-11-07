# HU-RUST-21: Despachador de herramientas Tool Dispatcher con registro dinámico y allowlist

**Puntos de Historia:** 5 | **Prioridad:** Highest | **Versión:** 1 | **Asignado a:** Agente 2 (Brain & Pipeline)

---

### Descripción
El agente asistente despacha herramientas locales registradas de forma segura y tipada mediante el Tool Dispatcher para ejecutar acciones en el sistema sin comprometer la estabilidad del proceso.

---

### Mockup / Flujo de Interacción
```text
[Intención de ToolCall desde Brain]
                 │
                 ▼
     ┌────────────────────────┐
     │ ToolRegistry (Allowlist)│ ── Si no está permitida ──► [ToolRejectedError]
     └────────────────────────┘
                 │ (Aprobada)
                 ▼
     ┌────────────────────────┐
     │ ToolDispatcher Async   │ ── Ejecución no bloqueante
     └────────────────────────┘
                 │
                 ▼
     [Resultado estructurado JSON emitido al Brain]
```

---

### Criterios de Aceptación
1. **Registro concurrente:** Permite registrar dinámicamente implementaciones de herramientas que satisfagan el trait `Tool`.
2. **Allowlist estricto:** Solo las herramientas registradas en la lista blanca (`DEFAULT_ALLOWLIST`) pueden ser invocadas; llamadas ajenas se rechazan de inmediato.
3. **Despacho asíncrono:** La ejecución de herramientas complejas corre en tareas asíncronas de Tokio sin bloquear el bucle de eventos del asistente.
4. **Validación de parámetros:** Los argumentos JSON se deserializan y validan de forma fuertemente tipada con Serde antes de la ejecución.
5. **Manejo de timeouts:** Cada invocación tiene un límite de tiempo configurable (default 30s) cancelando tareas colgadas.
6. **Pruebas unitarias:** 100% de cobertura en registro, ejecución exitosa, herramienta desconocida y timeout.

---

### Subtareas Técnicas
- **ST-RUST-21.1:** Diseñar trait Tool y estructura ToolRegistry con allowlist dinámico
- **ST-RUST-21.2:** Implementar ToolDispatcher asíncrono con control de timeouts y Serde
- **ST-RUST-21.3:** Pruebas unitarias exhaustivas y mock tools para validación
