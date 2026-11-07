# HU-RUST-23: Auditoría estructurada y logging forense de ejecución de herramientas

**Puntos de Historia:** 3 | **Prioridad:** Medium | **Versión:** 1 | **Asignado a:** Agente 2 (Brain & Pipeline)

---

### Descripción
El administrador del sistema audita todas las llamadas a herramientas mediante registros forenses estructurados en formato JSONL para trazabilidad total de acciones.

---

### Mockup / Flujo de Interacción
```text
[Invocación de Tool] ──► [AuditLogger::record_start]
                                       │
                                       ▼ (JSONL append)
                                  [~/.local/state/viernes/audit.jsonl]
                                       │
[Completada / Fallida] ──► [AuditLogger::record_finish con duración y status]
```

---

### Criterios de Aceptación
1. **Formato JSONL estándar:** Cada entrada contiene timestamp UTC, tool name, argumentos sanitizados, duración en ms y código de retorno.
2. **Rotación atómica:** El archivo de auditoría rota automáticamente al alcanzar 10 MB preservando los últimos 5 archivos históricos.
3. **Redacción de secretos:** API keys, tokens y credenciales se enmascaran automáticamente antes de persistir.
4. **Sin bloqueo:** El logging opera mediante canales no bloqueantes para no retrasar la ejecución principal.

---

### Subtareas Técnicas
- **ST-RUST-23.1:** Implementar AuditLogger concurrente con rotación de archivos
- **ST-RUST-23.2:** Agregar redactor de datos sensibles y formateador JSONL
- **ST-RUST-23.3:** Pruebas de rotación y verificación de no-bloqueo bajo carga
