# HU-15: Ejecutar el asistente como servicio de usuario en segundo plano (systemd --user)

**Puntos de Historia:** 3 | **Prioridad:** Medium | **Versión:** 1 | **Asignado a:** Agente Infraestructura / Linux

---

### Descripción
El usuario inicia y mantiene corriendo al asistente como un servicio de fondo en su sesión de usuario de Linux, con arranque automático al iniciar sesión, reinicio ante caídas y consulta centralizada de logs mediante herramientas estándar del sistema (`systemctl` y `journalctl`).

---

### Mockup / Flujo de Interacción
```text
~/.config/systemd/user/viernes.service
                  │
                  ▼
Comandos del usuario:
$ systemctl --user start viernes     # Iniciar servicio
$ systemctl --user status viernes    # Ver estado y PID activo
$ systemctl --user stop viernes      # Detener limpiamente
$ journalctl --user -u viernes -f    # Inspección de logs en vivo
```

---

### Criterios de Aceptación
1. **Archivo de unidad systemd:** El proyecto incluye la plantilla `viernes.service` configurada con `Type=simple`, `Restart=on-failure`, y `RestartSec=5s`.
2. **Entorno de ejecución aislado con uv:** La directiva `ExecStart` invoca el entrypoint del asistente a través del binario de `uv` dentro del directorio del proyecto (`uv run python -m viernes.main`).
3. **Paso de variables de entorno:** El servicio tiene acceso a las variables de entorno de la sesión gráfica y de audio (`DISPLAY`, `WAYLAND_DISPLAY`, `XDG_RUNTIME_DIR`, `PULSE_SERVER`).
4. **Script de instalación y desinstalación:** Se provee un script o comando documentado para crear el enlace simbólico en `~/.config/systemd/user/` y recargar el daemon con `systemctl --user daemon-reload`.
5. **Arranque automático opcional:** Se documenta y verifica la habilitación persistente al iniciar sesión (`systemctl --user enable viernes.service`).
6. **Detención limpia (SIGINT/SIGTERM):** Al recibir la orden de parada, el servicio finaliza ordenadamente sus hilos de audio y sockets sin corromper archivos ni dejar procesos colgados.
7. **Acceso a logs centralizado:** Los eventos de arranque, errores y advertencias son legibles mediante `journalctl --user -u viernes.service`.
