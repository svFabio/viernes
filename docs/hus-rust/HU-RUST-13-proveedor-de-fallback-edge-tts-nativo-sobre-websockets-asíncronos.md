# HU-RUST-13: Proveedor de fallback Edge-TTS nativo sobre WebSockets asíncronos

**Puntos de Historia:** 8 | **Prioridad:** Highest | **Versión:** 1 | **Asignado a:** Agente 3 (TTS & Audio Output)

---

### Descripción
El usuario nunca se queda sin respuesta hablada gracias a un motor de respaldo Edge-TTS 100% gratuito basado en WebSockets cuando las claves de ElevenLabs se agotan o no hay conexión.

---

### Mockup / Flujo de Interacción
```text
[Fallo en ElevenLabs / Sin cuota]
                   │
                   ▼
       ┌────────────────────────┐
       │   EdgeTtsClient        │ <── Conexión WSS a speech.platform.bing.com
       └────────────────────────┘
                   │ (Protocolo SSML sobre WebSocket binario)
                   ▼
       [Extracción de tramas MP3 binarias]
                   │
                   ▼
       [Voz sintetizada con éxito sin interrumpir la experiencia]
```

---

### Criterios de Aceptación
1. **Conexión WebSocket segura:** Establece conexión WSS hacia los endpoints de voz de Microsoft Edge mediante `tokio-tungstenite`.
2. **Generación de paquetes SSML:** Construye mensajes SSML con parámetros de voz en español neutro (`es-ES-ElviraNeural` o similar).
3. **Decodificación de paquetes mixtos:** Parsea tramas de texto de metadatos y extrae los payloads binarios de audio MP3.
4. **Conmutación transparente:** El fallback se activa automáticamente en < 200 ms si el proveedor primario falla.
5. **Manejo de desconexión:** Si el WebSocket se cierra abruptamente, reconecta automáticamente o lanza error tipado.
6. **Cero dependencias de Python:** Implementado puramente en Rust sin llamar a subprocesos externos de Python.
7. **Pruebas de síntesis:** Pruebas unitarias y de integración verifican la producción de streams de audio MP3 válidos.

---

### Subtareas Técnicas
- **ST-RUST-13.1:** Implementar handshake WSS y empaquetador SSML con tokio-tungstenite
- **ST-RUST-13.2:** Implementar parser de tramas binarias de audio MP3
- **ST-RUST-13.3:** Tests de conmutación automática y robustez ante cortes de conexión
