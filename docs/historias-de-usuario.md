# Historias de Usuario — Proyecto Viernes (09-viernes)

Especificación completa del backlog de desarrollo para el asistente de voz modular **Viernes**, elaborada conforme al estándar metodológico de TIS (Taller de Ingeniería de Software) con granularidad atómica y criterios de aceptación exhaustivos.

---

## Matriz General del Backlog

| ID | Título de la Historia de Usuario | Módulo | Puntos (Fibonacci) | Prioridad | Archivo de Especificación |
|:---|:---|:---|:---:|:---:|:---|
| **HU-01** | Sintetizar respuesta hablada con voz neural de ElevenLabs | Voz / TTS | 3 | High | [HU-01](hus/HU-01-sintetizar-voz-elevenlabs.md) |
| **HU-02** | Conmutar automáticamente a síntesis local con Piper por agotamiento de cuota (Circuit Breaker) | Voz / TTS | 5 | Highest | [HU-02](hus/HU-02-conmutar-fallback-piper.md) |
| **HU-03** | Configurar voz y credenciales de ElevenLabs y Piper vía `.env` | Infra / Config | 2 | Medium | [HU-03](hus/HU-03-configurar-credenciales-env.md) |
| **HU-04** | Reproducir audio generado en segundo plano sin congelar la aplicación | Audio / Output | 3 | High | [HU-04](hus/HU-04-reproducir-audio-sin-bloquear.md) |
| **HU-05** | Capturar audio de voz desde el micrófono con detección de actividad (VAD) | Audio / STT | 5 | Highest | [HU-05](hus/HU-05-capturar-audio-microfono-vad.md) |
| **HU-06** | Transcribir segmento de voz a texto utilizando Whisper local | Audio / STT | 5 | High | [HU-06](hus/HU-06-transcribir-voz-whisper-local.md) |
| **HU-07** | Activar captura de audio mediante atajo global Push-to-Talk en Linux | Entrada / UI | 3 | Medium | [HU-07](hus/HU-07-atajo-push-to-talk.md) |
| **HU-08** | Configurar dispositivo de entrada de micrófono y umbrales de captura | Audio / Config | 2 | Low | [HU-08](hus/HU-08-configurar-dispositivos-audio.md) |
| **HU-09** | Procesar intención del usuario mediante el agente Antigravity | Cerebro / Agente | 5 | Highest | [HU-09](hus/HU-09-procesar-intencion-antigravity.md) |
| **HU-10** | Ejecutar comandos del sistema permitidos mediante herramientas MCP | Seguridad / Tools | 5 | Highest | [HU-10](hus/HU-10-ejecutar-comandos-mcp.md) |
| **HU-11** | Solicitar confirmación explícita por voz antes de ejecutar acciones destructivas | Seguridad / UX | 3 | High | [HU-11](hus/HU-11-confirmacion-voz-acciones-destructivas.md) |
| **HU-12** | Notificar fallos de comandos o servicios externos mediante mensaje hablado de error | UX / Brain | 2 | Medium | [HU-12](hus/HU-12-notificar-fallos-servicios-por-voz.md) |
| **HU-13** | Streaming de respuesta por oraciones para reducir latencia de síntesis | Pipeline / Streaming | 5 | High | [HU-13](hus/HU-13-streaming-respuesta-por-oraciones.md) |
| **HU-14** | Interrumpir reproducción de audio al detectar nueva voz del usuario (Barge-in) | Pipeline / Audio | 5 | Highest | [HU-14](hus/HU-14-interrumpir-reproduccion-barge-in.md) |
| **HU-15** | Ejecutar el asistente como servicio de usuario en segundo plano (`systemd --user`) | Infra / Linux | 3 | Medium | [HU-15](hus/HU-15-servicio-systemd-usuario.md) |

---

## Distribución de Carga y Estimación
- **Total de Historias de Usuario:** 15 HUs atómicas
- **Total de Puntos de Historia:** 56 Story Points
- **Historias de Prioridad Highest / Críticas:** 5 HUs (HU-02, HU-05, HU-09, HU-10, HU-14)
- **Historias de Prioridad High:** 5 HUs (HU-01, HU-04, HU-06, HU-11, HU-13)
- **Historias de Prioridad Medium:** 4 HUs (HU-03, HU-07, HU-12, HU-15)
- **Historias de Prioridad Low:** 1 HU (HU-08)
