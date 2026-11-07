# HU-RUST-24: Manejador resiliente de errores e interceptor con políticas de reintento

**Puntos de Historia:** 3 | **Prioridad:** Medium | **Versión:** 1 | **Asignado a:** Agente 2 (Brain & Pipeline)

---

### Descripción
El sistema recupera fallos transitorios en ejecución de herramientas y servicios mediante una política de retroceso exponencial (exponential backoff) con jitter.

---

### Mockup / Flujo de Interacción
```text
[Llamada a servicio / tool falla con error transitorio]
                 │
                 ▼
     ┌────────────────────────┐
     │ ErrorInterceptor       │ ── Reintentos restantes? ── NO ──► [Error fatal reportado]
     └────────────────────────┘
                 │ SÍ
                 ▼ (Espera con backoff + jitter)
     [Reintento automático]
```

---

### Criterios de Aceptación
1. **Clasificación de errores:** Distingue entre errores recuperables (red, timeout, 429) e irrecuperables (autenticación, sintaxis).
2. **Backoff con jitter:** Implementa backoff exponencial truncado con fluctuación aleatoria para evitar efecto rebaño.
3. **Límite de reintentos:** Configura un máximo de 3 reintentos antes de devolver el error estructurado.
4. **Pruebas unitarias:** Simula fallos transitorios con mocks y verifica la recuperación en el intento N.

---

### Subtareas Técnicas
- **ST-RUST-24.1:** Diseñar clasificador de recuperabilidad de ViernesError
- **ST-RUST-24.2:** Implementar combinador de retry con backoff exponencial y jitter
- **ST-RUST-24.3:** Pruebas unitarias con fallos simulados y límites de reintento
