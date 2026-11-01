# HU-16: Gestionar credenciales de ElevenLabs y catálogo de voces desde la interfaz gráfica

**Puntos de Historia:** 3 | **Prioridad:** High | **Versión:** 1 | **Asignado a:** Agente Frontend & GUI

---

### Descripción
El usuario administrador configura su API key de ElevenLabs y selecciona la voz del asistente mediante el Centro de Control Gráfico de Viernes para personalizar la síntesis de voz y autenticar el servicio sin manipular manualmente archivos de texto en la terminal.

---

### Mockup / Flujo de Interacción
```text
┌──────────────────────────────────────────────────────────────────┐
│ VIERNES // CENTRO DE CONTROL             ● Servicio Activo       │
├──────────────────────────────────────────────────────────────────┤
│ [Credenciales & Voz]  [Hardware de Audio]  [Servicio & Estado]   │
│                                                                  │
│  ElevenLabs API Key:                                             │
│  [ sk_808ad725799d106b95dd40636aeaeb901a9... ]  [ Ver / Ocultar]│
│                                                                  │
│  Voz de Viernes:                                                 │
│  [ Jessica (Oficial)                                        ▼ ]  │
│                                                                  │
│  Voice ID de ElevenLabs:                                         │
│  [ cgSgspJ2msm6clMCkdW9                                       ]  │
│                                                                  │
│  [ ▶ Probar Voz (Audición en Vivo)                            ]  │
│  ✔ Reproducción completada exitosamente.                         │
│                                                                  │
├──────────────────────────────────────────────────────────────────┤
│ [ ✔ Guardar Cambios en .env ]                                    │
│ Archivo: /home/svfabio/Projects/09-viernes/.env                   │
└──────────────────────────────────────────────────────────────────┘
```

---

### Criterios de Aceptación
1. **Acceso y Renderizado inicial:** Al abrir el Centro de Control (`viernes-gui`), la pestaña "Credenciales & Voz" debe renderizar el campo de clave, el botón de visibilidad, el selector desplegable de voces y el campo de Voice ID con los valores actuales del archivo `.env`.
2. **Máscara de seguridad de clave:** El campo de la API key debe inicializarse en modo enmascarado (`show="*"`), ocultando los caracteres sensibles por defecto.
3. **Alternancia de visibilidad (Toggle):** Al hacer clic en "Ver", el campo debe mostrar el texto plano de la clave y el botón debe cambiar a "Ocultar". Un segundo clic debe restaurar la máscara inmediatamente.
4. **Catálogo de voces preconfiguradas:** El menú desplegable debe listar de forma obligatoria las opciones: `Jessica (Oficial)`, `Lily (Cálida)`, `Bella (Expresiva)`, `Laura (Conversacional)`, `Alice (Directa)`, `Sarah (Suave)` y `Personalizada (Custom ID)`.
5. **Autocompletado de Voice ID:** Al seleccionar cualquiera de las voces preconfiguradas, el campo "Voice ID" debe actualizarse de forma automática con su identificador oficial de ElevenLabs (ej. `cgSgspJ2msm6clMCkdW9` para Jessica).
6. **Modo personalizado (Custom Voice):** Al seleccionar "Personalizada (Custom ID)", el campo "Voice ID" debe quedar habilitado para la edición libre por parte del usuario.
7. **Validación de formato de clave:** Al intentar guardar, si la API key contiene caracteres no alfanuméricos incompatibles o espacios residuales, estos deben ser saneados (`strip()`), alertando si la longitud es inferior a 20 caracteres.
8. **Persistencia atómica en .env:** Al presionar "Guardar Cambios", los valores de `ELEVENLABS_API_KEY` y `ELEVENLABS_VOICE_ID` deben escribirse en `.env` preservando los comentarios y variables no modificadas.
9. **Feedback visual de éxito:** Tras un guardado exitoso, la barra inferior debe mostrar el mensaje en verde "✔ Cambios guardados exitosamente en .env".
10. **Manejo de errores de escritura:** Si el archivo `.env` no tiene permisos de escritura o el sistema de archivos es de solo lectura, la GUI debe mostrar un mensaje de error en rojo sin crashear.

---

### Subtareas de Implementación
- **SUB-16.1:** Crear parser y validador de formato para credenciales de ElevenLabs en `src/viernes/gui/service.py`.
- **SUB-16.2:** Implementar componente de entrada con botón de alternancia mostrar/ocultar en `src/viernes/gui/app.py`.
- **SUB-16.3:** Conectar el menú desplegable de voces con el catálogo estático y auto-relleno del Voice ID.
- **SUB-16.4:** Implementar escritura atómica conservativa en `.env` mediante `dotenv.set_key`.
- **SUB-16.5:** Crear pruebas unitarias automatizadas cubriendo carga inicial, toggle de máscara y persistencia en `.env`.
