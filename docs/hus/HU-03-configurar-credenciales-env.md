# HU-03: Configurar voz y credenciales de ElevenLabs y Piper vía archivo de entorno

**Puntos de Historia:** 2 | **Prioridad:** Medium | **Versión:** 1 | **Asignado a:** Agente Infraestructura

---

### Descripción
El usuario define sus credenciales (API Key de ElevenLabs), identificadores de voz y rutas de modelos locales en un archivo `.env` para personalizar el asistente sin exponer secretos en el código fuente ni en el repositorio de control de versiones.

---

### Mockup / Flujo de Interacción
```text
.env (Ignorado por git)
├── ELEVENLABS_API_KEY=sk_live_...
├── ELEVENLABS_VOICE_ID=21m00Tcm4TlvDq8ikWAM
├── PIPER_BIN=piper
├── PIPER_MODEL_PATH=/home/svfabio/models/es_ES-davefx-medium.onnx
└── VIERNES_TEMP_DIR=/tmp/viernes
```

---

### Criterios de Aceptación
1. **Archivo de plantilla:** El repositorio contiene una plantilla `.env.example` con todas las variables requeridas y valores vacíos o sugeridos por defecto.
2. **Ignorado en Git:** El archivo `.env` está explicitamente listado en `.gitignore` y nunca se commitea al repositorio.
3. **Carga automática:** Al iniciar el asistente, el módulo `config.py` carga las variables de `.env` utilizando `python-dotenv`.
4. **Valores por defecto seguros:** Si `ELEVENLABS_VOICE_ID` no está definida en `.env`, el sistema utiliza la voz estándar "Rachel" (`21m00Tcm4TlvDq8ikWAM`).
5. **Directorio temporal configurable:** Si no se especifica `VIERNES_TEMP_DIR`, se utiliza `/tmp/viernes` como ruta por defecto, asegurando su creación con permisos de usuario.
6. **Validación de presencia de claves:** Si el usuario no proporciona `ELEVENLABS_API_KEY`, el sistema no crashea al arrancar; emite una advertencia informativa y configura a Piper como proveedor único por defecto.
7. **Redacción de secretos:** En salidas de logs, auditoría o mensajes de error, el valor de `ELEVENLABS_API_KEY` se muestra enmascarado (ej. `sk_***1234`).
