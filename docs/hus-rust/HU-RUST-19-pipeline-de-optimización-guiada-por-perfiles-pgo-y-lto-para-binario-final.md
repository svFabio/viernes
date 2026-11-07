# HU-RUST-19: Pipeline de optimización guiada por perfiles (PGO) y LTO para binario final

**Puntos de Historia:** 5 | **Prioridad:** Medium | **Versión:** 1 | **Asignado a:** Orquestador / Infraestructura

---

### Descripción
El usuario obtiene un ejecutable de máxima velocidad compilado mediante Profile-Guided Optimization (PGO) y Link-Time Optimization (LTO) para reducir al mínimo el uso de CPU.

---

### Mockup / Flujo de Interacción
```text
[Compilar con -Cprofile-generate]
                 │
                 ▼
[Ejecutar cargas representativas de voz y VAD]
                 │
                 ▼
[Combinar perfiles con llvm-profdata]
                 │
                 ▼
[Recompilar con -Cprofile-use y lto = "fat"] ──► [Binario Optimizado de 35MB]
```

---

### Criterios de Aceptación
1. **Configuración de LTO:** El perfil `release` de Cargo incluye `lto = "fat"`, `codegen-units = 1` y `panic = "abort"`.
2. **Fase de instrumentación PGO:** Genera binario instrumentado con `RUSTFLAGS="-Cprofile-generate=./pgo-data"`.
3. **Fase de entrenamiento:** Ejecuta una sesión simulada de 5 minutos con procesamiento de voz, VAD y streaming.
4. **Fase de optimización:** Genera el binario final utilizando los datos de perfil combinados con `llvm-profdata`.
5. **Medición de ganancias:** Mide y registra en telemetría la reducción de uso de CPU y tiempo de arranque comparado con el build estándar.
6. **Generación de Infografía:** Genera el reporte interactivo `migration_infographic.html` mediante `scripts/telemetry.py`.

---

### Subtareas Técnicas
- **ST-RUST-19.1:** Configurar perfil de release en Cargo.toml con LTO y codegen-units
- **ST-RUST-19.2:** Automatizar pipeline de entrenamiento PGO y recolección de perfiles
- **ST-RUST-19.3:** Medir benchmarks y generar reporte interactivo migration_infographic.html
