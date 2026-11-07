# HU-RUST-22: Motor de validación de seguridad sandbox y sanitización de comandos

**Puntos de Historia:** 5 | **Prioridad:** Highest | **Versión:** 1 | **Asignado a:** Agente 2 (Brain & Pipeline)

---

### Descripción
El usuario ejecuta comandos y herramientas mediante el motor de validación de seguridad para evitar inyecciones de shell, accesos a rutas prohibidas y operaciones destructivas no autorizadas.

---

### Mockup / Flujo de Interacción
```text
[Entrada de comando / ruta]
                 │
                 ▼
     ┌────────────────────────┐
     │ SecurityValidator      │ <── Reglas de rutas permitidas & blacklists
     └────────────────────────┘
                 │
          Es seguro? ──► NO ──► [SecurityViolationError + Log Forense]
                 │
                SÍ
                 ▼
     [Comando sanitizado listo para ejecución]
```

---

### Criterios de Aceptación
1. **Sanitización de argumentos:** Rechaza caracteres peligrosos de interpolación de shell (`|`, `;`, `&`, `$`, `` ` ``) en herramientas de comandos.
2. **Validación de rutas:** Impide Directory Traversal (`../`) restringiendo lecturas y escrituras al espacio de trabajo permitido.
3. **Comandos destructivos:** Identifica comandos con riesgo de pérdida de datos (`rm -rf`, `mkfs`, `dd`, `truncate`) y exige flag de confirmación.
4. **Revisión estricta:** Cero falsos negativos en la suite de inyección de payloads conocidos.
5. **Pruebas con proptest:** Genera cadenas aleatorias arbitrarias para demostrar que ninguna secuencia escapa a la sanitización.

---

### Subtareas Técnicas
- **ST-RUST-22.1:** Implementar SecurityValidator con reglas de traversal y sanitización de shell
- **ST-RUST-22.2:** Integrar clasificador de operaciones destructivas y flags de confirmación
- **ST-RUST-22.3:** Suite de tests de penetración sintética y fuzzing con proptest
