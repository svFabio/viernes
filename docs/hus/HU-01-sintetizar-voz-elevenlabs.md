# HU-01: Sintetizar respuesta hablada con voz neural de ElevenLabs

**Puntos de Historia:** 3 | **Prioridad:** High | **Versión:** 1 | **Asignado a:** Agente TTS

---

### Descripción
El usuario escucha las respuestas del asistente con voz neural de alta fidelidad mediante la API de ElevenLabs para disfrutar de una interacción conversacional fluida, natural y comprensible.

---

### Mockup / Flujo de Interacción
```text
[Texto de respuesta generado por el Cerebro]
                   │
                   ▼
       ┌────────────────────────┐
       │   ElevenLabsProvider   │ <─── [API Key & Voice ID desde .env]
       └────────────────────────┘
                   │  (Streaming chunks MP3)
                   ▼
       [Archivo temporal /tmp/viernes/chunk_01.mp3]
                   │
                   ▼
       [Reproducción nativa en altavoces]
```

---

### Criterios de Aceptación
1. **Inicialización del cliente:** Dado que la variable `ELEVENLABS_API_KEY` está configurada en `.env`, cuando el asistente inicializa el servicio TTS, se instancia el cliente oficial de ElevenLabs sin errores de autenticación.
2. **Síntesis exitosa:** Al solicitar la síntesis de un texto no vacío, el sistema envía la petición a ElevenLabs usando el modelo `eleven_multilingual_v2` y el formato `mp3_44100_128`.
3. **Persistencia en disco:** Al recibir el flujo de bytes de audio de ElevenLabs, el sistema escribe el archivo en el directorio temporal configurado (`/tmp/viernes/`) y retorna la ruta absoluta generada.
4. **Validación de texto vacío:** Si se solicita sintetizar un texto vacío o compuesto únicamente por espacios en blanco, el sistema no realiza ninguna llamada a la API y retorna un error de validación temprano.
5. **Manejo de timeout de red:** Si la llamada a la API de ElevenLabs supera un timeout configurado de 5 segundos sin recibir datos, el sistema aborta la petición y lanza una excepción descriptiva `TimeoutError`.
6. **Mapeo de Voice ID:** Al realizar la síntesis, el sistema utiliza el `ELEVENLABS_VOICE_ID` indicado en la configuración, o el identificador por defecto de la voz en caso de no especificarse.
7. **No exposición de claves:** En ningún log de ejecución ni mensaje de consola se imprime el valor de `ELEVENLABS_API_KEY`.
8. **Pruebas unitarias:** El módulo cuenta con pruebas unitarias usando fakes/mocks que simulan la respuesta exitosa de ElevenLabs sin consumir cuota real de la API.
