# AEROS — evidencias visuales

| Dato | Valor |
| --- | --- |
| Versión auditada | AEROS Editor 0.3.0 (`Cargo.toml`), checkpoint Visual Excellence |
| Commit del código capturado | `86a4062` |
| Rama | `recovery/visual-excellence` (worktree `E:/workspace/aeros-excellence`) |
| Base recuperada | `7e0d027` (`design/precision-in-motion`) |
| Fecha | 2026-11-19 |
| Sistema operativo | Windows 11 Pro 10.0.26200, monitor 1920×1080 |
| Framework gráfico | eframe/egui 0.33.3, backend OpenGL (glow); syntect 5.3; rfd |
| Ejecutable | `E:/workspace/aeros-excellence-build/release/aeros-editor.exe` |

Todas las imágenes salen del framebuffer de la aplicación real (`ViewportCommand::Screenshot`). No hay mockups ni retoques. Los archivos de `06-mascot/` son recortes de la captura completa ampliados ×5 con Lanczos, y así figuran en el manifiesto (`upscaled_crop: true`). Los archivos `before-precision-*` son capturas reales del checkpoint anterior (`7e0d027`), copiadas de `design/precision-captures/final/dark/`.

Cada nombre termina en `-<ancho>x<alto>@<escala %>`. Tema, idioma, píxeles, puntos lógicos, escala y milisegundos desde la última acción de cada captura están en `reports/manifest-*.json`.

## Reproducir

PowerShell, desde el worktree:

```powershell
cargo build --release --target-dir E:\workspace\aeros-excellence-build
$env:AEROS_EVIDENCE_DIR = "artifacts/visual-review"
$env:AEROS_TEST_EXE = "E:\workspace\aeros-excellence-build\release\aeros-editor.exe"
cargo test --release --target-dir E:\workspace\aeros-excellence-build capture_visual_review -- --ignored
```

Matriz de resolución y escala: agregar `$env:AEROS_EVIDENCE_SET = "matrix"` y `AEROS_EVIDENCE_WIDTH`, `AEROS_EVIDENCE_HEIGHT`, `AEROS_EVIDENCE_SCALE` (1, 1.25, 1.5).

El arnés (`src/editor/evidence.rs`) crea un proyecto temporal con Rust, TypeScript, Python, TOML, JSON, Markdown, Docker, Git, una solución y carpetas especiales, usa un perfil temporal (`AEROS_CONFIG_DIR`) y, si se indica `AEROS_TEST_EXE`, lanza un proceso real `--agent-hook` con una solicitud de prueba que nunca ejecuta herramientas.

## Índice

| Carpeta | Contenido |
| --- | --- |
| `01-home/` | Home con fondo Orbital, hover de tarjeta, antes/después |
| `02-explorer/` | Árbol expandido con iconos Material |
| `03-editor/rust/` | Rust y TOML, antes/después |
| `03-editor/typescript/` | TypeScript |
| `03-editor/python/` | Python |
| `03-editor/command-palette-*` | Command Palette |
| `04-agent-hub/` | Conversación, tareas, panel de Codex vacío, antes/después |
| `05-companion/` | Isla compacta y tarjeta expandida con aprobación, antes/después |
| `06-mascot/` | Luma: idle, thinking, coding, searching, waiting, success, error, sleeping, hover, clic |
| `07-themes/` | Editor en los cinco temas, Home en Light y High Contrast |
| `08-ascii-backgrounds/` | Orbital, Neural, Void, Minimal y ASCII Flow: Nebula, Prism, Drift, Orbit, Contour, Signal; Nebula en Light |
| `09-settings-es/` | Settings, Home, editor, Agent Hub y Companion en español |
| `10-motion/` | Secuencias: cambio de pestaña, apertura de paleta, apertura y cierre del Companion, lente y onda de clic en ASCII |
| `reports/` | `VISUAL_AUDIT.md`, `FEATURE_STATUS.md`, `PERFORMANCE.md`, `NEXT_STEPS.md`, manifiestos y muestras de rendimiento |

Resoluciones capturadas: 1366×768 al 100 % (conjunto completo), 1920×1080 al 100/125/150 % y 1366×768 al 125/150 % (Home, Explorador, editor Rust, Agent Hub, Settings en español).

## Limitaciones

- La entrada se inyecta en la ventana nativa mediante `raw_input_hook` de eframe; el control del escritorio no fue autorizado, así que no hay pruebas con ratón físico ni SendInput.
- 2560×1440 no se pudo capturar: el monitor es de 1920×1080 y Windows limita el tamaño de la ventana.
- Las escalas 125/150 % usan `pixels_per_point` de egui, no el DPI global de Windows.
- El logo de OpenAI/Codex no se incluyó: requiere aceptar los términos de OpenAI. Codex usa un monograma neutro.
- No se capturaron sesiones autenticadas de proveedores; la conversación de Agent Hub es un fixture de texto.
- Los videos no se generaron (no hay codificador disponible); el movimiento se documenta con secuencias de frames reales.
