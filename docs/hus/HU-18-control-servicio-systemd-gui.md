# HU-18: Controlar el ciclo de vida del servicio systemd e inspeccionar su estado desde la interfaz gráfica

**Puntos de Historia:** 3 | **Prioridad:** Medium | **Versión:** 1 | **Asignado a:** Agente Infraestructura & Linux

---

### Descripción
El usuario monitorea el estado de ejecución del asistente e inicia, detiene o reinicia el demonio de usuario mediante la interfaz gráfica de Viernes para gestionar el servicio en segundo plano sin recurrir a comandos de terminal `systemctl`.

---

### Mockup / Flujo de Interacción
```text
┌──────────────────────────────────────────────────────────────────┐
│ VIERNES // CENTRO DE CONTROL             ● Servicio Activo       │
├──────────────────────────────────────────────────────────────────┤
│ [Credenciales & Voz]  [Hardware de Audio]  [Servicio & Estado]   │
│                                                                  │
│  ┌────────────────────────────────────────────────────────────┐  │
│  │ Servicio Systemd: viernes.service                          │  │
│  │ Estado: ACTIVE (En ejecución)                              │  │
│  └────────────────────────────────────────────────────────────┘  │
│                                                                  │
│  [ Iniciar Servicio ]   [ Reiniciar Servicio ]   [ Detener ]     │
│                                                                  │
│  Feedback: Servicio restart completado exitosamente              │
│                                                                  │
├──────────────────────────────────────────────────────────────────┤
│ [ ✔ Guardar Cambios en .env ]                                    │
│ Archivo: /home/svfabio/Projects/09-viernes/.env                   │
└──────────────────────────────────────────────────────────────────┘
```

---

### Criterios de Aceptación
1. **Inspección de estado al inicio:** Al abrir la aplicación o navegar a la pestaña "Servicio & Estado", el sistema debe consultar de forma no bloqueante el comando `systemctl --user is-active viernes.service`.
2. **Insignia de estado visual (Badge):** La cabecera principal y la tarjeta de servicio deben mostrar un indicador en verde con el texto "● Servicio Activo" cuando esté en ejecución, o en gris con "○ Servicio Inactivo" cuando esté detenido.
3. **Ejecución asíncrona de comandos:** Las acciones de "Iniciar", "Reiniciar" y "Detener" deben ejecutarse en un hilo secundario para evitar que la interfaz gráfica se congele durante la invocación de `systemctl`.
4. **Retroalimentación de acción en curso:** Al presionar un botón de acción, debe mostrarse de inmediato un mensaje de progreso (ej. "Ejecutando restart...").
5. **Actualización reactiva tras comando:** Al finalizar la ejecución de la acción de control, la insignia de estado y la descripción textual deben refrescarse automáticamente.
6. **Deshabilitación condicional:** Durante la ejecución de una orden del servicio, los botones de acción deben deshabilitarse temporalmente para prevenir llamadas concurrentes o conflictivas.
7. **Manejo de errores del daemon:** Si `systemctl` retorna un código de salida distinto de 0 o el servicio no está instalado, la interfaz debe mostrar el error en color rojo (ej. "Error: Unit viernes.service not found").
8. **Compatibilidad con entornos sin systemd:** Si el sistema operativo no posee soporte para `systemctl --user`, la interfaz debe alertar que el control de servicio no está disponible sin provocar un cierre abrupto de la aplicación.

---

### Subtareas de Implementación
- **SUB-18.1:** Implementar adaptador no bloqueante para `systemctl --user is-active` en `src/viernes/gui/service.py`.
- **SUB-18.2:** Implementar métodos seguros para despachar `start`, `stop` y `restart` con timeout controlado.
- **SUB-18.3:** Diseñar tarjeta de estado e insignia dinámica con colores semánticos (verde/rojo/gris) en `src/viernes/gui/app.py`.
- **SUB-18.4:** Conectar botones de acción con hilos `threading.Thread` daemon y callbacks `after(0, ...)`.
- **SUB-18.5:** Elaborar tests unitarios simulando salidas exitosas y fallidas de `systemctl` y respuestas de timeout.
