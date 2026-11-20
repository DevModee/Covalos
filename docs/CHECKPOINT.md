# AEROS — Checkpoint (2026-11-19)

## Dónde está todo

| Dato | Valor |
| --- | --- |
| Worktree | `E:/workspace/aeros-excellence` |
| Rama | `design/system-rebuild` (sobre `recovery/visual-excellence`, que sigue intacta) |
| Último commit de código | `a875add` (Library Studio); evidencias en el commit siguiente |
| Ejecutable Release | `E:\workspace\aeros-rebuild-build\release\aeros-editor.exe` |
| Directorio de build | `E:\workspace\aeros-rebuild-build` (el antiguo `aeros-excellence-build\release` estaba bloqueado por una instancia abierta) |
| Evidencias | `artifacts/visual-review/design-system/` y `artifacts/library-studio-review/` (copiadas también a `E:\Dev\AEROS\Editor\artifacts\`) |
| Referencias y licencias | `docs/DESIGN_SYSTEM.md`, `assets/brands/PROVENANCE.md`, `assets/icons/ui/LICENSE` |

Reconstruir y verificar (PowerShell, desde el worktree):

```powershell
cargo build --release --target-dir E:\workspace\aeros-rebuild-build
cargo test --target-dir E:\workspace\aeros-excellence-build
$env:AEROS_EVIDENCE_DIR = "artifacts/visual-review/design-system"; cargo test --release --target-dir E:\workspace\aeros-rebuild-build capture_visual_review -- --ignored
$env:AEROS_EVIDENCE_DIR = "artifacts/library-studio-review"; cargo test --release --target-dir E:\workspace\aeros-rebuild-build capture_library_review -- --ignored
```

Estado al cierre: 42 pruebas aprobadas (11 ignoradas: arneses nativos), `cargo clippy --all-targets -- -D warnings` sin avisos, `cargo fmt` aplicado.

## Terminado

Design System (commits `331bc21`, `aa108eb`, `397aca6`):
- Tema base AEROS Nocturne y paleta de sintaxis propia; sintaxis configurable aparte del tema (Nocturne, Graphite, Daylight, High Contrast). Ajustes de densidad, tamaño de fuente e intensidad del movimiento, con vista previa en vivo en Settings.
- Iconos de interfaz Lucide (ISC, 86 iconos); chevron del Explorador rotado; Material Icon Theme para archivos.
- Fondo Dither Field (por defecto) y Dither Bloom: shader GLSL en dos pasadas, estela del cursor y ondas de clic verificadas contra los videos de referencia.
- Paneles como islas redondeadas, pestañas con indicador animado por resorte, motor de springs (`motion.rs`).
- Git agrupado por estado, diffs en el área del editor.
- Clean Code: editores flotantes sobre el mismo buffer, varios a la vez, modo enfoque y acople animado.
- Logo oficial de OpenAI (Blossom) para Codex, en monocromo.

Library Studio (commit `a875add`):
- Fuentes: `src/library.rs` (`SOURCES`). Se sincronizan 10 registros oficiales (2.056 entradas reales en 3–4 s, ETag incremental). Hay 5 fuentes de solo enlace y 7 recursos nativos de AEROS.
- Almacenamiento según la licencia: React Bits, Animate UI y Aceternity guardan solo metadatos; los de pago y los no verificados solo enlace.
- Búsqueda indexada con sinónimos, filtros, favoritos, recientes y estado persistente en `<config>/library/state.json`.
- Detección del proyecto y compatibilidad honesta; un proyecto Rust o Vue no ofrece instalar React.
- Plan de instalación revisable antes de ejecutar: comandos del CLI oficial de shadcn, archivos, conflictos y dependencias. Incluye copia de seguridad, ejecución en streaming, archivos creados y modificados, y reversión.
- API para agentes: `aeros-editor.exe --library search|show|compat|plan|installed|sources|sync` (JSON; los textos de terceros se marcan como datos no confiables; la instalación siempre la aprueba el usuario en la interfaz).
- Accesos: barra de actividad, menú File, paleta de comandos, Agent Hub y atajo configurable (`Ctrl+Shift+L`).

## Pendiente (en orden)

1. **Instalación real de punta a punta.** El plan se generó con datos reales (ver `10-install-review`), pero en esta sesión no se ejecutó `npx shadcn add` en un proyecto temporal con `npm install` completo. Probar con un proyecto Vite+React temporal y capturar los estados Done y Revert.
2. **Benchmark válido del Dither Field en dos pasadas.**
   - La primera medición (una pasada) dio 136–151 % de un núcleo con la GPU AMD integrada. Eso motivó el cambio a dos pasadas.
   - La medición posterior se invalidó porque las ventanas no tuvieron el foco.
   - Para repetirla: `python -I tools/bench_process.py <exe anterior> <exe nuevo> <salida.json>`, sin otra actividad en el escritorio. Script: `tools/bench_process.py`.
3. **Vista previa web aislada** para componentes React (WebView2 en proceso separado). Hoy las tarjetas web muestran un esquema honesto y "Open live demo".
4. **Integración de agentes.** La API JSON existe, pero falta inyectar la guía de uso en el contexto de Codex/Claude desde Agent Hub.
5. **Detalles menores:**
   - El comando del plan se muestra subrayado (el resaltador lo trata como texto inválido): usar una etiqueta sin resaltado.
   - El mensaje del commit `a875add` perdió la línea del comando CLI por las comillas invertidas.
6. **Arneses heredados:** `capture_design_review` y `capture_precision_interactions` todavía no aíslan `AEROS_CONFIG_DIR`.
7. **No disponible en esta sesión:** la captura del editor minimalista que pidió el usuario como referencia de la paleta; Nocturne se calibró con la especificación escrita.
