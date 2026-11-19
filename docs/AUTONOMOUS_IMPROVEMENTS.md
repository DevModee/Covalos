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

## Ciclo 3 — Supervisión y decisiones observables

Problema: V3 no tenía una vista compacta de sesiones ni un puente para eventos externos. Los estados no estaban disponibles fuera del chat.

Solución: Agent companion y Live agents, snapshots de ejecuciones internas, mensajes de ciclo de vida, contador compacto, notificaciones y atención en la barra de tareas. Puente local de hooks compatible con nombres canónicos; decisiones Claude de una sola acción, tickets, caducidad y auditoría. Ningún ajuste de proveedor se instala automáticamente.

Archivos: src/agent_bridge.rs, src/editor/companion.rs, main.rs, editor.rs, hub.rs, panels.rs, shell.rs y preferences.rs.

Verificación: pruebas de estados, decisiones duplicadas, tickets cambiados y vencimiento. Assist autenticado de Codex aprobado: propuesta pendiente y archivo en disco conservado. Captura nativa del hook del ejecutable con payload de prueba explícito y rechazo posterior.

Límites: otros proveedores conservan sus aprobaciones en el terminal. Las sesiones externas muestran el último evento recibido. No hay detección de sesiones sin hooks ni activación exacta de terminal externo. Ver AGENT_COMPANION.md.

## Ciclo 4 — Coste de la barra de estado en buffers grandes

Evidencia: cada frame construía un String con todo el contenido anterior al cursor, y después lo recorría dos veces para contar línea y columna. Cerca del final de un buffer de 1 MiB esto asignaba casi 1 MiB por frame.

Solución: contar posiciones Unicode en una única pasada sin construir el prefijo. Archivos: src/workspace.rs y src/editor/panels.rs.

Validación: Unicode, nuevas líneas y cursor fuera de rango; 18 regresiones finales aprobadas y Clippy estricto aprobado. Microbenchmark Release, 810.000 bytes y 200 iteraciones: 258,039 ms con copia frente a 90,747 ms con conteo directo. Es una medición de esta función, no de latencia total de escritura.

Próximas mejoras: persistencia y recuperación de conversaciones; validación interactiva de hooks Claude; comparación de DPI global entre monitores.

## Rendimiento observado

El primer benchmark se descartó para conclusiones porque no distinguía bien la ventana principal y hubo cambios de foco. Se conservó como first-pass. El benchmark corregido filtra las muestras por foco o minimización comprobados. Los resultados y las muestras completas están en [performance-quality-windows.json](../design/performance-quality-windows.json).

| Home del workspace | Muestras válidas | RAM MiB | CPU % de un núcleo |
| --- | --- | --- | --- |
| V3 estático | 1 | 83,50 | 0 |
| Orbital a 20 FPS | 1 | 88,45 | 13,22 |
| Orbital a 10 FPS | 1 | 87,47 | 8,41 |
| Orbital estático | 1 | 83,65 | 0 |
| Efectos desactivados | 2 | 83,68 | 0 |
| Ventana minimizada | 2 | 84,19 | 0 |

Las configuraciones activas solo conservaron una muestra válida: sirven para detectar coste, no para afirmar una mejora estadística. Se eligieron 10 FPS por defecto por el menor coste observado; se puede elegir entre 5 y 30. El tiempo hasta WaitForInputIdle fue aproximadamente 0,24 s en la muestra válida de V3 y 0,30–0,32 s en las configuraciones nuevas. No se afirma una mejora de arranque.

La CPU es porcentaje de un núcleo durante 1,3 segundos y no porcentaje total del equipo. El 0 representa la ventana de medición. No se midió latencia física de teclado, tiempo GPU, VRAM ni un escenario autenticado con varios agentes simultáneos.

Revisión visual final: se detectó que el icono del proyecto usaba una posición absoluta y se superponía a Luma. Ahora ocupa una celda propia después del companion y los contadores.

## Evidencia visual final

La carpeta final contiene las pantallas vigentes; las carpetas de temas y de identidad documentan los ciclos previos. Los PNG son capturas de ventanas nativas.

- [Home Orbital 1920×1080](../design/quality-captures/final/orbital-1920x1080-100/home.png)
- [Editor 1920×1080](../design/quality-captures/final/orbital-1920x1080-100/editor.png)
- [ASCII 1366×768 al 125 %](../design/quality-captures/final/ascii-1366x768-125/home.png)
- [Settings claro 1366×768 al 150 %](../design/quality-captures/final/light-1366x768-150/settings.png)
- [Aprobación del hook de prueba](../design/quality-captures/final/orbital-1920x1080-100/companion.png)
- [Home V3 anterior](../design/captures/after/1920x1080-100/home.png)

El 125/150 % usa escalado de egui en Windows; queda pendiente cambiar el DPI global y mover la ventana entre monitores. Las capturas motion-start/end registran dos instantes reales del motor.


## Ciclo 5 — Continuación: conversación recuperable por proyecto

Retomado el 18 de noviembre de 2026 desde 7dbd2d5. Se verificaron los tres worktrees: principal en design/aeros-v3, visual-identity-recovery sin cambios y esta rama limpia. Continúa la implementación en improvement/autonomous-quality. El Release de esta continuación se genera en E:/workspace/aeros-quality-resumed-build para conservar el ejecutable anterior que estaba abierto.

Problema: el historial, borrador e ID de sesión solo vivían en memoria. Cambiar de proyecto o reiniciar el editor descartaba la conversación de Workspace. Además, New chat conservaba una propuesta anterior pendiente de aprobación.

Solución: historial local separado por ruta canónica de proyecto, guardado atómico junto a la configuración, autoguardado tras 750 ms sin modificaciones y guardado inmediato al cambiar de proyecto o cerrar. Conserva hasta 100 mensajes, 1 MiB de texto y un borrador de hasta 48 KB; rechaza archivos mayores de 2 MiB, versiones incompatibles y rutas que no corresponden. Los IDs de sesión se validan antes de recuperarlos. No se guarda ni ejecuta una aprobación o workflow. Una solicitud interrumpida muestra un aviso; el proveedor permanece desconectado hasta que el usuario envía otro mensaje. New chat limpia el historial persistido y rechaza propuestas pendientes.

Errores de lectura: el archivo original se conserva y no se sobrescribe automáticamente. Si falla un guardado al cerrar, se puede reintentar, seguir editando o cerrar descartando explícitamente los cambios del chat. Un cambio de proyecto con fallo de guardado conserva el proyecto y conversación actuales.

Archivos: src/editor/sessions.rs, editor.rs, hub.rs, visual_tests.rs y assistant/providers.rs.

Validación: 23 pruebas de regresión aprobadas (incluyen reinicio de Editor, cambio A→B→A, New chat, aislamiento, Unicode, corrupción, límite de tamaño, recuperación sin ejecución y descarte de propuestas); Clippy --all-targets -D warnings y Release aprobados. La captura nativa de recuperación también pasó y se inspeccionó: [history-restored.png](../design/quality-captures/recovery/history-restored.png). Es un fixture identificado, sin llamada a un proveedor real; no demuestra una nueva autenticación o un resume remoto.

Límites: esta fase conserva la conversación de Workspace. General sigue usando un directorio temporal y no se persiste. No se recuperan respuestas parciales que solo estaban en streaming; un cierre forzado puede perder los últimos 750 ms de cambios del historial o borrador. La disponibilidad del ID remoto depende del proveedor y su cuenta. No se repitieron las pruebas autenticadas de pago ni se atribuye una mejora de rendimiento global a este ciclo.

Checkpoint: continuar desde esta rama, no fusionada, para recuperación de conversaciones General con aislamiento persistente, hooks Claude autenticados, DPI entre monitores y activos de marca/referencias visuales todavía pendientes. Ejecutable de la continuación: E:/workspace/aeros-quality-resumed-build/release/aeros-editor.exe.

Prueba del ejecutable: apertura nativa y cierre limpio con código 0, usando un proyecto y perfil temporales propios. Reproducible con design/smoke_recovery_windows.py <ruta-del-exe>. La prueba espera la inicialización de la ventana antes de solicitar el cierre; no cierra otras instancias.

## Continuación visual unificada — Precision in Motion

La integración de Visual Perfection + Project Apex continúa desde 17a2998 en design/precision-in-motion, dentro de E:/workspace/aeros-precision-motion, con build E:/workspace/aeros-precision-build. Se conservan todos los ciclos previos y la recuperación de conversaciones. Ver [sistema visual, evidencia y pendientes](PRECISION_IN_MOTION.md). No se fusionó con las ramas activas.
