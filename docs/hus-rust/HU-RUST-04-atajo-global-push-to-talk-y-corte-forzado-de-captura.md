# HU-RUST-04: Atajo global Push-to-Talk y corte forzado de captura

**Puntos de Historia:** 3 | **Prioridad:** Medium | **Versión:** 1 | **Asignado a:** Agente 1 (Audio Input & STT)

---

### Descripción
El usuario activa y desactiva la captura de voz presionando y soltando una combinación de teclas global (ej. Super+V) para interactuar sin depender exclusivamente del VAD en ambientes ruidosos.

---

### Mockup / Flujo de Interacción
```text
[Presionar Super+V] ──► Iniciar AudioStream y emitir evento PttStarted
           │
           ▼
[Hablar al micrófono]
           │
           ▼
[Soltar Super+V]    ──► Llamar a finish_recording() y emitir PttFinished
```

---

### Criterios de Aceptación
1. **Captura de tecla global:** El sistema escucha eventos de pulsación y liberación de teclas a nivel de sesión de usuario en Linux.
2. **Inicio instantáneo:** Al presionar el atajo, el capturador comienza a recolectar muestras en < 10 ms.
3. **Corte inmediato:** Al soltar la tecla, se invoca `finish_recording()`, cerrando el buffer de inmediato sin esperar tiempos de silencio.
4. **Concurrencia segura:** Si el VAD estaba procesando un segmento, el comando Push-to-Talk toma precedencia determinista.
5. **Desactivación limpia:** Si el daemon se detiene, los listeners globales de teclado se liberan limpiamente.
6. **Pruebas unitarias:** Se valida la simulación de secuencias de presión y liberación con despacho de eventos.

---

### Subtareas Técnicas
- **ST-RUST-04.1:** Implementar listener de eventos de teclado para Wayland/X11
- **ST-RUST-04.2:** Vincular evento key-up con finish_recording() en el pipeline de captura
- **ST-RUST-04.3:** Tests de carrera y precedencia entre VAD y Push-to-Talk
