# HU-02: Conmutar automáticamente a síntesis local con Piper por agotamiento de cuota (Circuit Breaker)

**Puntos de Historia:** 5 | **Prioridad:** Highest | **Versión:** 1 | **Asignado a:** Agente Resiliencia / TTS

---

### Descripción
El usuario continúa escuchando las respuestas habladas del asistente mediante el motor local offline Piper cuando se produce un error de cuota o rate limit (HTTP 429) en ElevenLabs, garantizando continuidad de servicio sin fallos ni costos imprevistos.

---

### Mockup / Flujo de Interacción
```text
           [Solicitud de Síntesis]
                      │
                      ▼
            ┌───────────────────┐
            │   ResilientTTS    │
            └───────────────────┘
              │               │
     (Circuito CERRADO)       │ (Circuito ABIERTO o HTTP 429)
              ▼               ▼
      [ElevenLabsProvider]   [PiperProvider Local]
              │ (Error 429)           │
              └──────► Abre Circuito ─┘
```

---

### Criterios de Aceptación
1. **Detección de error de cuota:** Dado un intento de síntesis con ElevenLabs, si la API retorna código HTTP 429, `quota_exceeded`, `insufficient_credits` o mensaje equivalente, el sistema captura la excepción sin propagar el error al usuario.
2. **Apertura de circuito (Circuit Breaker):** Al detectarse el agotamiento de cuota, la bandera interna `quota_exhausted` cambia a `True`.
3. **Conmutación transparente:** Inmediatamente tras la captura del error 429, el texto es reenviado de forma transparente al proveedor secundario local (`PiperProvider`).
4. **Bypass de ElevenLabs:** Dado que el circuito está abierto (`quota_exhausted = True`), todas las solicitudes de síntesis posteriores dentro de la sesión se dirigen directamente a Piper sin invocar a ElevenLabs.
5. **Generación local válida:** Al procesar la síntesis con Piper, el sistema genera un archivo de audio WAV en disco reproducible por los altavoces.
6. **Logging estructurado de advertencia:** Al ocurrir el fallback, se registra un log de nivel `WARNING` indicando la conmutación a Piper sin exponer datos sensibles.
7. **Callback de notificación:** Si se configuró un callback `on_fallback_activated`, este es invocado exactamente una vez pasando la causa del error.
8. **Manejo de errores transitorios de red:** Si ElevenLabs falla por un error transitorio de conexión (no relacionado con cuota), la petición puntual se resuelve con Piper pero no se abre el circuito de forma permanente.
9. **Pruebas unitarias de circuito:** Se cuenta con pruebas automatizadas que verifican:
   - Que ElevenLabs no sea llamado una segunda vez tras abrir el circuito.
   - Que el audio final retorne exitosamente generado por el proveedor fallback.
