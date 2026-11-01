# HU-11: Solicitar confirmación explícita por voz antes de ejecutar acciones destructivas

**Puntos de Historia:** 3 | **Prioridad:** High | **Versión:** 1 | **Asignado a:** Agente Seguridad / Brain

---

### Descripción
El usuario es alertado por voz y debe responder afirmativamente antes de que el asistente ejecute una acción potencialmente dañina o irreversible (eliminar notas, borrar archivos, apagar el equipo o cancelar procesos críticos), previniendo desastres accidentales.

---

### Mockup / Flujo de Interacción
```text
Usuario: "Viernes, borra la carpeta de descargas"
                        │
                        ▼
Viernes (Voz): "¿Confirmas que deseas eliminar los archivos de Descargas? Di 'Sí' para proceder o 'Cancelar'."
                        │
                        ▼
           ┌────────────────────────┐
           │   Confirmation State   │ (Espera respuesta: máx 8 seg)
           └────────────────────────┘
             │                    │
       (Dice "Sí")          (Dice "No", silencia o timeout)
             ▼                    ▼
     [Ejecuta Acción]     [Cancela y responde: "Acción cancelada."]
```

---

### Criterios de Aceptación
1. **Clasificación de acciones de riesgo:** Las herramientas marcadas con `requires_confirmation = True` detienen su ejecución inmediata y pasan al estado de confirmación.
2. **Pregunta hablada específica:** El asistente formula una pregunta clara describiendo el impacto exacto de la acción a realizar.
3. **Ventana de escucha acotada:** El sistema abre una ventana de escucha temporal de 8 segundos esperando exclusivamente la confirmación del usuario.
4. **Respuestas afirmativas aceptadas:** Se reconocen como confirmación únicamente variantes explícitas en español ("sí", "confirmo", "adelante", "procede").
5. **Respuestas de cancelación:** Si el usuario responde con palabras negativas ("no", "cancela", "espera", "pará"), la acción se aborta y se confirma verbalmente la cancelación.
6. **Cancelación por timeout:** Si transcurren los 8 segundos sin respuesta clara del usuario, la acción se descarta automáticamente por seguridad.
7. **No reintento acumulativo:** Una orden no confirmada o cancelada no queda encolada ni se ejecuta en turnos posteriores.
8. **Pruebas automatizadas del flujo:** Se cuenta con tests que simulan las tres ramas: confirmación exitosa, rechazo explícito y expiración por timeout.
