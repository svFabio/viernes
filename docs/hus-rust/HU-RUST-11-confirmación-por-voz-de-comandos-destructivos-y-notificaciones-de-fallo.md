# HU-RUST-11: Confirmación por voz de comandos destructivos y notificaciones de fallo

**Puntos de Historia:** 5 | **Prioridad:** Medium | **Versión:** 1 | **Asignado a:** Agente 2 (Brain & Coordinator)

---

### Descripción
El usuario mantiene el control seguro de su sistema recibiendo alertas auditivas claras y solicitudes de confirmación explícita antes de ejecutar acciones de alto riesgo (ej. borrar directorios).

---

### Mockup / Flujo de Interacción
```text
[Comando riesgoso detectado: "rm -rf /directorio"]
                   │
                   ▼
       [Viernes vocaliza: "¿Está seguro que desea ejecutar esta acción, Señor?"]
                   │
                   ▼
       [Esperar confirmación: "Sí, procede" / "Cancela"]
                   │
       ┌───────────┴───────────┐
       ▼                       ▼
  [Ejecutar acción]       [Abortar con feedback sonoro]
```

---

### Criterios de Aceptación
1. **Clasificador de riesgo:** Identifica comandos destructivos o de alto impacto mediante patrones configurados.
2. **Pausa en espera de confirmación:** El asistente no ejecuta la acción y entra en estado `AwaitingConfirmation` con timeout de 10 segundos.
3. **Mapeo de respuestas afirmativas/negativas:** Procesa respuestas como 'sí', 'afirmativo', 'procede' para ejecutar, o 'no', 'cancela', 'aborta' para descartar.
4. **Cancelación por timeout:** Si transcurren 10 segundos sin respuesta explícita, la acción se cancela automáticamente por seguridad.
5. **Notificación de fallos de infraestructura:** Si falla la red, el micrófono o una API externa, se vocaliza una explicación clara en lenguaje natural.
6. **Pruebas de flujo de confirmación:** Pruebas unitarias verifican la denegación por omisión y la confirmación positiva/negativa.

---

### Subtareas Técnicas
- **ST-RUST-11.1:** Implementar clasificador de operaciones destructivas y estado de confirmación
- **ST-RUST-11.2:** Implementar temporizador de cancelación y feedback sonoro de fallos
- **ST-RUST-11.3:** Pruebas unitarias de flujos de confirmación y rechazo
