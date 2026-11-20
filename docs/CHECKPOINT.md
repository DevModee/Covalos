# COVALOS — Checkpoint (2026-11-20)

## Estado y ubicaciones

| Dato | Valor |
| --- | --- |
| Worktree activo | `E:/workspace/covalos-release` |
| Rama | `feat/covalos-rebrand` (sobre `design/system-rebuild` `f390e31`, que queda intacta) |
| Identidad Git local | `DevModee <caelien2.0@gmail.com>` |
| Paquete / versión | `covalos` `0.4.0` |
| Ejecutable Release | `E:\workspace\covalos-build\release\covalos.exe` |
| Directorio de build | `E:\workspace\covalos-build` |
| Evidencias de verificación | `artifacts/covalos-verification/design-system/`, `artifacts/covalos-verification/library-studio/`, `artifacts/covalos-verification/library-e2e-report.json`, `artifacts/covalos-verification/dither-benchmark.json` |
| Identidad visual (provisional) | `assets/brand/` (`covalos-mark.svg`, `covalos-icon.ico`, `covalos-app-256.png`, `covalos-mark-256.png`, `covalos-mark-mono-64.png`, `PROVENANCE.md`) |

Reconstruir y verificar (PowerShell, desde `E:\workspace\covalos-release`):

```powershell
cargo fmt --check
cargo clippy --all-targets --target-dir E:\workspace\covalos-build -- -D warnings
cargo test --target-dir E:\workspace\covalos-build
cargo build --release --target-dir E:\workspace\covalos-build
$env:COVALOS_EVIDENCE_DIR = "artifacts/covalos-verification/design-system"; $env:COVALOS_EVIDENCE_SET = "matrix"; cargo test --release --target-dir E:\workspace\covalos-build capture_visual_review -- --ignored
$env:COVALOS_EVIDENCE_DIR = "artifacts/covalos-verification/library-studio"; cargo test --release --target-dir E:\workspace\covalos-build capture_library_review -- --ignored
```

## Completado en esta entrega

1. **Rebranding integral AEROS → COVALOS:**
   - Paquete `covalos` (`covalos.exe`), título de ventana `COVALOS`, temas `COVALOS Nocturne / Graphite / Midnight / Violet / Light`, pantalla de inicio, modal **About COVALOS** (`Archivo → Acerca de COVALOS` y Quick Access `> About COVALOS`), Library Studio (`covalos/*`), CLI (`covalos.exe --library ...`), traducciones EN/ES y documentación.
   - Metadatos PE de Windows (`ProductName = COVALOS`, `FileDescription = COVALOS IDE`, `OriginalFilename = covalos.exe`, `FileVersion = 0.4.0.0`) e ícono multicapa `.ico` (16/24/32/48/64/256 px) embebidos nativamente desde `build.rs` sin dependencias externas ni `rc.exe`.
2. **Migración segura de configuración (`src/paths.rs`):**
   - Directorio unificado (`%LOCALAPPDATA%\Covalos`, `COVALOS_CONFIG_DIR` con fallback retrocompatible a `AEROS_CONFIG_DIR`).
   - Migración automática única y no destructiva desde `AerosEditor` (`editor.json`, `conversations/`, `library/`, `agent-bridge/`) con marcador `.migrated-from-aeros`: nunca sobrescribe datos existentes en `Covalos`, nunca borra `AerosEditor`, no sigue symlinks y normaliza IDs `aeros/*` → `covalos/*` en `library::State` y `Catalog::get`.
   - Variables de override de proveedores actualizadas a `COVALOS_*_BIN` con fallback a `AEROS_*_BIN`.
3. **Identidad visual centralizada (provisional):**
   - Módulo único `src/brand.rs` + recursos en `assets/brand/` y script reproducible `tools/generate_brand_assets.py`.
   - Isotipo provisional basado en el refinamiento *Bond Prism* (Variante 03 — *Precision Facet C*: superelipse sin hexágono Web3, corte diagonal limpio y núcleo facetado contenido). Reemplazar el SVG maestro y correr el script actualiza todas las superficies y el `.ico`.
4. **Library Studio — prueba real de punta a punta y correcciones:**
   - Corregido el bloque de comandos en la vista de revisión de instalación (`command_block` en `src/editor/studio.rs` con texto monoespaciado seleccionable envuelto y botón Copiar, eliminando el subrayado de sintaxis inválida).
   - `library::backup` y `library::revert` endurecidos para respaldar también archivos conflictivos antes de sobrescribir, recrear directorios padre al restaurar y podar carpetas vacías creadas durante la instalación.
   - Ejecutada instalación real con `npm` + `covalos.exe --library sync shadcn` + `plan` + `npx shadcn@latest add` en un proyecto Vite+React+Tailwind temporal: creó `src/components/ui/button.tsx`, instaló `cn` y `radix-ui`, y la reversión restauró `package.json`, `package-lock.json` y el árbol de archivos bit a bit.
5. **Dither Field — optimización y benchmark real:**
   - `src/editor/dither.rs`: ubicaciones de los 17 uniforms OpenGL cacheadas al compilar shaders y buffers `trail`/`ripples` pasados a arrays en stack sin asignaciones heap por cuadro.
   - `src/editor/identity.rs`: guardia adicional de tamaño de viewport para pausar animación en ventanas colapsadas.
   - Benchmark reproducible (`tools/bench_process.py`) en AMD Ryzen 5 5600G + Radeon integrada (`1920×1080`, ventana `1390×860`):
     - `Dither Field` (24 FPS en estado estacionario): **24,97 %** de 1 núcleo (~2,1 % del CPU de 12 hilos), pico **34,35 %** (vs **43,74 %** en el build anterior y **136–151 %** en la primera versión de 1 pasada), GPU **6,09 %**, RAM **102,45 MiB**.
     - `Dither Field` con barrido continuo del mouse: **24,69 %** de 1 núcleo, pico **36,65 %**.
     - `Background Off` / `Reduced Motion`: **0,00 %** CPU, **0,00 %** GPU, **92,6–96,6 MiB**.
6. **Arneses heredados aislados:** `capture_design_review` y `capture_precision_interactions` ahora aíslan `COVALOS_CONFIG_DIR` en un perfil temporal.

## Pendiente para próximas fases

1. **Aprobación definitiva del isotipo:** elegir entre las 3 variantes refinadas de *Bond Prism* (o ajustes finales) y regenerar `assets/brand/` con `python tools/generate_brand_assets.py`.
2. **Pausa inmediata al minimizar ventana borderless en Windows:** cuando no existe otra ventana en primer plano (`GetForegroundWindow() == 0`), `ShowWindow(SW_MINIMIZE)` sobre ventana sin decoraciones en `winit` todavía dejó ~7,8 % CPU residual en `Dither Field`; investigar consulta nativa `IsIconic` en Windows o evento de minimizado de `winit`.
3. **Orquestador multiagente completo:** adaptadores headless reales de Gemini CLI y Antigravity (`agy`), más inyección de guía de CLI de Library Studio en contexto de agentes desde Agent Hub.
4. **Vista previa web aislada (WebView2)** para componentes React en Library Studio.
5. **Capacidades de IDE avanzado:** LSP, depuración (DAP), terminal con PTY real y soporte de extensiones.
