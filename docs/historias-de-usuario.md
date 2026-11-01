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
| **HU-16** | Gestionar credenciales de ElevenLabs y catálogo de voces desde la interfaz gráfica | Frontend / GUI | 3 | High | [HU-16](hus/HU-16-gestionar-credenciales-voz-gui.md) |
| **HU-17** | Configurar hardware de audio y calibrar sensibilidad VAD desde la interfaz gráfica | Audio / Hardware | 3 | High | [HU-17](hus/HU-17-configurar-hardware-audio-vad-gui.md) |
| **HU-18** | Controlar el ciclo de vida del servicio systemd e inspeccionar su estado desde la interfaz gráfica | Infra / Linux | 3 | Medium | [HU-18](hus/HU-18-control-servicio-systemd-gui.md) |
| **HU-19** | Audición y prueba de voz neural interactiva con prevención de condiciones de carrera | TTS / Audio | 3 | High | [HU-19](hus/HU-19-audicion-prueba-voz-gui.md) |
| **HU-20** | Auditoría y resolución de defectos de concurrencia, notificaciones apiladas y recarga de configuración | QA / Core | 5 | Highest | [HU-20](hus/HU-20-auditoria-defectos-concurrencia-notificaciones.md) |

---

## Distribución de Carga y Estimación
- **Total de Historias de Usuario:** 20 HUs atómicas
- **Total de Puntos de Historia:** 73 Story Points
- **Historias de Prioridad Highest / Críticas:** 6 HUs (HU-02, HU-05, HU-09, HU-10, HU-14, HU-20)
- **Historias de Prioridad High:** 8 HUs (HU-01, HU-04, HU-06, HU-11, HU-13, HU-16, HU-17, HU-19)
- **Historias de Prioridad Medium:** 5 HUs (HU-03, HU-07, HU-12, HU-15, HU-18)
- **Historias de Prioridad Low:** 1 HU (HU-08)
