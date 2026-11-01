# HU-19: Audición y prueba de voz neural interactiva con prevención de condiciones de carrera

**Puntos de Historia:** 3 | **Prioridad:** High | **Versión:** 1 | **Asignado a:** Agente TTS & Audio

---

### Descripción
El usuario prueba la síntesis de voz con la API key y voz configuradas mediante el botón "Probar Voz" en el Centro de Control para verificar de manera inmediata que el audio se reproduce correctamente sin congelar la ventana ni generar procesos huérfanos.

---

### Mockup / Flujo de Interacción
```text
┌──────────────────────────────────────────────────────────────────┐
│  ElevenLabs API Key: [ sk_808ad725799d106b95... ]               │
│  Voz: [ Jessica (Oficial)                                    ▼ ] │
│  Voice ID: [ cgSgspJ2msm6clMCkdW9                              ] │
│                                                                  │
│  [ ▶ Probar Voz (Audición en Vivo)                             ] │
│  Estado: Sintetizando y reproduciendo con Jessica...             │
│                                                                  │
│  (Al finalizar)                                                  │
│  Estado: ✔ Reproducción completada.                              │
└──────────────────────────────────────────────────────────────────┘
```

---

### Criterios de Aceptación
1. **Validación previa de API Key:** Al hacer clic en "Probar Voz", si el campo de API Key está vacío o solo contiene espacios, el sistema no debe contactar a ElevenLabs y debe mostrar la advertencia "⚠ Ingresa una API Key válida primero" en color rojo.
2. **Deshabilitación durante audición:** Al iniciar la prueba, el botón "Probar Voz" debe pasar a estado `disabled` (`state="disabled"`) de inmediato para evitar que el usuario lance múltiples solicitudes de síntesis en paralelo.
3. **Ejecución en hilo en segundo plano:** La sintetización y reproducción de audio deben ejecutarse en un hilo secundario independiente (`threading.Thread(daemon=True)`), manteniendo la interfaz gráfica totalmente fluida y responsiva.
4. **Frase de prueba por defecto:** La síntesis debe reproducir la locución oficial: `"A la orden, Señor. Viernes en línea y lista para operar."`
5. **Reproducción por canal de audio configurado:** El audio generado debe enviarse al reproductor del sistema (`AudioPlayer`) respetando los reproductores soportados (`mpv`, `pw-play`, `paplay`).
6. **Rehabilitación del botón al finalizar:** Una vez terminada la reproducción sonora o si ocurre un fallo, el botón debe volver a su estado normal (`state="normal"`).
7. **Traducción y visualización de errores:** Si la API key es inválida (error 401), la cuota se agotó (error 402/429) o no hay conexión a internet, el mensaje de error capturado debe mostrarse en la etiqueta inferior en color rojo sin provocar el cierre de la GUI.
8. **Limpieza de procesos al cerrar ventana:** Si el usuario cierra el Centro de Control mientras se está reproduciendo el audio, todos los procesos de reproducción hijos deben terminarse de forma inmediata sin dejar procesos zombies ni lanzar errores de `after` sobre widgets destruidos.

---

### Subtareas de Implementación
- **SUB-19.1:** Crear método de sintetización de prueba con phrase personalizable en `src/viernes/gui/service.py`.
- **SUB-19.2:** Implementar bloqueo de botón y despacho asíncrono en `src/viernes/gui/app.py`.
- **SUB-19.3:** Agregar interceptor de errores de red y cuota para mensajes descriptivos en la interfaz.
- **SUB-19.4:** Incorporar manejador de evento de cierre de ventana (`WM_DELETE_WINDOW`) que cancele hilos de audición y limpie procesos de `AudioPlayer`.
- **SUB-19.5:** Redactar tests unitarios con mocks de ElevenLabs verificando hilos, deshabilitación de botones y manejo de fallos.
