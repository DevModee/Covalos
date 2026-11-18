# AEROS — Autonomous improvements

Base: 391fe4c (V3). Rama: improvement/autonomous-quality. Worktree: E:/workspace/aeros-autonomous-quality. Build: E:/workspace/aeros-quality-build.

El árbol principal permanece en design/aeros-v3. El worktree feature/visual-identity-recovery queda limpio en la misma base; la directiva posterior concentra la implementación en esta rama de calidad. No se han fusionado ramas ni alterado credenciales.

## Ciclo 1 — Temas coherentes

Problema: Settings mostraba un nombre fijo y la aplicación usaba constantes de color en todos los paneles. No existía selección real de tema.

Solución: cinco paletas semánticas completas, selección inmediata, persistencia compatible con preferencias antiguas y sintaxis mapeada a los mismos tokens. Se conserva eframe/egui y las fuentes locales.

Archivos: src/editor/theme.rs, preferences.rs, shell.rs y consumidores visuales.

Validación: 12 pruebas de regresión aprobadas, contraste de texto >= 4,5:1 en las cinco paletas y 60 capturas nativas (12 superficies por tema) a 1366×768. Inspección del editor claro y Home de alto contraste sin clipping observado. Sintaxis clara conserva categorías y contraste. Clippy --all-targets -D warnings aprobado.

Próxima mejora: recuperar fondos y mascota con suspensión automática al perder foco.

## Ciclo 2 — Identidad dinámica con reposo real

Problema: Home solo tenía un dibujo estático; no existían fondos configurables, motor ASCII ni mascota.

Solución: motor vectorial con seis presets (incluido Off), seis patrones ASCII, controles persistentes y Luma original. Relojes independientes, pausa sin foco/minimizado/reduced motion y dibujo detrás del código desactivado por defecto. El patrón Neural usa almacenamiento fijo para sus nodos.

Defecto observado y corregido en captura: Luma se había insertado en la barra superior; se trasladó a la zona del proyecto en la barra de estado.

Archivos: src/editor/identity.rs, editor.rs, shell.rs, panels.rs, hub.rs y preferences.rs.

Pruebas: 14 regresiones aprobadas; reloj de pausa/reanudación y límites de preferencias. Clippy estricto aprobado. Capturas nativas Orbital, ASCII, Neural y Void, incluida ventana 1366×768 a 150 % en modo compacto y movimiento reducido.

Próxima mejora: companion de sesiones y aprobaciones basado en eventos reales.
