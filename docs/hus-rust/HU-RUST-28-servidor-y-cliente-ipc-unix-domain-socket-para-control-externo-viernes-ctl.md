# HU-RUST-28: Servidor y cliente IPC Unix Domain Socket para control externo viernes-ctl

**Puntos de Historia:** 3 | **Prioridad:** Medium | **Versión:** 1 | **Asignado a:** Agente Orquestador

---

### Descripción
El usuario controla y consulta el estado del daemon Viernes desde atajos de teclado o scripts mediante el cliente de línea de comandos viernes-ctl sobre Unix Domain Sockets.

---

### Mockup / Flujo de Interacción
```text
$ viernes-ctl press
          │ (Conexión /tmp/viernes_ptt.sock)
          ▼
     ┌────────────────────────┐
     │ Daemon IPC Server      │ ── Despacha PttPressed & BargeIn
     └────────────────────────┘
          │
          ▼
     [Captura de voz iniciada en sub-10ms]
```

---

### Criterios de Aceptación
1. **Comandos IPC soportados:** `press`, `release`, `cancel`, `status`, `reload`.
2. **Subcomando CLI:** `viernes ctl <COMMAND>` envía el comando al socket y reporta el status con código de salida 0 en éxito.
3. **Protocolo ligero:** Formato texto plano delimitado por `
` o NDJSON de baja sobrecarga.
4. **Respuesta rápida:** Tiempo de respuesta entre comando y procesamiento en el daemon < 10 ms.
5. **Pruebas de integración:** Prueba de socket cliente/servidor punta a punta automatizada.

---

### Subtareas Técnicas
- **ST-RUST-28.1:** Implementar subcomando CLI 'ctl' con cliente de socket Unix
- **ST-RUST-28.2:** Estandarizar parser de comandos IPC en el servidor del daemon
- **ST-RUST-28.3:** Pruebas de integración cliente-servidor con sockets temporales
