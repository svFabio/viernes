# HU-RUST-05: Enumeración y selección segura de dispositivos de audio ALSA/PipeWire

**Puntos de Historia:** 3 | **Prioridad:** Medium | **Versión:** 1 | **Asignado a:** Agente 1 (Audio Input & STT)

---

### Descripción
El usuario visualiza y selecciona dispositivos de audio físicos o virtuales de su sistema mediante una utilidad de resolución robusta para garantizar que el micrófono y los altavoces correctos sean utilizados.

---

### Mockup / Flujo de Interacción
```text
[Host ALSA / PipeWire / PulseAudio]
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
[Resolución segura con coincidencia parcial o índice numérico]
```

---

### Criterios de Aceptación
1. **Enumeración completa:** El sistema lista todos los dispositivos de entrada y salida disponibles en el host.
2. **Resolución por índice o nombre:** Permite resolver un dispositivo mediante su índice numérico (ej. `1`), su nombre exacto o una coincidencia parcial insensible a mayúsculas.
3. **Fallback a dispositivo por defecto:** Si el identificador configurado no existe o es vacío, el sistema selecciona silenciosamente el dispositivo predeterminado del sistema.
4. **Aislamiento de variables de entorno:** La resolución lee `AUDIO_INPUT_DEVICE` y `AUDIO_OUTPUT_DEVICE` respetando entornos herméticos de prueba.
5. **Formateo legible:** La salida de depuración imprime índices, nombres y canales máximos admitidos.
6. **Pruebas con listas simuladas:** Pruebas unitarias verifican la selección en escenarios de dispositivos ausentes, índices fuera de rango y nombres ambiguos.

---

### Subtareas Técnicas
- **ST-RUST-05.1:** Implementar módulo de enumeración de dispositivos con cpal
- **ST-RUST-05.2:** Implementar algoritmo de resolución con fallback a default
- **ST-RUST-05.3:** Tests unitarios de resolución con fixtures de dispositivos simulados
