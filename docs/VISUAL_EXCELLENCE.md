# AEROS — Visual Excellence checkpoint

Rama: `recovery/visual-excellence`. Worktree: `E:/workspace/aeros-excellence`. Build aislado: `E:/workspace/aeros-excellence-build`.
Base recuperada: `7e0d027` (`design/precision-in-motion`), que contiene en historia lineal todo el trabajo de `improvement/autonomous-quality` (`17a2998`) y de V3 (`391fe4c`). `feature/visual-identity-recovery` estaba limpio en `391fe4c`. No existían ramas ni carpetas V4/V5 separadas: ese nombre aparece solo en `docs/INTEGRATION_V4_V5.md` como plan. No se modificaron ni fusionaron otras ramas.

Framework: eframe/egui 0.33.3 con backend OpenGL (glow), syntect 5.3 y rfd. Sin cambio de framework.

## Qué cambió

| Área | Implementación | Archivos |
| --- | --- | --- |
| Mascota | Luma rediseñada como personaje vectorial original: cápsula squircle iluminada con la órbita abierta de AEROS, ojos en píldora, rotación hacia el cursor, respiración, parpadeo, hover, rebote al clic y ocho estados (Idle, Thinking, Coding, Searching, Waiting, Success, Error, Sleeping) ligados a la actividad real. | `src/editor/mascot.rs`, `identity.rs` |
| Companion | Isla en la barra de estado que se transforma en una tarjeta expandida: chips de estado, medidores de actividad, línea de progreso, operación en monoespaciado, aprobación destacada, historial y toasts con barra de tiempo. | `companion_ui.rs`, `companion.rs` |
| Marcas | Claude, Gemini y Antigravity con marcas oficiales. Codex usa un monograma neutro hasta que el propietario acepte los términos del Blossom de OpenAI. | `brands.rs`, `assets/brands/PROVENANCE.md` |
| Iconos | Material Icon Theme 5.40 (MIT): 116 iconos por nombre y extensión, carpetas especiales abiertas/cerradas, texturas con mipmaps y ajuste a píxel. | `assets.rs`, `icons.rs`, `assets/icons/material/` |
| Sintaxis | Clasificador semántico sobre syntect con paletas por tema, lexer TOML, corrección para TS/JS, línea activa, guías de indentación adaptativas, emparejado de corchetes y cursor de acento. | `syntax.rs`, `panels.rs` |
| Explorador | Filas propias con chevron animado, guías, hover y selección animados. | `panels.rs` |
| Chat y Agent Hub | Identidad del proveedor, burbujas, bloques de código resaltados, sugerencias, botón circular, Enter para enviar, tarjetas de proveedores. | `hub.rs` |
| Home | Diseño hero en ventanas anchas, tarjetas de acción, velo de lectura sobre el fondo. | `shell.rs` |
| Fondos | ASCII Flow reconstruido en dos capas con seis estilos (Nebula, Prism, Drift, Orbit, Contour, Signal); Minimal con retícula de puntos reactiva. | `identity.rs` |
| Idioma | Español/inglés con cambio en vivo, preferencia persistente, idioma del sistema por defecto y prueba de cobertura. | `i18n.rs`, `preferences.rs` |
| Evidencias | Arnés nativo que recorre la app real y escribe `artifacts/visual-review/`. | `evidence.rs` |

## Referencias consultadas

Las publicaciones de X se leyeron mediante la API pública de fxtwitter (texto y miniaturas). Se usaron como dirección: retícula ASCII con celdas teñidas (AlbiaHossain), render de objetos 3D en matriz de puntos (Skirook), nubes de semitono detrás del chat (sarthakguptadev) y mascota cuadrada que gira hacia el cursor (noechague). No se copió arte. Coucou se revisó en la sesión anterior; no se usan sus assets.

## Verificación

Ver `artifacts/visual-review/README.md` y `reports/`.

## Para continuar

Usar esta rama. Ejecutable: `E:/workspace/aeros-excellence-build/release/aeros-editor.exe`. Repetir capturas con `cargo test --release capture_visual_review -- --ignored` y `AEROS_EVIDENCE_DIR`.
