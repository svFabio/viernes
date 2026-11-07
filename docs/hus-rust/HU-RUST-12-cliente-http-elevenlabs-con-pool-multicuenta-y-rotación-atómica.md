# HU-RUST-12: Cliente HTTP ElevenLabs con Pool Multicuenta y rotación atómica

**Puntos de Historia:** 8 | **Prioridad:** Highest | **Versión:** 1 | **Asignado a:** Agente 3 (TTS & Audio Output)

---

### Descripción
El usuario escucha voces ultra-realistas de ElevenLabs utilizando un pool multicuenta con rotación automática transparente ante agotamiento de cuotas gratuitas.

---

### Mockup / Flujo de Interacción
```text
[Texto: "A la orden, Señor."]
                   │
                   ▼
       ┌────────────────────────┐
       │ KeyPoolElevenLabsClient│ <── Lista de API Keys en .env
       └────────────────────────┘
                   │
       ┌───────────┴───────────┐
       ▼ (200 OK)              ▼ (401 / 429 Quota Exceeded)
[Retornar bytes MP3]     [Rotación atómica a siguiente Key en el Pool]
                               │
                               ▼
                         [Reintentar petición sin error perceptible]
```

---

### Criterios de Aceptación
1. **Cliente HTTP asíncrono reqwest:** Configura un cliente HTTP con connection pooling, TLS nativo y compresión de tramas.
2. **Carga y parsing de pool de claves:** Parsea `ELEVENLABS_API_KEYS` permitiendo una o múltiples claves separadas por coma.
3. **Rotación atómica thread-safe:** El índice de clave activa se gestiona con `AtomicUsize`, permitiendo concurrencia segura sin contención de locks.
4. **Detección de 401 y 429:** Ante respuestas HTTP 401 (Unauthorized) o 429 (Too Many Requests), marca la clave como agotada y rota inmediatamente a la siguiente.
5. **Sin reintentos infinitos:** Si todas las claves del pool fallan, emite un error estructurado `AllKeysExhausted` para activar el fallback a Edge-TTS.
6. **Seguridad de credenciales:** Ninguna clave se registra en logs o salidas de consola.
7. **Mock HTTP Server:** Pruebas automatizadas con servidor HTTP simulado (`wiremock`) validan la rotación ante códigos 429.

---

### Subtareas Técnicas
- **ST-RUST-12.1:** Implementar cliente HTTP reqwest para síntesis de ElevenLabs con streaming MP3
- **ST-RUST-12.2:** Implementar lógica de rotación atómica KeyPool con AtomicUsize
- **ST-RUST-12.3:** Tests con wiremock simulando fallos de cuota y rotación de claves
