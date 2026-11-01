# HU-04: Reproducir audio generado en segundo plano sin congelar la aplicación

**Puntos de Historia:** 3 | **Prioridad:** High | **Versión:** 1 | **Asignado a:** Agente Audio

---

### Descripción
El usuario escucha la reproducción de audio a través de los reproductores nativos del sistema (mpv o pipewire) de forma asíncrona y controlada para que la interfaz y la escucha del micrófono sigan activas mientras el asistente habla.

---

### Mockup / Flujo de Interacción
```text
[AudioPlayer.play(path, blocking=False)]
                  │
                  ▼
   Detecta binario del sistema:
   1. mpv (--no-video --no-terminal)
   2. pw-play (PipeWire)
   3. paplay (PulseAudio)
   4. aplay (ALSA fallback)
                  │
                  ▼
   Spawn subproceso aislado Popen
   Control de PID para interrupción (Barge-in)
```

---

### Criterios de Aceptación
1. **Detección automática de backend:** Al instanciar `AudioPlayer`, el sistema verifica la disponibilidad en `PATH` de los ejecutables `mpv`, `pw-play`, `paplay` o `aplay` en ese orden de preferencia.
2. **Excepción clara si falta reproductor:** Si ninguno de los ejecutables está instalado en el sistema, se lanza un error descriptivo indicando al usuario qué comando instalar (`sudo pacman -S mpv` o equivalente).
3. **Reproducción no bloqueante:** Al invocar `play(audio_path, blocking=False)`, el método retorna inmediatamente un identificador de proceso (`Popen`), sin detener el bucle principal de eventos.
4. **Reproducción bloqueante opcional:** Al invocar `play(audio_path, blocking=True)`, el método espera a que el audio finalice antes de continuar la ejecución.
5. **Silenciamiento de salidas estándar:** La ejecución del reproductor redirige `stdout` y `stderr` a `DEVNULL` para evitar ensuciar la terminal con metadatos del reproductor multimedia.
6. **Validación de existencia del archivo:** Si el archivo de audio especificado no existe o es ilegible, el método lanza `FileNotFoundError` antes de invocar cualquier subproceso.
7. **Limpieza de procesos huérfanos:** Cuando el audio termina de reproducirse de forma natural, el proceso finaliza limpiamente sin dejar procesos zombie.
