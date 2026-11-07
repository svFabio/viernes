# HU-RUST-10: Streaming y tokenización de oraciones en tiempo real para síntesis incremental

**Puntos de Historia:** 5 | **Prioridad:** High | **Versión:** 1 | **Asignado a:** Agente 2 (Brain & Coordinator)

---

### Descripción
El usuario escucha las respuestas habladas en streaming continuo oración por oración sin esperar a que el modelo termine de redactar párrafos enteros.

---

### Mockup / Flujo de Interacción
```text
[Stream de Tokens de Antigravity]
                 │
                 ▼
     ┌────────────────────────┐
     │   SentenceTokenizer    │ <── Detecta signos de puntuación (. ! ? \n)
     └────────────────────────┘
                 │
     ┌───────────┴───────────┐
     ▼                       ▼
[Oración 1 lista]       [Oración 2 lista]
     │                       │
     ▼                       ▼
[Enviar a TTS en paralelo con la generación del resto del texto]
```

---

### Criterios de Aceptación
1. **Buffer acumulador inteligente:** Acumula tokens entrantes y segmenta el texto cuando detecta signos terminadores válidos (`.`, `?`, `!`, `
`).
2. **Protección de abreviaturas:** No corta oraciones en abreviaturas comunes (ej. `Sr.`, `Dr.`, `etc.`, números con decimales `3.14`).
3. **Longitud mínima de emisión:** Fragmentos inferiores a 10 caracteres no se emiten hasta acumular la siguiente cláusula para evitar audio entrecortado.
4. **Vaciado al finalizar:** Al recibir la señal de fin de generación, emite cualquier texto remanente en el buffer.
5. **Canal pipelined hacia TTS:** Cada oración completada se envía inmediatamente a la cola de síntesis sin bloquear el parser de tokens.
6. **Pruebas con texto complejo:** Pruebas unitarias validan la tokenización correcta en diálogos técnicos, código embebido y listas numeradas.

---

### Subtareas Técnicas
- **ST-RUST-10.1:** Implementar SentenceTokenizer con reglas defensivas de puntuación y números
- **ST-RUST-10.2:** Conectar el stream de tokens al pipeline productor-consumidor hacia TTS
- **ST-RUST-10.3:** Pruebas de borde de tokenización con signos ortográficos en español
