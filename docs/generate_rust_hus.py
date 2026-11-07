"""Generates the 20 comprehensive User Stories for the Viernes Rust Migration."""

from pathlib import Path

BASE_DIR = Path("/home/svfabio/Projects/09-viernes/docs/hus-rust")
BASE_DIR.mkdir(parents=True, exist_ok=True)

HUS = [
    # -------------------------------------------------------------------------
    # AGENTE 1: Audio Input & STT ("Las Orejas")
    # -------------------------------------------------------------------------
    {
        "id": "HU-RUST-01",
        "title": "Capturar audio de micrófono mediante cpal con buffer lock-free y fallback de sample rate",
        "points": 5,
        "priority": "Highest",
        "version": 1,
        "agent": "Agente 1 (Audio Input & STT)",
        "epic": "epic:B",
        "risk": "risk:alto",
        "desc": "El usuario habla de manera natural frente al micrófono del sistema mediante el capturador nativo cpal para registrar su señal de voz con latencia sub-milisegundo y sin degradación de rendimiento.",
        "flow": """[Micrófono Hardware (ALSA / PipeWire)]
                     │
                     ▼
         ┌─────────────────────────┐
         │ cpal AudioCaptureStream │ <── Fallback automático de sample rate
         └─────────────────────────┘
                     │ (Float32 PCM Ring-Buffer)
                     ▼
         ┌─────────────────────────┐
         │  crossbeam lock-free    │
         └─────────────────────────┘
                     │
                     ▼
         [Chunk de audio listo para VAD]""",
        "criteria": [
            "1. **Inicialización cpal:** El sistema inicializa el stream de entrada utilizando el dispositivo de captura predeterminado del host o el especificado en configuración.",
            "2. **Fallback defensivo de Sample Rate:** Si el hardware rechaza una frecuencia de 16,000 Hz, el capturador conmuta automáticamente a la frecuencia nativa del dispositivo (44.1 kHz o 48 kHz) y aplica submuestreo por software.",
            "3. **Buffer libre de bloqueos:** Las muestras de audio capturadas en el callback de tiempo real se transmiten a través de un ring-buffer lock-free (`crossbeam-channel`) sin alocaciones dinámicas en el bucle caliente.",
            "4. **Formato estándar PCM:** El stream de audio produce muestras en formato Float32 normalizadas en el rango [-1.0, 1.0].",
            "5. **Manejo de desconexión:** Si el dispositivo de audio es desconectado durante la captura, el sistema captura el error sin entrar en pánico y emite un evento `AudioDeviceDisconnected`.",
            "6. **Cero fugas de memoria:** La ejecución continua durante 1 hora de captura no incrementa el consumo de memoria residente (RSS < 30 MB).",
            "7. **Pruebas herméticas:** El módulo cuenta con un `FakeAudioSource` para ejecutar pruebas unitarias y de integración sin depender de hardware físico ni servidores de sonido.",
        ],
        "subtasks": [
            ("ST-RUST-01.1", "Diseñar abstracción AudioCaptureStream y ring-buffer concurrente con crossbeam"),
            ("ST-RUST-01.2", "Implementar algoritmo de resampleo defensivo y fallback de sample rate"),
            ("ST-RUST-01.3", "Pruebas unitarias con FakeAudioDevice y verificación de memoria con Valgrind"),
        ],
    },
    {
        "id": "HU-RUST-02",
        "title": "Detección de Actividad de Voz (VAD) en tiempo real con ventana deslizante",
        "points": 5,
        "priority": "High",
        "version": 1,
        "agent": "Agente 1 (Audio Input & STT)",
        "epic": "epic:B",
        "risk": "risk:medio",
        "desc": "El usuario modula su voz libremente mediante el detector de actividad de voz en tiempo real para que el asistente diferencie automáticamente cuándo está hablando y cuándo se detiene.",
        "flow": """[Stream Float32 PCM] ──► ┌─────────────────────────┐
                           │    VoiceActivityDetector│ <── Umbral RMS & Silencio
                           └─────────────────────────┘
                                        │
                         ┌──────────────┴──────────────┐
                         ▼                             ▼
                [Evento VoiceStarted]        [Evento VoiceFinished]
                         │                             │
                         ▼                             ▼
                [Activar Grabación]          [Enviar Buffer a STT]""",
        "criteria": [
            "1. **Cálculo RMS en ventana deslizante:** El sistema procesa tramas de 20 ms a 30 ms calculando la energía RMS (Root Mean Square) en tiempo real.",
            "2. **Disparo de inicio de voz:** Cuando la energía supera el umbral configurado (`VAD_THRESHOLD`) durante al menos 2 tramas consecutivas, se emite el evento `VoiceStarted`.",
            "3. **Detección de silencio sostenido:** Cuando la energía cae por debajo del umbral durante un tiempo acumulado (`SILENCE_TIMEOUT_MS`, ej. 800 ms), se emite el evento `VoiceFinished` con el buffer completo.",
            "4. **Rechazo de chasquidos residuales:** Muestras de audio con duración de voz inferior a 250 ms son descartadas como ruido espurio.",
            "5. **Límite máximo de seguridad:** Si el usuario habla continuamente por más de 20 segundos sin pausas, el VAD fuerza el corte y emite el audio acumulado para evitar buffers infinitos.",
            "6. **Ajuste dinámico:** El umbral de sensibilidad puede reconfigurarse en caliente sin recrear el stream de audio.",
            "7. **Pruebas con audio sintético:** Pruebas unitarias verifican la detección precisa de voz y silencio con archivos de onda pregrabados.",
        ],
        "subtasks": [
            ("ST-RUST-02.1", "Implementar cálculo vectorial RMS con ventana deslizante y máquina de estados VAD"),
            ("ST-RUST-02.2", "Implementar temporizador de silencio y límite de seguridad de 20 segundos"),
            ("ST-RUST-02.3", "Pruebas unitarias de segmentación con datasets de audio y casos borde"),
        ],
    },
    {
        "id": "HU-RUST-03",
        "title": "Transcribir segmentos de voz a texto localmente con whisper-rs",
        "points": 8,
        "priority": "Highest",
        "version": 1,
        "agent": "Agente 1 (Audio Input & STT)",
        "epic": "epic:B",
        "risk": "risk:alto",
        "desc": "El usuario obtiene la transcripción textual exacta de sus comandos hablados de forma 100% privada y local mediante bindings optimizados de Whisper para ejecutar instrucciones sin depender de APIs cloud.",
        "flow": """[Buffer PCM Float32 (16kHz)]
                   │
                   ▼
       ┌────────────────────────┐
       │   WhisperTranscriber   │ <── Modelo local whisper.bin (tiny/base)
       └────────────────────────┘
                   │  (Inferencia nativa C++/AVX)
                   ▼
       [String transcripto: "Viernes, ¿cuál es el estado?"]
                   │
                   ▼
       [Canal mpsc hacia Agente 2 Cerebro]""",
        "criteria": [
            "1. **Carga del modelo local:** El transcriptor carga modelos compatibles con `whisper.cpp` (`ggml-base.bin` o `ggml-small.bin`) desde el directorio de modelos local.",
            "2. **Aceleración por hardware:** La inferencia compila con soporte de instrucciones vectoriales nativas (AVX2 / FMA o CUDA/Vulkan si está disponible).",
            "3. **Normalización a 16 kHz mono:** El componente valida que los buffers de entrada estén a 16,000 Hz Float32 antes de llamar al contexto de inferencia.",
            "4. **Parámetros de idioma:** La transcripción se parametriza por defecto en español (`es`) con supresión de tokens no deseados.",
            "5. **Inferencia no bloqueante:** El procesamiento neuronal se ejecuta en un threadpool de cómputo (`tokio::task::spawn_blocking`), sin bloquear el runtime async.",
            "6. **Filtrado de transcripciones alucinadas:** Cadenas repetitivas o ruidos residuales (ej. `[Música]`, `Gracias`) son filtrados y retornan texto vacío.",
            "7. **MockProvider para tests:** Existe una implementación `FakeTranscriber` para la suite de CI con 0 tiempo de inicialización.",
        ],
        "subtasks": [
            ("ST-RUST-03.1", "Configurar crate whisper-rs y build script para compilación de whisper.cpp"),
            ("ST-RUST-03.2", "Implementar trait SpeechToText con spawn_blocking y normalización de texto"),
            ("ST-RUST-03.3", "Implementar FakeSpeechToText y tests de integración sin dependencias neuronales"),
        ],
    },
    {
        "id": "HU-RUST-04",
        "title": "Atajo global Push-to-Talk y corte forzado de captura",
        "points": 3,
        "priority": "Medium",
        "version": 1,
        "agent": "Agente 1 (Audio Input & STT)",
        "epic": "epic:B",
        "risk": "risk:bajo",
        "desc": "El usuario activa y desactiva la captura de voz presionando y soltando una combinación de teclas global (ej. Super+V) para interactuar sin depender exclusivamente del VAD en ambientes ruidosos.",
        "flow": """[Presionar Super+V] ──► Iniciar AudioStream y emitir evento PttStarted
           │
           ▼
[Hablar al micrófono]
           │
           ▼
[Soltar Super+V]    ──► Llamar a finish_recording() y emitir PttFinished""",
        "criteria": [
            "1. **Captura de tecla global:** El sistema escucha eventos de pulsación y liberación de teclas a nivel de sesión de usuario en Linux.",
            "2. **Inicio instantáneo:** Al presionar el atajo, el capturador comienza a recolectar muestras en < 10 ms.",
            "3. **Corte inmediato:** Al soltar la tecla, se invoca `finish_recording()`, cerrando el buffer de inmediato sin esperar tiempos de silencio.",
            "4. **Concurrencia segura:** Si el VAD estaba procesando un segmento, el comando Push-to-Talk toma precedencia determinista.",
            "5. **Desactivación limpia:** Si el daemon se detiene, los listeners globales de teclado se liberan limpiamente.",
            "6. **Pruebas unitarias:** Se valida la simulación de secuencias de presión y liberación con despacho de eventos.",
        ],
        "subtasks": [
            ("ST-RUST-04.1", "Implementar listener de eventos de teclado para Wayland/X11"),
            ("ST-RUST-04.2", "Vincular evento key-up con finish_recording() en el pipeline de captura"),
            ("ST-RUST-04.3", "Tests de carrera y precedencia entre VAD y Push-to-Talk"),
        ],
    },
    {
        "id": "HU-RUST-05",
        "title": "Enumeración y selección segura de dispositivos de audio ALSA/PipeWire",
        "points": 3,
        "priority": "Medium",
        "version": 1,
        "agent": "Agente 1 (Audio Input & STT)",
        "epic": "epic:B",
        "risk": "risk:bajo",
        "desc": "El usuario visualiza y selecciona dispositivos de audio físicos o virtuales de su sistema mediante una utilidad de resolución robusta para garantizar que el micrófono y los altavoces correctos sean utilizados.",
        "flow": """[Host ALSA / PipeWire / PulseAudio]
                   │
                   ▼
       ┌────────────────────────┐
       │   DeviceEnumerator     │
       └────────────────────────┘
                   │
       ┌───────────┴───────────┐
       ▼                       ▼
[Lista de Micrófonos]     [Lista de Altavoces]
       │                       │
       ▼                       ▼
[Resolución segura con coincidencia parcial o índice numérico]""",
        "criteria": [
            "1. **Enumeración completa:** El sistema lista todos los dispositivos de entrada y salida disponibles en el host.",
            "2. **Resolución por índice o nombre:** Permite resolver un dispositivo mediante su índice numérico (ej. `1`), su nombre exacto o una coincidencia parcial insensible a mayúsculas.",
            "3. **Fallback a dispositivo por defecto:** Si el identificador configurado no existe o es vacío, el sistema selecciona silenciosamente el dispositivo predeterminado del sistema.",
            "4. **Aislamiento de variables de entorno:** La resolución lee `AUDIO_INPUT_DEVICE` y `AUDIO_OUTPUT_DEVICE` respetando entornos herméticos de prueba.",
            "5. **Formateo legible:** La salida de depuración imprime índices, nombres y canales máximos admitidos.",
            "6. **Pruebas con listas simuladas:** Pruebas unitarias verifican la selección en escenarios de dispositivos ausentes, índices fuera de rango y nombres ambiguos.",
        ],
        "subtasks": [
            ("ST-RUST-05.1", "Implementar módulo de enumeración de dispositivos con cpal"),
            ("ST-RUST-05.2", "Implementar algoritmo de resolución con fallback a default"),
            ("ST-RUST-05.3", "Tests unitarios de resolución con fixtures de dispositivos simulados"),
        ],
    },

    # -------------------------------------------------------------------------
    # AGENTE 2: Brain & Pipeline Coordinator ("El Cerebro")
    # -------------------------------------------------------------------------
    {
        "id": "HU-RUST-06",
        "title": "Comunicación asíncrona NDJSON vía subproceso streaming con Antigravity",
        "points": 8,
        "priority": "Highest",
        "version": 1,
        "agent": "Agente 2 (Brain & Coordinator)",
        "epic": "epic:C",
        "risk": "risk:alto",
        "desc": "El usuario interactúa con la inteligencia de Antigravity mediante un cliente de subproceso NDJSON asíncrono para recibir razonamientos, respuestas habladas y llamadas a herramientas en tiempo real.",
        "flow": """[Prompt transcripto: "Hola Viernes"]
                   │
                   ▼
       ┌────────────────────────┐
       │ AntigravityProcessPipe │ <── Subproceso tokio::process (stdin/stdout)
       └────────────────────────┘
                   │
                   ▼  (Líneas NDJSON en tiempo real)
       [Línea: {"type":"thought", "content":"Pensando..."}]
       [Línea: {"type":"text_chunk", "content":"Hola Señor."}]
       [Línea: {"type":"tool_call", "name":"run_command"}]
                   │
                   ▼
       [Despacho de eventos al pipeline de Viernes]""",
        "criteria": [
            "1. **Spawn asíncrono del daemon:** El cliente inicia el proceso `antigravity-cli` con pipes no bloqueantes en `stdin`, `stdout` y `stderr`.",
            "2. **Streaming línea por línea:** Utiliza `tokio::io::BufReader` y `LinesStream` para procesar cada objeto JSON sin buffering masivo en memoria.",
            "3. **Deserialización tipada con Serde:** Cada mensaje NDJSON se deserializa en un enum tipado `BrainEvent` con validación estricta de esquemas.",
            "4. **Manejo de errores de proceso:** Si el subproceso falla o termina prematuramente con código de error, el cliente captura el stderr y notifica la anomalía.",
            "5. **Timeout de inactividad:** Si transcurren más de 15 segundos sin recibir tokens desde el envío del prompt, se emite una alerta de timeout.",
            "6. **Cierre limpio:** Al finalizar la aplicación, el proceso hijo recibe `SIGTERM` y se espera hasta 3 segundos antes de forzar `SIGKILL`.",
            "7. **Mock subproceso para CI:** Existe un simulador de NDJSON para ejecutar pruebas sin requerir el binario de Antigravity instalado.",
        ],
        "subtasks": [
            ("ST-RUST-06.1", "Implementar spawn y gestión de subproceso con tokio::process"),
            ("ST-RUST-06.2", "Modelar esquemas Serde para eventos NDJSON y parser de líneas en stream"),
            ("ST-RUST-06.3", "Implementar MockAntigravityServer y tests de integración de streaming"),
        ],
    },
    {
        "id": "HU-RUST-07",
        "title": "Dynamic Session Pointer y rotación de memoria conversacional",
        "points": 5,
        "priority": "High",
        "version": 1,
        "agent": "Agente 2 (Brain & Coordinator)",
        "epic": "epic:C",
        "risk": "risk:medio",
        "desc": "El usuario renueva el contexto de su conversación por voz diciendo 'Viernes, abre otra conversación' mediante el gestor de punteros de sesión para limpiar la memoria activa sin reiniciar el servicio.",
        "flow": """[Voz: "Viernes, abre otra conversación"]
                   │
                   ▼
       ┌────────────────────────┐
       │ SessionPointerManager  │ <── Lectura/Escritura session.json
       └────────────────────────┘
                   │
       ┌───────────┴───────────┐
       ▼                       ▼
[Archivar ID anterior]    [Generar nuevo UUIDv4]
       │                       │
       ▼                       ▼
[Confirmar por voz: "Iniciando nueva sesión, Señor"]""",
        "criteria": [
            "1. **Persistencia atómica:** La sesión activa se almacena en `~/.config/viernes/session.json` mediante escritura atómica (archivo temporal + renombrado).",
            "2. **Detección de intención de rotación:** El coordinador analiza comandos de intención con regex compilada para detectar frases como 'abre otra conversación' o 'nueva conversación'.",
            "3. **Generación de nuevo puntero:** La rotación genera un nuevo UUIDv4 y archiva el ID anterior en el registro histórico.",
            "4. **Reconexión inmediata:** El cliente de Antigravity adopta el nuevo session ID en la siguiente interacción sin necesidad de reiniciar el daemon.",
            "5. **Flag CLI --new-conversation:** Permite forzar la rotación desde línea de comandos al invocar el binario.",
            "6. **Pruebas de concurrencia:** Pruebas unitarias verifican que lecturas y escrituras simultáneas a `session.json` no produzcan archivos corruptos.",
        ],
        "subtasks": [
            ("ST-RUST-07.1", "Implementar SessionPointerManager con escritura atómica de archivos JSON"),
            ("ST-RUST-07.2", "Implementar detector de intención de rotación en el analizador léxico"),
            ("ST-RUST-07.3", "Tests de integridad y carreras concurrentes en sesión"),
        ],
    },
    {
        "id": "HU-RUST-08",
        "title": "Máquina de estados central (Idle, Listening, Thinking, Speaking) y bus de eventos",
        "points": 5,
        "priority": "Highest",
        "version": 1,
        "agent": "Agente 2 (Brain & Coordinator)",
        "epic": "epic:D",
        "risk": "risk:alto",
        "desc": "El usuario experimenta un asistente predecible y coherente mediante una máquina de estados determinista para coordinar cuándo escuchar, cuándo pensar y cuándo hablar sin estados inconsistentes.",
        "flow": """      ┌───────────┐
      │   Idle    │ ◄─────────────────────────┐
      └─────┬─────┘                           │
            │ (VoiceStarted / PTT)            │ (PlaybackFinished)
            ▼                                 │
      ┌───────────┐                     ┌─────┴─────┐
      │ Listening │ ──► Thinking ───►   │ Speaking  │
      └───────────┘    (VoiceFinished)  └───────────┘""",
        "criteria": [
            "1. **Definición de estados:** Modela estrictamente los estados `Idle`, `Listening`, `Thinking` y `Speaking` con tipos cerrados.",
            "2. **Transiciones deterministas:** Cualquier evento inesperado (ej. audio entrante mientras ya está en `Listening`) se maneja con descarte limpio o transición segura.",
            "3. **Bus de eventos broadcast:** El estado se distribuye a todos los subsistemas mediante un canal `tokio::sync::broadcast` con latencia sub-microsegundo.",
            "4. **Telemetría de transiciones:** Cada cambio de estado registra timestamp, estado previo, estado nuevo y causa disparadora en logs estructurados.",
            "5. **Soporte de interrupción:** Una transición de `Speaking` a `Listening` cancela inmediatamente la reproducción activa.",
            "6. **Pruebas de propiedad:** Se valida con `proptest` que ninguna secuencia aleatoria de eventos deje al sistema en estado bloqueado o zombi.",
        ],
        "subtasks": [
            ("ST-RUST-08.1", "Modelar StateMachine y canal broadcast de eventos del sistema"),
            ("ST-RUST-08.2", "Implementar lógica de transición de estados y despacho concurrente"),
            ("ST-RUST-08.3", "Suite de pruebas property-based para robustez de la máquina de estados"),
        ],
    },
    {
        "id": "HU-RUST-09",
        "title": "Disparo inmediato de frases de relleno (conversational fillers) en < 150ms",
        "points": 5,
        "priority": "High",
        "version": 1,
        "agent": "Agente 2 (Brain & Coordinator)",
        "epic": "epic:D",
        "risk": "risk:medio",
        "desc": "El usuario escucha una confirmación hablada inmediata (ej. 'A la orden, Señor') en menos de 150 ms para enmascarar la latencia de red de los modelos de lenguaje mientras procesan la consulta.",
        "flow": """[Evento VoiceFinished]
          │
          ├─────────────────────────────────────────┐
          ▼ (Inmediato < 150ms)                     ▼ (Async)
┌────────────────────────┐              ┌────────────────────────┐
│  Reproducir Filler     │              │  Consultar Antigravity │
│  pre-sintetizado WAV   │              │  (Red: 600ms - 1500ms) │
└────────────────────────┘              └────────────────────────┘
          │                                         │
          ▼                                         ▼
[Usuario oye confirmación]              [Llega respuesta final]""",
        "criteria": [
            "1. **Caché pre-sintetizado:** El sistema mantiene en memoria o disco archivos PCM/WAV pre-cacheados de frases inmediatas, de herramientas y de paciencia.",
            "2. **Disparo ultra-rápido:** Al detectarse fin de voz, se despacha la reproducción de una frase de relleno aleatoria en < 150 ms.",
            "3. **Categorización por herramienta:** Si la respuesta invoca herramientas de archivos, terminal o búsqueda web, se vocaliza una frase representativa de la categoría.",
            "4. **Temporizador de paciencia:** Si la respuesta de Antigravity supera los 6.5 segundos, se dispara automáticamente una frase de paciencia ('Sigo procesando su consulta...').",
            "5. **Personalización desde JSON:** Permite cargar frases personalizadas desde `~/.config/viernes/fillers.json` con recarga dinámica.",
            "6. **Pruebas de latencia:** Pruebas automatizadas miden que el tiempo transcurrido entre `VoiceFinished` y el inicio del audio de filler sea inferior a 150 ms.",
        ],
        "subtasks": [
            ("ST-RUST-09.1", "Implementar FillerCacheManager y carga desde ~/.config/viernes/fillers.json"),
            ("ST-RUST-09.2", "Implementar dispatcher de fillers con temporizadores de paciencia concurrentes"),
            ("ST-RUST-09.3", "Pruebas unitarias de selección aleatoria y verificación de benchmarks de latencia"),
        ],
    },
    {
        "id": "HU-RUST-10",
        "title": "Streaming y tokenización de oraciones en tiempo real para síntesis incremental",
        "points": 5,
        "priority": "High",
        "version": 1,
        "agent": "Agente 2 (Brain & Coordinator)",
        "epic": "epic:D",
        "risk": "risk:medio",
        "desc": "El usuario escucha las respuestas habladas en streaming continuo oración por oración sin esperar a que el modelo termine de redactar párrafos enteros.",
        "flow": """[Stream de Tokens de Antigravity]
                 │
                 ▼
     ┌────────────────────────┐
     │   SentenceTokenizer    │ <── Detecta signos de puntuación (. ! ? \\n)
     └────────────────────────┘
                 │
     ┌───────────┴───────────┐
     ▼                       ▼
[Oración 1 lista]       [Oración 2 lista]
     │                       │
     ▼                       ▼
[Enviar a TTS en paralelo con la generación del resto del texto]""",
        "criteria": [
            "1. **Buffer acumulador inteligente:** Acumula tokens entrantes y segmenta el texto cuando detecta signos terminadores válidos (`.`, `?`, `!`, `\n`).",
            "2. **Protección de abreviaturas:** No corta oraciones en abreviaturas comunes (ej. `Sr.`, `Dr.`, `etc.`, números con decimales `3.14`).",
            "3. **Longitud mínima de emisión:** Fragmentos inferiores a 10 caracteres no se emiten hasta acumular la siguiente cláusula para evitar audio entrecortado.",
            "4. **Vaciado al finalizar:** Al recibir la señal de fin de generación, emite cualquier texto remanente en el buffer.",
            "5. **Canal pipelined hacia TTS:** Cada oración completada se envía inmediatamente a la cola de síntesis sin bloquear el parser de tokens.",
            "6. **Pruebas con texto complejo:** Pruebas unitarias validan la tokenización correcta en diálogos técnicos, código embebido y listas numeradas.",
        ],
        "subtasks": [
            ("ST-RUST-10.1", "Implementar SentenceTokenizer con reglas defensivas de puntuación y números"),
            ("ST-RUST-10.2", "Conectar el stream de tokens al pipeline productor-consumidor hacia TTS"),
            ("ST-RUST-10.3", "Pruebas de borde de tokenización con signos ortográficos en español"),
        ],
    },
    {
        "id": "HU-RUST-11",
        "title": "Confirmación por voz de comandos destructivos y notificaciones de fallo",
        "points": 5,
        "priority": "Medium",
        "version": 1,
        "agent": "Agente 2 (Brain & Coordinator)",
        "epic": "epic:C",
        "risk": "risk:medio",
        "desc": "El usuario mantiene el control seguro de su sistema recibiendo alertas auditivas claras y solicitudes de confirmación explícita antes de ejecutar acciones de alto riesgo (ej. borrar directorios).",
        "flow": """[Comando riesgoso detectado: "rm -rf /directorio"]
                   │
                   ▼
       [Viernes vocaliza: "¿Está seguro que desea ejecutar esta acción, Señor?"]
                   │
                   ▼
       [Esperar confirmación: "Sí, procede" / "Cancela"]
                   │
       ┌───────────┴───────────┐
       ▼                       ▼
  [Ejecutar acción]       [Abortar con feedback sonoro]""",
        "criteria": [
            "1. **Clasificador de riesgo:** Identifica comandos destructivos o de alto impacto mediante patrones configurados.",
            "2. **Pausa en espera de confirmación:** El asistente no ejecuta la acción y entra en estado `AwaitingConfirmation` con timeout de 10 segundos.",
            "3. **Mapeo de respuestas afirmativas/negativas:** Procesa respuestas como 'sí', 'afirmativo', 'procede' para ejecutar, o 'no', 'cancela', 'aborta' para descartar.",
            "4. **Cancelación por timeout:** Si transcurren 10 segundos sin respuesta explícita, la acción se cancela automáticamente por seguridad.",
            "5. **Notificación de fallos de infraestructura:** Si falla la red, el micrófono o una API externa, se vocaliza una explicación clara en lenguaje natural.",
            "6. **Pruebas de flujo de confirmación:** Pruebas unitarias verifican la denegación por omisión y la confirmación positiva/negativa.",
        ],
        "subtasks": [
            ("ST-RUST-11.1", "Implementar clasificador de operaciones destructivas y estado de confirmación"),
            ("ST-RUST-11.2", "Implementar temporizador de cancelación y feedback sonoro de fallos"),
            ("ST-RUST-11.3", "Pruebas unitarias de flujos de confirmación y rechazo"),
        ],
    },

    # -------------------------------------------------------------------------
    # AGENTE 3: TTS & Audio Output ("La Voz")
    # -------------------------------------------------------------------------
    {
        "id": "HU-RUST-12",
        "title": "Cliente HTTP ElevenLabs con Pool Multicuenta y rotación atómica",
        "points": 8,
        "priority": "Highest",
        "version": 1,
        "agent": "Agente 3 (TTS & Audio Output)",
        "epic": "epic:A",
        "risk": "risk:alto",
        "desc": "El usuario escucha voces ultra-realistas de ElevenLabs utilizando un pool multicuenta con rotación automática transparente ante agotamiento de cuotas gratuitas.",
        "flow": """[Texto: "A la orden, Señor."]
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
                         [Reintentar petición sin error perceptible]""",
        "criteria": [
            "1. **Cliente HTTP asíncrono reqwest:** Configura un cliente HTTP con connection pooling, TLS nativo y compresión de tramas.",
            "2. **Carga y parsing de pool de claves:** Parsea `ELEVENLABS_API_KEYS` permitiendo una o múltiples claves separadas por coma.",
            "3. **Rotación atómica thread-safe:** El índice de clave activa se gestiona con `AtomicUsize`, permitiendo concurrencia segura sin contención de locks.",
            "4. **Detección de 401 y 429:** Ante respuestas HTTP 401 (Unauthorized) o 429 (Too Many Requests), marca la clave como agotada y rota inmediatamente a la siguiente.",
            "5. **Sin reintentos infinitos:** Si todas las claves del pool fallan, emite un error estructurado `AllKeysExhausted` para activar el fallback a Edge-TTS.",
            "6. **Seguridad de credenciales:** Ninguna clave se registra en logs o salidas de consola.",
            "7. **Mock HTTP Server:** Pruebas automatizadas con servidor HTTP simulado (`wiremock`) validan la rotación ante códigos 429.",
        ],
        "subtasks": [
            ("ST-RUST-12.1", "Implementar cliente HTTP reqwest para síntesis de ElevenLabs con streaming MP3"),
            ("ST-RUST-12.2", "Implementar lógica de rotación atómica KeyPool con AtomicUsize"),
            ("ST-RUST-12.3", "Tests con wiremock simulando fallos de cuota y rotación de claves"),
        ],
    },
    {
        "id": "HU-RUST-13",
        "title": "Proveedor de fallback Edge-TTS nativo sobre WebSockets asíncronos",
        "points": 8,
        "priority": "Highest",
        "version": 1,
        "agent": "Agente 3 (TTS & Audio Output)",
        "epic": "epic:A",
        "risk": "risk:alto",
        "desc": "El usuario nunca se queda sin respuesta hablada gracias a un motor de respaldo Edge-TTS 100% gratuito basado en WebSockets cuando las claves de ElevenLabs se agotan o no hay conexión.",
        "flow": """[Fallo en ElevenLabs / Sin cuota]
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
       [Voz sintetizada con éxito sin interrumpir la experiencia]""",
        "criteria": [
            "1. **Conexión WebSocket segura:** Establece conexión WSS hacia los endpoints de voz de Microsoft Edge mediante `tokio-tungstenite`.",
            "2. **Generación de paquetes SSML:** Construye mensajes SSML con parámetros de voz en español neutro (`es-ES-ElviraNeural` o similar).",
            "3. **Decodificación de paquetes mixtos:** Parsea tramas de texto de metadatos y extrae los payloads binarios de audio MP3.",
            "4. **Conmutación transparente:** El fallback se activa automáticamente en < 200 ms si el proveedor primario falla.",
            "5. **Manejo de desconexión:** Si el WebSocket se cierra abruptamente, reconecta automáticamente o lanza error tipado.",
            "6. **Cero dependencias de Python:** Implementado puramente en Rust sin llamar a subprocesos externos de Python.",
            "7. **Pruebas de síntesis:** Pruebas unitarias y de integración verifican la producción de streams de audio MP3 válidos.",
        ],
        "subtasks": [
            ("ST-RUST-13.1", "Implementar handshake WSS y empaquetador SSML con tokio-tungstenite"),
            ("ST-RUST-13.2", "Implementar parser de tramas binarias de audio MP3"),
            ("ST-RUST-13.3", "Tests de conmutación automática y robustez ante cortes de conexión"),
        ],
    },
    {
        "id": "HU-RUST-14",
        "title": "Reproductor de audio streaming sin bloqueo con decodificación MP3",
        "points": 5,
        "priority": "High",
        "version": 1,
        "agent": "Agente 3 (TTS & Audio Output)",
        "epic": "epic:A",
        "risk": "risk:medio",
        "desc": "El usuario escucha las respuestas de voz con reproducción suave e ininterrumpida sin que la interfaz o los hilos de red se congelen.",
        "flow": """[Chunks MP3 de ElevenLabs / Edge-TTS]
                   │
                   ▼
       ┌────────────────────────┐
       │   StreamingAudioPlayer │ <── Backend rodio / symphonia
       └────────────────────────┘
                   │
                   ▼
       [Cola FIFO de fragmentos en hilo de reproducción dedicado]
                   │
                   ▼
       [Emisión continua hacia los altavoces seleccionados]""",
        "criteria": [
            "1. **Decodificación MP3 en memoria:** Decodifica streams de bytes MP3 en tiempo real utilizando `symphonia` o decodificadores nativos de `rodio`.",
            "2. **Hilo de reproducción dedicado:** El reproductor opera en su propio hilo de alta prioridad para evitar glitches o saltos de audio.",
            "3. **Cola FIFO continua:** Admite encolar múltiples oraciones secuenciales, reproduciéndolas fluidamente sin silencios entre fragmentos.",
            "4. **Evento de finalización:** Emite el evento `PlaybackFinished` cuando el último chunk de la cola concluye.",
            "5. **Selección de dispositivo:** Permite enrutar el stream de salida hacia el dispositivo configurado en `AUDIO_OUTPUT_DEVICE`.",
            "6. **Pruebas sin servidor de audio:** Dispone de un mock `SilentAudioSink` para pruebas en entornos headless de CI.",
        ],
        "subtasks": [
            ("ST-RUST-14.1", "Implementar StreamingAudioPlayer basado en rodio Sink y symphonia"),
            ("ST-RUST-14.2", "Implementar cola FIFO concurrente con notificación de fin de reproducción"),
            ("ST-RUST-14.3", "Tests unitarios con SilentAudioSink en entornos headless"),
        ],
    },
    {
        "id": "HU-RUST-15",
        "title": "Interrupción atómica de reproducción por voz humana (Barge-in)",
        "points": 8,
        "priority": "Highest",
        "version": 1,
        "agent": "Agente 3 (TTS & Audio Output)",
        "epic": "epic:D",
        "risk": "risk:alto",
        "desc": "El usuario interrumpe a Viernes en cualquier momento mientras está hablando simplemente empezando a hablar, logrando que el asistente calle al instante sin retrasos.",
        "flow": """[Viernes reproduciendo respuesta larga...]
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
[Buffer vaciado de golpe]   [Estado vuelve a Listening]""",
        "criteria": [
            "1. **Kill-Switch atómico:** Cuenta con un flag `AtomicBool` y `CancellationToken` de Tokio para abortar la reproducción activa en < 80 ms.",
            "2. **Vaciado inmediato de hardware:** Al recibir la señal de interrupción, se vacía inmediatamente el buffer de hardware de audio (`sink.stop()`).",
            "3. **Cancelación de peticiones TTS pendientes:** Cualquier petición de red en curso para oraciones futuras se aborta de inmediato para ahorrar cuota.",
            "4. **Transición limpia a Listening:** El sistema transiciona de forma determinista a `Listening`, canalizando el nuevo audio del usuario al STT.",
            "5. **Protección anti-eco (Acoustic Echo):** Si se utiliza altavoz abierto, se implementa una ventana de gracia para evitar que el propio audio de Viernes dispare falsos barge-ins.",
            "6. **Pruebas de latencia de corte:** Pruebas automatizadas garantizan que el tiempo entre `VoiceStarted` y el cese de audio sea inferior a 80 ms.",
        ],
        "subtasks": [
            ("ST-RUST-15.1", "Implementar mecanismo de corte con AtomicBool y CancellationToken"),
            ("ST-RUST-15.2", "Vincular evento VAD VoiceStarted con cancelación de TTS y vaciado de sink"),
            ("ST-RUST-15.3", "Pruebas de latencia de interrupción y descarte de cola de síntesis"),
        ],
    },
    {
        "id": "HU-RUST-16",
        "title": "Auditoría de concurrencia, gestión de cache temporal y retención de memoria",
        "points": 3,
        "priority": "Low",
        "version": 1,
        "agent": "Agente 3 (TTS & Audio Output)",
        "epic": "epic:A",
        "risk": "risk:bajo",
        "desc": "El usuario disfruta de un sistema estable que puede ejecutarse durante semanas sin saturar el disco ni agotar la memoria RAM por archivos temporales huérfanos.",
        "flow": """[Archivos temporales generados en /tmp/viernes/]
                   │
                   ▼
       ┌────────────────────────┐
       │ TempStorageManager     │ <── Política de retención (TTL / Capacidad)
       └────────────────────────┘
                   │
                   ▼
       [Limpieza periódica en segundo plano sin bloquear llamadas activas]""",
        "criteria": [
            "1. **Directorio temporal seguro:** Crea y administra `/tmp/viernes/` con permisos de usuario restrictivos (0700).",
            "2. **Purga automática por antigüedad:** Elimina archivos temporales de audio con más de 2 horas de antigüedad.",
            "3. **Límite de cuota en disco:** Si el directorio supera 100 MB, purga los archivos más antiguos hasta restablecer un margen de seguridad.",
            "4. **Limpieza en shutdown:** Al recibir la señal de apagado, limpia los fragmentos de audio huérfanos.",
            "5. **Verificación con AddressSanitizer:** La suite de tests compila con ASan para garantizar ausencia de fugas de memoria o use-after-free.",
            "6. **Pruebas unitarias:** Valida la purga automática simulando archivos temporales con timestamps manipulados.",
        ],
        "subtasks": [
            ("ST-RUST-16.1", "Implementar TempStorageManager con políticas de retención y cuota"),
            ("ST-RUST-16.2", "Vincular limpieza automática al ciclo de vida del servicio"),
            ("ST-RUST-16.3", "Tests de retención de disco y verificación con Valgrind"),
        ],
    },

    # -------------------------------------------------------------------------
    # ORQUESTADOR & INFRAESTRUCTURA (Skill migrate-to-rust)
    # -------------------------------------------------------------------------
    {
        "id": "HU-RUST-17",
        "title": "Scaffolding del Cargo Workspace modular con #![forbid(unsafe_code)]",
        "points": 5,
        "priority": "Highest",
        "version": 1,
        "agent": "Orquestador / Infraestructura",
        "epic": "epic:A",
        "risk": "risk:medio",
        "desc": "El equipo de desarrollo cuenta con una base de código modular y 100% segura en Rust con separación estricta de responsabilidades entre crates.",
        "flow": """viernes-rust (Workspace)
  ├── crates/viernes-core  (Contratos, Eventos, StateMachine)
  ├── crates/viernes-audio (cpal, VAD, Whisper)
  ├── crates/viernes-brain (Antigravity NDJSON, Pipeline)
  └── crates/viernes-tts   (ElevenLabs, Edge-TTS, Rodio)""",
        "criteria": [
            "1. **Workspace Cargo multi-crate:** Estructura modular con crates independientes `viernes-core`, `viernes-audio`, `viernes-brain` y `viernes-tts`.",
            "2. **Prohibición de código inseguro:** Se declara `#![forbid(unsafe_code)]` en la raíz de cada crate sin excepciones no aprobadas.",
            "3. **Linter estricto:** Compilación limpia con `cargo clippy --all-targets -- -D warnings` (0 advertencias toleradas).",
            "4. **Traits canónicos en viernes-core:** Define los contratos `AudioInput`, `SpeechToText`, `BrainService`, `TextToSpeech` y `AudioSink`.",
            "5. **Manejo de errores idiomático con thiserror:** Cada crate expone sus tipos de error fuertemente tipados sin conversión genérica a String.",
            "6. **CI automatizado:** Workflow de GitHub Actions configurado para compilar, verificar formato (`cargo fmt`) y pasar linter en cada PR.",
        ],
        "subtasks": [
            ("ST-RUST-17.1", "Crear estructura de workspace Cargo con los 4 crates y dependencias base"),
            ("ST-RUST-17.2", "Configurar linter clippy estricto y políticas de unsafe_code"),
            ("ST-RUST-17.3", "Definir traits e interfaces comunes en viernes-core"),
        ],
    },
    {
        "id": "HU-RUST-18",
        "title": "Harness de paridad diferencial A/B (Python vs Rust) y suite proptest",
        "points": 8,
        "priority": "High",
        "version": 1,
        "agent": "Orquestador / Infraestructura",
        "epic": "epic:A",
        "risk": "risk:alto",
        "desc": "El equipo técnico valida que la versión en Rust ofrezca exactamente el mismo comportamiento funcional y precisión que la versión en Python mediante pruebas diferenciales automatizadas.",
        "flow": """[Dataset de prueba: 50 muestras de voz WAV + Prompts]
                     │
         ┌───────────┴───────────┐
         ▼                       ▼
  [Viernes Python]        [Viernes Rust]
         │                       │
         └───────────┬───────────┘
                     ▼
         ┌───────────────────────┐
         │  A/B Parity Harness   │ ──► Compara transcripción y eventos
         └───────────────────────┘
                     │
                     ▼
  [Reporte de Paridad: >= 99.5% Coincidencia]""",
        "criteria": [
            "1. **Harness de prueba diferencial:** Ejecuta comparaciones automáticas entre la implementación de Python y la de Rust alimentadas con el mismo dataset de audio.",
            "2. **Tolerancia de transcripción:** La transcripción de Whisper en Rust coincide con Python con un WER (Word Error Rate) < 1.0%.",
            "3. **Pruebas de propiedades con proptest:** Genera entradas aleatorias para el parser de NDJSON y la máquina de estados verificando invariantes.",
            "4. **Medición de latencia comparativa:** Registra el tiempo de respuesta total comparando Python vs Rust bajo condiciones idénticas.",
            "5. **Salida de telemetría:** Genera `migration_telemetry.json` con los resultados de cada ejecución según la especificación de la skill.",
            "6. **Ejecución en CI:** El harness se ejecuta en GitHub Actions como paso bloqueante de integración continua.",
        ],
        "subtasks": [
            ("ST-RUST-18.1", "Crear dataset canónico de audio y prompts para pruebas diferenciales"),
            ("ST-RUST-18.2", "Implementar script de comparación A/B y cálculo de WER"),
            ("ST-RUST-18.3", "Integrar suite proptest y exportación de telemetría JSON"),
        ],
    },
    {
        "id": "HU-RUST-19",
        "title": "Pipeline de optimización guiada por perfiles (PGO) y LTO para binario final",
        "points": 5,
        "priority": "Medium",
        "version": 1,
        "agent": "Orquestador / Infraestructura",
        "epic": "epic:A",
        "risk": "risk:medio",
        "desc": "El usuario obtiene un ejecutable de máxima velocidad compilado mediante Profile-Guided Optimization (PGO) y Link-Time Optimization (LTO) para reducir al mínimo el uso de CPU.",
        "flow": """[Compilar con -Cprofile-generate]
                 │
                 ▼
[Ejecutar cargas representativas de voz y VAD]
                 │
                 ▼
[Combinar perfiles con llvm-profdata]
                 │
                 ▼
[Recompilar con -Cprofile-use y lto = "fat"] ──► [Binario Optimizado de 35MB]""",
        "criteria": [
            "1. **Configuración de LTO:** El perfil `release` de Cargo incluye `lto = \"fat\"`, `codegen-units = 1` y `panic = \"abort\"`.",
            "2. **Fase de instrumentación PGO:** Genera binario instrumentado con `RUSTFLAGS=\"-Cprofile-generate=./pgo-data\"`.",
            "3. **Fase de entrenamiento:** Ejecuta una sesión simulada de 5 minutos con procesamiento de voz, VAD y streaming.",
            "4. **Fase de optimización:** Genera el binario final utilizando los datos de perfil combinados con `llvm-profdata`.",
            "5. **Medición de ganancias:** Mide y registra en telemetría la reducción de uso de CPU y tiempo de arranque comparado con el build estándar.",
            "6. **Generación de Infografía:** Genera el reporte interactivo `migration_infographic.html` mediante `scripts/telemetry.py`.",
        ],
        "subtasks": [
            ("ST-RUST-19.1", "Configurar perfil de release en Cargo.toml con LTO y codegen-units"),
            ("ST-RUST-19.2", "Automatizar pipeline de entrenamiento PGO y recolección de perfiles"),
            ("ST-RUST-19.3", "Medir benchmarks y generar reporte interactivo migration_infographic.html"),
        ],
    },
    {
        "id": "HU-RUST-20",
        "title": "Daemonización con servicio systemd de usuario y CLI enriquecida",
        "points": 3,
        "priority": "Medium",
        "version": 1,
        "agent": "Orquestador / Infraestructura",
        "epic": "epic:A",
        "risk": "risk:bajo",
        "desc": "El usuario inicia, detiene o supervisa Viernes como un servicio de fondo en Linux mediante systemd y una interfaz de comandos ergonómica con clap.",
        "flow": """$ viernes-rust --daemon
           │
           ▼
[Iniciar bucle en segundo plano gestionado por systemd]
           │
$ systemctl --user status viernes-rust
● viernes-rust.service - Viernes Voice Assistant (Rust Native Daemon)
   Active: active (running)""",
        "criteria": [
            "1. **CLI con clap:** Provee comandos y flags útiles: `--daemon`, `--new-conversation`, `--test-voice`, `--list-devices`, `--version`.",
            "2. **Manejo de señales Unix:** Intercepta `SIGINT`, `SIGTERM` y `SIGHUP` para realizar un apagado ordenado en < 500 ms.",
            "3. **Unidad systemd nativa:** Provee el archivo `viernes-rust.service` configurado para arranque automático en la sesión de usuario.",
            "4. **Watchdog de salud:** Notifica a systemd el estado de salud mediante el protocolo `sd_notify` si está disponible.",
            "5. **Logs estructurados:** Emite logs a journald en formato limpio con niveles de severidad configurables (`RUST_LOG=info`).",
            "6. **Pruebas de señales:** Pruebas automatizadas validan el shutdown ordenado ante el envío de señales de terminación.",
        ],
        "subtasks": [
            ("ST-RUST-20.1", "Implementar CLI con clap y despacho de subcomandos y banderas"),
            ("ST-RUST-20.2", "Implementar signal handling para SIGINT/SIGTERM con apagado ordenado"),
            ("ST-RUST-20.3", "Crear unidad systemd viernes-rust.service y validar en entorno local"),
        ],
    },
]

for hu in HUS:
    filename = f"{hu['id']}-{hu['title'].lower().replace(' ', '-').replace('/', '-').replace('(', '').replace(')', '').replace(',', '')}.md"
    filepath = BASE_DIR / filename

    content = f"""# {hu['id']}: {hu['title']}

**Puntos de Historia:** {hu['points']} | **Prioridad:** {hu['priority']} | **Versión:** {hu['version']} | **Asignado a:** {hu['agent']}

---

### Descripción
{hu['desc']}

---

### Mockup / Flujo de Interacción
```text
{hu['flow']}
```

---

### Criterios de Aceptación
{chr(10).join(hu['criteria'])}

---

### Subtareas Técnicas
{chr(10).join(f"- **{st_id}:** {st_title}" for st_id, st_title in hu['subtasks'])}
"""
    filepath.write_text(content, encoding="utf-8")

print(f"Generated {len(HUS)} User Stories in {BASE_DIR}")
