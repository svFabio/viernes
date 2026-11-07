# HU-RUST-16: Auditoría de concurrencia, gestión de cache temporal y retención de memoria

**Puntos de Historia:** 3 | **Prioridad:** Low | **Versión:** 1 | **Asignado a:** Agente 3 (TTS & Audio Output)

---

### Descripción
El usuario disfruta de un sistema estable que puede ejecutarse durante semanas sin saturar el disco ni agotar la memoria RAM por archivos temporales huérfanos.

---

### Mockup / Flujo de Interacción
```text
[Archivos temporales generados en /tmp/viernes/]
                   │
                   ▼
       ┌────────────────────────┐
       │ TempStorageManager     │ <── Política de retención (TTL / Capacidad)
       └────────────────────────┘
                   │
                   ▼
       [Limpieza periódica en segundo plano sin bloquear llamadas activas]
```

---

### Criterios de Aceptación
1. **Directorio temporal seguro:** Crea y administra `/tmp/viernes/` con permisos de usuario restrictivos (0700).
2. **Purga automática por antigüedad:** Elimina archivos temporales de audio con más de 2 horas de antigüedad.
3. **Límite de cuota en disco:** Si el directorio supera 100 MB, purga los archivos más antiguos hasta restablecer un margen de seguridad.
4. **Limpieza en shutdown:** Al recibir la señal de apagado, limpia los fragmentos de audio huérfanos.
5. **Verificación con AddressSanitizer:** La suite de tests compila con ASan para garantizar ausencia de fugas de memoria o use-after-free.
6. **Pruebas unitarias:** Valida la purga automática simulando archivos temporales con timestamps manipulados.

---

### Subtareas Técnicas
- **ST-RUST-16.1:** Implementar TempStorageManager con políticas de retención y cuota
- **ST-RUST-16.2:** Vincular limpieza automática al ciclo de vida del servicio
- **ST-RUST-16.3:** Tests de retención de disco y verificación con Valgrind
