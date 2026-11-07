# HU-RUST-26: Inyección de plantillas de sistema Prompt Templates y modulación de personalidad

**Puntos de Historia:** 3 | **Prioridad:** Medium | **Versión:** 1 | **Asignado a:** Agente 2 (Brain & Pipeline)

---

### Descripción
El usuario interactúa con Viernes con su personalidad cálida, concisa y profesional mediante plantillas de prompt de sistema dinámicamente inyectadas.

---

### Mockup / Flujo de Interacción
```text
[Inicio de sesión / turn]
                 │
                 ▼
     ┌────────────────────────┐
     │ PromptTemplateEngine   │ <── Contexto de usuario, herramientas y fecha/hora
     └────────────────────────┘
                 │
                 ▼
     [System Prompt compilado inyectado en Antigravity]
```

---

### Criterios de Aceptación
1. **Plantilla modular:** Ensambla dinámicamente identidad, restricciones operativas, lista de tools y variables de entorno.
2. **Inyección temporal:** Incorpora fecha, hora local y zona horaria actualizadas en cada turno.
3. **Zero-copy templating:** Renders de plantillas con buffers prealocados sin alocaciones innecesarias.
4. **Pruebas unitarias:** Verifica que las variables críticas siempre están presentes en el prompt final.

---

### Subtareas Técnicas
- **ST-RUST-26.1:** Crear motor de plantillas de sistema con variables dinámicas
- **ST-RUST-26.2:** Integrar ensamblado de catálogo de herramientas en el prompt
- **ST-RUST-26.3:** Pruebas de renderizado y validación de restricciones de personalidad
