# Próxima iteración

Ordenado por impacto en la experiencia.

1. **Logo de OpenAI/Codex.** Aceptar los términos en openai.com/brand (decisión del propietario), exportar el Blossom monocromo a `assets/brands/openai.png` y activar `mark: Some("brand_openai")` en `src/editor/brands.rs`.
2. **Cobertura de español al 100 %.** Pasar por `trf()` los mensajes con formato de `editor.rs`, `files.rs`, `tools.rs` y `hub.rs` (errores de guardado, conteos de búsqueda, títulos de tareas). La prueba `every_routed_interface_string_has_a_spanish_translation` ya detecta claves faltantes.
3. **Costo de fondos animados.** Orbital y ASCII cuestan 5–10 puntos de un núcleo más que el checkpoint anterior mientras animan (`PERFORMANCE.md`). Cachear la malla de Luma cuando la pose no cambia, agrupar las celdas ASCII en una sola malla de glifos y bajar la cadencia ambiental a 15 fps cuando no hay puntero.
4. **Gramática TypeScript real.** Incorporar una sintaxis `.sublime-syntax` de TypeScript con licencia compatible y cargarla en `syntax.rs` junto a los defaults de syntect.
5. **Diagnósticos LSP.** Advertencias y errores subrayados con los colores `invalid`/`warning` de la paleta, más hover y go-to-definition.
6. **Springs.** Sustituir `animate_value_with_time` por un integrador de resorte amortiguado en `motion.rs` para isla, paneles y pestañas, conservando Reduced Motion.
7. **Transición de tema.** Fundido de 160 ms entre paletas.
8. **Terminal PTY.** ConPTY en Windows para shells interactivos.
9. **Accesibilidad.** Verificar AccessKit con Narrador y navegación completa por teclado del Explorador y de la isla.
10. **Arneses heredados.** `capture_design_review` y `capture_precision_interactions` todavía usan el perfil real del usuario para el puente de agentes. Aislarlos con `AEROS_CONFIG_DIR` como hace `capture_visual_review`.
11. **2560×1440 y DPI global.** Repetir la matriz en un monitor QHD y con escalado de Windows real (no solo `pixels_per_point`).
12. **Latencia física y GPU.** PresentMon o medición de entrada a píxel para latencia tecla→pantalla.
