# HU-06: Transcribir segmento de voz a texto utilizando Whisper local

**Puntos de Historia:** 5 | **Prioridad:** High | **Versión:** 1 | **Asignado a:** Agente STT

---

### Descripción
El usuario obtiene la transcripción textual exacta de sus comandos de voz procesados de forma 100% local y privada mediante Whisper (ej. `faster-whisper`), sin transmitir señales de audio hacia servidores externos ni incurrir en costos de APIs cloud.

---

### Mockup / Flujo de Interacción
```text
[Archivo WAV temporal] ──► ┌────────────────────────┐
                           │   WhisperSTTProvider   │ <─── [Modelo local 'base' o 'small']
                           └────────────────────────┘
                                       │
                                       ▼
                       "Viernes, ¿cuál es el clima hoy?"
                                       │
                                       ▼
                             [Envío a Antigravity]
```

---

### Criterios de Aceptación
1. **Interfaz desacoplada:** El componente implementa la interfaz abstracta `STTProvider` con el método `transcribe(audio_path: Path) -> str`.
2. **Carga del modelo local:** Al inicializar el proveedor, se carga el modelo Whisper especificado (`tiny`, `base` o `small`) ejecutándose en CPU o GPU local mediante `faster-whisper`.
3. **Soporte de idioma español:** La transcripción está configurada por defecto para detectar y transcribir en idioma español (`language="es"`), o en modo automático multilingüe si se parametriza.
4. **Filtrado de transcripciones vacías:** Si el archivo de audio solo contiene silencio o ruido residual ininteligible, el método retorna una cadena vacía `""` sin lanzar excepciones.
5. **Normalización de texto:** El texto transcripto se devuelve limpio, eliminando espacios en blanco redundantes al inicio y final (`strip()`).
6. **Manejo de archivo inexistente:** Si la ruta del archivo WAV no existe en disco, se lanza `FileNotFoundError` con un mensaje claro.
7. **Proveedor Fake para tests:** Existe una implementación `FakeSTTProvider` que retorna textos predeterminados para pruebas continuas sin requerir la carga pesada de modelos neuronales en CI.
8. **Pruebas unitarias de transcripción:** Se verifica con tests automatizados que el mock y el pipeline de transcripción procesen correctamente archivos de audio WAV de muestra.
