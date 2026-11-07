# HU-RUST-18: Harness de paridad diferencial A/B (Python vs Rust) y suite proptest

**Puntos de Historia:** 8 | **Prioridad:** High | **Versión:** 1 | **Asignado a:** Orquestador / Infraestructura

---

### Descripción
El equipo técnico valida que la versión en Rust ofrezca exactamente el mismo comportamiento funcional y precisión que la versión en Python mediante pruebas diferenciales automatizadas.

---

### Mockup / Flujo de Interacción
```text
[Dataset de prueba: 50 muestras de voz WAV + Prompts]
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
  [Reporte de Paridad: >= 99.5% Coincidencia]
```

---

### Criterios de Aceptación
1. **Harness de prueba diferencial:** Ejecuta comparaciones automáticas entre la implementación de Python y la de Rust alimentadas con el mismo dataset de audio.
2. **Tolerancia de transcripción:** La transcripción de Whisper en Rust coincide con Python con un WER (Word Error Rate) < 1.0%.
3. **Pruebas de propiedades con proptest:** Genera entradas aleatorias para el parser de NDJSON y la máquina de estados verificando invariantes.
4. **Medición de latencia comparativa:** Registra el tiempo de respuesta total comparando Python vs Rust bajo condiciones idénticas.
5. **Salida de telemetría:** Genera `migration_telemetry.json` con los resultados de cada ejecución según la especificación de la skill.
6. **Ejecución en CI:** El harness se ejecuta en GitHub Actions como paso bloqueante de integración continua.

---

### Subtareas Técnicas
- **ST-RUST-18.1:** Crear dataset canónico de audio y prompts para pruebas diferenciales
- **ST-RUST-18.2:** Implementar script de comparación A/B y cálculo de WER
- **ST-RUST-18.3:** Integrar suite proptest y exportación de telemetría JSON
