# HU-12: Notificar fallos de comandos o servicios externos mediante mensaje hablado de error

**Puntos de Historia:** 2 | **Prioridad:** Medium | **Versión:** 1 | **Asignado a:** Agente UX / Brain

---

### Descripción
El usuario escucha una explicación concisa y clara por voz cuando un comando, herramienta o servicio externo falla, en lugar de experimentar un silencio confuso o un cuelgue inesperado del sistema.

---

### Mockup / Flujo de Interacción
```text
[Falla de servicio externo: ej. Error 503 en API meteorológica]
                              │
                              ▼
                   ┌───────────────────────┐
                   │   Error Speech Gate   │
                   └───────────────────────┘
                              │
                              ▼
Viernes (Voz): "No pude consultar el clima en este momento porque el servicio no responde."
                              │
                              ▼
                [Retorno a estado IDLE limpio]
```

---

### Criterios de Aceptación
1. **Captura global de excepciones de herramientas:** Cualquier excepción levantada durante la ejecución de una herramienta o comando es capturada por el orquestador.
2. **Traducción a lenguaje natural:** El mensaje técnico del error (stack trace o códigos HTTP) se traduce a una frase breve y comprensible para el usuario, sin tecnicismos innecesarios.
3. **Síntesis y reproducción hablada:** El mensaje de error formulado se sintetiza a través del servicio de TTS (usando el fallback de Piper si la falla es de conectividad) y se reproduce por los altavoces.
4. **No bloqueo del hilo principal:** Tras emitir el aviso de error, el asistente libera todos los locks y retorna de inmediato al estado de reposo (`IDLE`), listo para la siguiente orden.
5. **Log técnico detallado:** El detalle completo del error y el stack trace se registran en los logs del sistema (`journalctl` / archivo de logs) para diagnóstico por parte del desarrollador.
6. **Pruebas de captura:** Pruebas unitarias confirman que ante excepciones simuladas en tools, el sistema nunca crashea y genera el audio explicativo correspondiente.
