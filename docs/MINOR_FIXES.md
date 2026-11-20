# COVALOS — Minor Fixes checkpoint (2026-11-20)

| Dato | Valor |
| --- | --- |
| Worktree | `E:/workspace/covalos-minor-fixes` |
| Rama | `fix/minor-fixes` (desde `feat/covalos-rebrand` `733627a`; el trabajo del instalador sin commit en `E:/workspace/covalos-release` no se tocó) |
| Ejecutable Release | `E:\workspace\covalos-fixes-build\release\covalos.exe` |
| Evidencias | `artifacts/minor-fixes/` (50+ capturas reales del arnés `capture_visual_review`, 1366×768 @100 %) |
| Pruebas | 46 aprobadas, 11 ignoradas (arneses nativos); `clippy -D warnings` y `fmt` limpios |

Estados según el roadmap §6.1: **I** Implemented · **T** Tested · **V** Visually verified.

| Fix | Estado | Detalle |
| --- | --- | --- |
| MF-01 Dither Field por defecto | I T V | Perfil nuevo: Dither Field. Perfiles migrados que guardaron el viejo default `orbital` sin haberlo elegido pasan una vez a Dither Field; elegir un fondo en Settings marca `identity.background_chosen` y se respeta para siempre (prueba `preferences_round_trip_and_bound_legacy_values`). El fondo ya no corre cuando no se ve (antes Settings abierto sobre el editor lo mantenía activo). |
| MF-02 Orbital ASCII refinado | **Pendiente** | Orbital sigue disponible como alternativa, sin rediseño. |
| MF-03 Home centrado | I V | Columna única centrada en el área de contenido (no en la ventana), ancho máximo 640, 2→1 columnas en angosto, posición vertical calculada con contenido conocido (sin saltos al cargar recientes). |
| MF-04 Header unificado | I V | Un único **Agents** reemplaza `Codex` + `Agent Hub` (barra superior; tarjeta de inicio; se quitó el duplicado de la barra lateral). El chat lateral se abre con `Ctrl+Shift+A`. El buscador se centra en la ventana, independiente del ancho de los extremos, y se oculta antes de chocar. |
| MF-05 Zoom independiente | I T V | Tres nociones separadas y persistentes: `font_size` (editor, 11–26), `explorer_zoom` (0,8–1,6, escala filas, iconos, sangrías y texto del árbol) y `ui_scale` (0,8–1,5, escala global). Se desactivó el zoom global de teclado de egui, que era la causa del efecto cruzado. `Ctrl+rueda` / `Ctrl+±` / `Ctrl+0` actúan sobre el panel bajo el puntero. Capturas `explorer-zoom-140-code-unchanged` y `editor-zoom-18pt-explorer-unchanged`. |
| MF-06 Límites de paneles | I (parcial) | El Explorador calcula su máximo según la ventana para no comprimir el editor por debajo de su mínimo y escala su mínimo con el zoom. **Falta** auditar terminal, dock de Agents y Clean Code a 125/150/200 % de DPI real de Windows. |
| MF-07 Transiciones | I | Settings › Navigation transitions: Off / Subtle (160 ms, default) / Expressive (280 ms). Fundido al pasar a un workspace y al abrir el primer archivo; no agrega espera al I/O; Off es instantáneo; respeta Reduce motion e intensidad. **Falta** una secuencia capturada y medir time-to-interactive. |
| MF-08 Discord Rich Presence | I T · **bloqueado para verificación** | Transporte IPC local documentado por Discord (`\\.\pipe\discord-ipc-N`), desactivado por defecto, sin rutas ni nombres de archivo; nombre del proyecto solo con opt-in; estados: Editing <lenguaje>, Reviewing changes, Coordinating agents, In Library Studio, Working on a project; limpieza al desactivar o cerrar. Requiere un **Application ID** registrado por el propietario en el Discord Developer Portal (y subir el asset `covalos`); sin eso no puede verificarse en un cliente real. Se eligió IPC sobre el Social SDK porque no requiere SDK binario ni OAuth para presencia local; revisar si Discord exige el Social SDK para apps nuevas. |
| MF-09 Diseño fino | Parcial | Home y header corregidos; resto pendiente. |
| MF-10 Pendientes heredados | Cubierto por la rama `feat/covalos-rebrand` | Instalación real `npm` + rollback y benchmark enfocado ya hechos allí (`artifacts/covalos-verification/library-e2e-report.json`, `dither-benchmark.json`: Dither Field ~25 % de un núcleo en estado estable). No se repitió. El instalador de Windows está en curso, sin commit, en `covalos-release`. |

## Siguiente sesión

1. Integrar: `fix/minor-fixes` se basa en `feat/covalos-rebrand`; cuando el trabajo del instalador se confirme allí, hacer merge o rebase de esta rama.
2. MF-02 (Orbital ASCII), MF-06 a DPI reales, secuencia capturada de MF-07.
3. MF-08: registrar la app COVALOS en Discord, pegar el Application ID en Settings y verificar en un cliente real.
4. Luego V4 (Agents multiproveedor), según `COVALOS_MASTER_ROADMAP_V4_V5_V6.md`.
