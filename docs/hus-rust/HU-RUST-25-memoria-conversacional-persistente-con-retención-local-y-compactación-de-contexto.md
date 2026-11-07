# HU-RUST-25: Memoria conversacional persistente con retención local y compactación de contexto

**Puntos de Historia:** 5 | **Prioridad:** High | **Versión:** 1 | **Asignado a:** Agente 2 (Brain & Pipeline)

---

### Descripción
El usuario mantiene conversaciones fluidas a lo largo del tiempo mediante la memoria local persistente que retiene turnos recientes y compacta sesiones anteriores.

---

### Mockup / Flujo de Interacción
```text
[Turno conversacional completado]
                 │
                 ▼
     ┌────────────────────────┐
     │ ConversationStore      │ ── Persistencia atómica en SQLite / JSON local
     └────────────────────────┘
                 │ Si supera límite de tokens
                 ▼
     ┌────────────────────────┐
     │ ContextCompactor       │ ── Genera resumen compacto de turnos viejos
     └────────────────────────┘
```

---

### Criterios de Aceptación
1. **Persistencia local:** Guarda mensajes con roles (`user`, `assistant`, `tool`) en disco local de forma atómica.
2. **Recuperación histórica:** Carga los últimos N turnos o K tokens al iniciar una nueva sesión.
3. **Compactación automática:** Cuando el historial supera el presupuesto de tokens, comprime turnos antiguos preservando datos clave.
4. **Rotación segura:** Permite limpiar o reiniciar la memoria mediante comando explícito o flag `--new-conversation`.
5. **Pruebas de integridad:** Pruebas de crash durante escritura validan que la base de datos nunca se corrompe.

---

### Subtareas Técnicas
- **ST-RUST-25.1:** Implementar almacenamiento conversacional local y esquema de mensajes
- **ST-RUST-25.2:** Desarrollar algoritmo de ventana deslizante y compactación de tokens
- **ST-RUST-25.3:** Pruebas unitarias de persistencia concurrente y recuperación ante fallos
