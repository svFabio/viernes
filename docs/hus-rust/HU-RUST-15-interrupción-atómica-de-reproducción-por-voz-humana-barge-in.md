# HU-RUST-15: Interrupción atómica de reproducción por voz humana (Barge-in)

**Puntos de Historia:** 8 | **Prioridad:** Highest | **Versión:** 1 | **Asignado a:** Agente 3 (TTS & Audio Output)

---

### Descripción
El usuario interrumpe a Viernes en cualquier momento mientras está hablando simplemente empezando a hablar, logrando que el asistente calle al instante sin retrasos.

---

### Mockup / Flujo de Interacción
```text
[Viernes reproduciendo respuesta larga...]
                   │
                   ▼
       [Usuario empieza a hablar: "Para Viernes"]
                   │
                   ▼
       [VAD emite evento VoiceStarted]
                   │
                   ▼
       ┌────────────────────────┐
       │   Atomic Kill-Switch   │ ──► rodio_sink.stop() en < 80ms
       └────────────────────────┘
                   │
       ┌───────────┴───────────┐
       ▼                       ▼
[Buffer vaciado de golpe]   [Estado vuelve a Listening]
```

---

### Criterios de Aceptación
1. **Kill-Switch atómico:** Cuenta con un flag `AtomicBool` y `CancellationToken` de Tokio para abortar la reproducción activa en < 80 ms.
2. **Vaciado inmediato de hardware:** Al recibir la señal de interrupción, se vacía inmediatamente el buffer de hardware de audio (`sink.stop()`).
3. **Cancelación de peticiones TTS pendientes:** Cualquier petición de red en curso para oraciones futuras se aborta de inmediato para ahorrar cuota.
4. **Transición limpia a Listening:** El sistema transiciona de forma determinista a `Listening`, canalizando el nuevo audio del usuario al STT.
5. **Protección anti-eco (Acoustic Echo):** Si se utiliza altavoz abierto, se implementa una ventana de gracia para evitar que el propio audio de Viernes dispare falsos barge-ins.
6. **Pruebas de latencia de corte:** Pruebas automatizadas garantizan que el tiempo entre `VoiceStarted` y el cese de audio sea inferior a 80 ms.

---

### Subtareas Técnicas
- **ST-RUST-15.1:** Implementar mecanismo de corte con AtomicBool y CancellationToken
- **ST-RUST-15.2:** Vincular evento VAD VoiceStarted con cancelación de TTS y vaciado de sink
- **ST-RUST-15.3:** Pruebas de latencia de interrupción y descarte de cola de síntesis
