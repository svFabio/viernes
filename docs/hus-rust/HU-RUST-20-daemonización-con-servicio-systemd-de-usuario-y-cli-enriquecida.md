# HU-RUST-20: Daemonización con servicio systemd de usuario y CLI enriquecida

**Puntos de Historia:** 3 | **Prioridad:** Medium | **Versión:** 1 | **Asignado a:** Orquestador / Infraestructura

---

### Descripción
El usuario inicia, detiene o supervisa Viernes como un servicio de fondo en Linux mediante systemd y una interfaz de comandos ergonómica con clap.

---

### Mockup / Flujo de Interacción
```text
$ viernes-rust --daemon
           │
           ▼
[Iniciar bucle en segundo plano gestionado por systemd]
           │
$ systemctl --user status viernes-rust
● viernes-rust.service - Viernes Voice Assistant (Rust Native Daemon)
   Active: active (running)
```

---

### Criterios de Aceptación
1. **CLI con clap:** Provee comandos y flags útiles: `--daemon`, `--new-conversation`, `--test-voice`, `--list-devices`, `--version`.
2. **Manejo de señales Unix:** Intercepta `SIGINT`, `SIGTERM` y `SIGHUP` para realizar un apagado ordenado en < 500 ms.
3. **Unidad systemd nativa:** Provee el archivo `viernes-rust.service` configurado para arranque automático en la sesión de usuario.
4. **Watchdog de salud:** Notifica a systemd el estado de salud mediante el protocolo `sd_notify` si está disponible.
5. **Logs estructurados:** Emite logs a journald en formato limpio con niveles de severidad configurables (`RUST_LOG=info`).
6. **Pruebas de señales:** Pruebas automatizadas validan el shutdown ordenado ante el envío de señales de terminación.

---

### Subtareas Técnicas
- **ST-RUST-20.1:** Implementar CLI con clap y despacho de subcomandos y banderas
- **ST-RUST-20.2:** Implementar signal handling para SIGINT/SIGTERM con apagado ordenado
- **ST-RUST-20.3:** Crear unidad systemd viernes-rust.service y validar en entorno local
