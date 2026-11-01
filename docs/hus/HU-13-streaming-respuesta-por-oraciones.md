# HU-13: Streaming de respuesta por oraciones para reducir latencia de síntesis

**Puntos de Historia:** 5 | **Prioridad:** High | **Versión:** 1 | **Asignado a:** Agente Pipeline / Tiempo Real

---

### Descripción
El usuario comienza a escuchar la respuesta hablada en menos de un segundo gracias al corte y sintetizado en paralelo de la primera oración generada por el cerebro, sin necesidad de esperar a que se complete todo el párrafo de respuesta.

---

### Mockup / Flujo de Interacción
```text
Cerebro (Token stream): "Hola. [corte 1] Hoy tienes 3 tareas pendientes en tu lista. [corte 2] ..."
                             │                       │
                             ▼                       ▼
                     TTS Oración 1           TTS Oración 2
                             │                       │
                             ▼                       ▼
                     Audio Chunk 1           Audio Chunk 2
                             │                       │
                             ▼                       ▼
                   [Reproduce Chunk 1] ──► [Encola y reproduce Chunk 2]
```

---

### Criterios de Aceptación
1. **Segmentador de texto por puntuación:** El pipeline cuenta con un buffer de streaming que detecta delimitadores de oraciones completas (`.`, `!`, `?`, `\n`) a medida que llegan los tokens del LLM.
2. **Despacho temprano al TTS:** En cuanto se completa la primera oración (con un mínimo de 3 palabras), el texto se despacha inmediatamente al proveedor TTS en un hilo/tarea asíncrona independiente.
3. **Reproducción encadenada:** Mientras el reproductor de audio emite el primer chunk de voz, el segundo chunk se está sintetizando en paralelo en segundo plano.
4. **Cola de reproducción fluida:** El reproductor mantiene una cola secuencial (`Queue`) sin solapamientos de audio ni vacíos perceptibles entre oraciones.
5. **Manejo de oraciones cortas:** Puntuaciones que formen abreviaturas comunes (ej. `Dr.`, `Sr.`, `ej.`, `p.ej.`) no provocan un corte de oración anticipado.
6. **Métrica de latencia inicial:** El tiempo transcurrido desde que el usuario termina de hablar hasta que se emite el primer sonido de voz se mide y registra en logs (objetivo < 1.2 segundos).
7. **Pruebas de concurrencia:** Tests automatizados con streams de tokens simulados verifican que las oraciones se procesen y reproduzcan en el orden correcto sin pérdidas de texto.
