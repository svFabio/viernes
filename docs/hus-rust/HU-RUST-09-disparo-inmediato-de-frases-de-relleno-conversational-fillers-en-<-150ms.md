# HU-RUST-09: Disparo inmediato de frases de relleno (conversational fillers) en < 150ms

**Puntos de Historia:** 5 | **Prioridad:** High | **Versión:** 1 | **Asignado a:** Agente 2 (Brain & Coordinator)

---

### Descripción
El usuario escucha una confirmación hablada inmediata (ej. 'A la orden, Señor') en menos de 150 ms para enmascarar la latencia de red de los modelos de lenguaje mientras procesan la consulta.

---

### Mockup / Flujo de Interacción
```text
[Evento VoiceFinished]
          │
          ├─────────────────────────────────────────┐
          ▼ (Inmediato < 150ms)                     ▼ (Async)
┌────────────────────────┐              ┌────────────────────────┐
│  Reproducir Filler     │              │  Consultar Antigravity │
│  pre-sintetizado WAV   │              │  (Red: 600ms - 1500ms) │
└────────────────────────┘              └────────────────────────┘
          │                                         │
          ▼                                         ▼
[Usuario oye confirmación]              [Llega respuesta final]
```

---

### Criterios de Aceptación
1. **Caché pre-sintetizado:** El sistema mantiene en memoria o disco archivos PCM/WAV pre-cacheados de frases inmediatas, de herramientas y de paciencia.
2. **Disparo ultra-rápido:** Al detectarse fin de voz, se despacha la reproducción de una frase de relleno aleatoria en < 150 ms.
3. **Categorización por herramienta:** Si la respuesta invoca herramientas de archivos, terminal o búsqueda web, se vocaliza una frase representativa de la categoría.
4. **Temporizador de paciencia:** Si la respuesta de Antigravity supera los 6.5 segundos, se dispara automáticamente una frase de paciencia ('Sigo procesando su consulta...').
5. **Personalización desde JSON:** Permite cargar frases personalizadas desde `~/.config/viernes/fillers.json` con recarga dinámica.
6. **Pruebas de latencia:** Pruebas automatizadas miden que el tiempo transcurrido entre `VoiceFinished` y el inicio del audio de filler sea inferior a 150 ms.

---

### Subtareas Técnicas
- **ST-RUST-09.1:** Implementar FillerCacheManager y carga desde ~/.config/viernes/fillers.json
- **ST-RUST-09.2:** Implementar dispatcher de fillers con temporizadores de paciencia concurrentes
- **ST-RUST-09.3:** Pruebas unitarias de selección aleatoria y verificación de benchmarks de latencia
