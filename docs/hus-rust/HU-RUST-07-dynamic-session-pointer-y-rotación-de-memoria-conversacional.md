# HU-RUST-07: Dynamic Session Pointer y rotación de memoria conversacional

**Puntos de Historia:** 5 | **Prioridad:** High | **Versión:** 1 | **Asignado a:** Agente 2 (Brain & Coordinator)

---

### Descripción
El usuario renueva el contexto de su conversación por voz diciendo 'Viernes, abre otra conversación' mediante el gestor de punteros de sesión para limpiar la memoria activa sin reiniciar el servicio.

---

### Mockup / Flujo de Interacción
```text
[Voz: "Viernes, abre otra conversación"]
                   │
                   ▼
       ┌────────────────────────┐
       │ SessionPointerManager  │ <── Lectura/Escritura session.json
       └────────────────────────┘
                   │
       ┌───────────┴───────────┐
       ▼                       ▼
[Archivar ID anterior]    [Generar nuevo UUIDv4]
       │                       │
       ▼                       ▼
[Confirmar por voz: "Iniciando nueva sesión, Señor"]
```

---

### Criterios de Aceptación
1. **Persistencia atómica:** La sesión activa se almacena en `~/.config/viernes/session.json` mediante escritura atómica (archivo temporal + renombrado).
2. **Detección de intención de rotación:** El coordinador analiza comandos de intención con regex compilada para detectar frases como 'abre otra conversación' o 'nueva conversación'.
3. **Generación de nuevo puntero:** La rotación genera un nuevo UUIDv4 y archiva el ID anterior en el registro histórico.
4. **Reconexión inmediata:** El cliente de Antigravity adopta el nuevo session ID en la siguiente interacción sin necesidad de reiniciar el daemon.
5. **Flag CLI --new-conversation:** Permite forzar la rotación desde línea de comandos al invocar el binario.
6. **Pruebas de concurrencia:** Pruebas unitarias verifican que lecturas y escrituras simultáneas a `session.json` no produzcan archivos corruptos.

---

### Subtareas Técnicas
- **ST-RUST-07.1:** Implementar SessionPointerManager con escritura atómica de archivos JSON
- **ST-RUST-07.2:** Implementar detector de intención de rotación en el analizador léxico
- **ST-RUST-07.3:** Tests de integridad y carreras concurrentes en sesión
