# HU-10: Ejecutar comandos del sistema permitidos mediante herramientas MCP

**Puntos de Historia:** 5 | **Prioridad:** Highest | **Versión:** 1 | **Asignado a:** Agente Seguridad / Herramientas

---

### Descripción
El usuario solicita acciones operativas en su máquina (consultar tareas, abrir aplicaciones, ejecutar scripts controlados) y el asistente las ejecuta de forma segura a través de herramientas MCP autorizadas bajo una lista blanca estricta, prohibiendo la ejecución arbitraria de shells libres.

---

### Mockup / Flujo de Interacción
```text
[Solicitud de Acción: "Abre Spotify y pon música"]
                       │
                       ▼
            ┌──────────────────────┐
            │   MCP Security Gate  │
            └──────────────────────┘
              │                  │
        (En Allowlist)     (No autorizada)
              ▼                  ▼
     [Ejecución Controlada] [Rechazo seguro con aviso por voz]
```

---

### Criterios de Aceptación
1. **Lista blanca estricta (Allowlist):** Solo se permite invocar herramientas registradas explícitamente en el archivo de configuración de seguridad (ej. `ticktick_*`, `system_open_app`, `system_volume_control`).
2. **Prohibición de shell libre:** El agente no tiene acceso a comandos arbitrarios como `bash -c`, `eval`, `rm -rf` o llamadas al sistema sin sanitizar.
3. **Validación de parámetros:** Todos los argumentos entregados por el LLM a la herramienta se validan mediante esquemas estrictos de Pydantic antes de ser despachados.
4. **Rechazo seguro:** Si el LLM intenta invocar una herramienta no autorizada o fuera del esquema permitido, la llamada es bloqueada de inmediato y se genera un evento de seguridad.
5. **Respuesta inteligible de la herramienta:** El resultado retornado por la herramienta se formatea como texto plano para que el agente elabore la respuesta hablada final.
6. **Auditoría de ejecuciones:** Cada herramienta ejecutada se registra en el log de auditoría con timestamp, nombre de la herramienta, estado de ejecución y duración, omitiendo secretos.
7. **Pruebas de seguridad:** Existen pruebas automatizadas que verifican el rechazo de inyecciones de comandos y llamadas no permitidas.
