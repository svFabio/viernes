# HU-17: Configurar hardware de audio y calibrar sensibilidad VAD desde la interfaz gráfica

**Puntos de Historia:** 3 | **Prioridad:** High | **Versión:** 1 | **Asignado a:** Agente Audio & Hardware

---

### Descripción
El usuario configura los dispositivos físicos de entrada y salida de audio y calibra los parámetros de detección de voz (VAD) mediante la interfaz gráfica de Viernes para asegurar una captura nítida de su voz sin cortes prematuros ni ecos en la reproducción.

---

### Mockup / Flujo de Interacción
```text
┌──────────────────────────────────────────────────────────────────┐
│ VIERNES // CENTRO DE CONTROL             ● Servicio Activo       │
├──────────────────────────────────────────────────────────────────┤
│ [Credenciales & Voz]  [Hardware de Audio]  [Servicio & Estado]   │
│                                                                  │
│  Micrófono de Entrada (Input Device):                            │
│  [ [0] USB PnP Audio Device Microphone                      ▼ ]  │
│                                                                  │
│  Dispositivo de Salida (Altavoces / Auriculares):                │
│  [ [1] Family 17h/19h HD Audio Controller Headphones        ▼ ]  │
│                                                                  │
│  Sensibilidad de Detección VAD (Umbral de Energía):              │
│  [══════════════════●────────────────────────────────] 0.0120    │
│                                                                  │
│  Tiempo de Silencio para Corte (ms):                             │
│  [══════════════════════════════●────────────────────] 800 ms    │
│                                                                  │
├──────────────────────────────────────────────────────────────────┤
│ [ ✔ Guardar Cambios en .env ]                                    │
│ Archivo: /home/svfabio/Projects/09-viernes/.env                   │
└──────────────────────────────────────────────────────────────────┘
```

---

### Criterios de Aceptación
1. **Detección dinámica de hardware:** Al cargar la pestaña "Hardware de Audio", el sistema debe invocar `list_audio_devices()` y poblar los selectores desplegables con todos los micrófonos y altavoces conectados.
2. **Opción por defecto del sistema:** Ambos selectores deben incluir como primera opción "Por defecto del sistema" (valor vacío o `None`).
3. **Mapeo de selección existente:** Si en el archivo `.env` ya existe un `AUDIO_INPUT_DEVICE` o `AUDIO_OUTPUT_DEVICE` configurado, el selector correspondiente debe posicionarse en dicho dispositivo al iniciar.
4. **Control deslizante de VAD interactivo:** El umbral de energía de VAD debe controlarse mediante un slider con rango de 0.0020 a 0.0500, con saltos suaves de 0.0010.
5. **Etiqueta reactiva de valor VAD:** Al arrastrar el slider de VAD, la etiqueta numérica adyacente debe actualizarse en tiempo real reflejando el valor decimal exacto con 4 dígitos.
6. **Control deslizante de corte de silencio:** El tiempo de silencio debe configurarse entre 300 ms y 2000 ms con saltos discretos de 100 ms.
7. **Etiqueta reactiva de tiempo:** Al arrastrar el slider de silencio, la etiqueta debe mostrar inmediatamente el valor en milisegundos (ej. "800 ms").
8. **Persistencia en .env:** Al presionar "Guardar Cambios", se deben escribir las variables `AUDIO_INPUT_DEVICE`, `AUDIO_OUTPUT_DEVICE`, `VAD_THRESHOLD` y `SILENCE_TIMEOUT_MS` en el archivo de configuración.
9. **Resiliencia ante desconexión de dispositivo:** Si un dispositivo previamente guardado ya no está conectado, el sistema debe seleccionar la opción por defecto sin lanzar excepciones no controladas.
10. **Cancelación o descarte:** Si el usuario cierra la ventana sin presionar "Guardar Cambios", los valores previos en `.env` deben permanecer inalterados.

---

### Subtareas de Implementación
- **SUB-17.1:** Desarrollar extractor de dispositivos compatibles discriminando canales de entrada y salida en `src/viernes/gui/service.py`.
- **SUB-17.2:** Construir selectores dinámicos y sliders interactivos con callbacks reactivos en `src/viernes/gui/app.py`.
- **SUB-17.3:** Integrar lectura y guardado de variables de hardware en el serializador `.env`.
- **SUB-17.4:** Implementar fallback automático cuando el ID o nombre del dispositivo almacenado no esté disponible.
- **SUB-17.5:** Diseñar tests unitarios con mocks de `sounddevice` verificando renderizado de dispositivos y persistencia de sliders.
