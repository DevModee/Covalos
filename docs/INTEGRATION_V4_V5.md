# Integrar el checkpoint de calidad con V4/V5

Base común: 391fe4c (design/aeros-v3). Implementación aislada en improvement/autonomous-quality, dentro de E:/workspace/aeros-autonomous-quality. Build independiente en E:/workspace/aeros-quality-build. El árbol E:/Dev/AEROS/Editor no recibió estos cambios.

El worktree feature/visual-identity-recovery se creó para la directiva anterior y quedó limpio en la base de V3. La directiva posterior concentra los cambios en improvement/autonomous-quality. No hubo merges automáticos ni publicación.

## Procedimiento recomendado

1. Confirmar o preservar el trabajo de V4/V5 y crear otro worktree de integración desde esa rama.
2. Revisar el diff entre 391fe4c y improvement/autonomous-quality.
3. Aplicar los commits de esta rama en orden mediante cherry-pick en el worktree de integración, resolviendo conflictos por intención.
4. Volver a ejecutar cargo fmt --all --check, cargo test --locked y Clippy con warnings como errores.
5. Repetir las capturas nativas con las pantallas y proveedores de V4/V5 antes de reemplazar su ejecutable.

Las zonas con más probabilidad de conflicto son Preferences, Editor::new/render, hub.rs, panels.rs y shell.rs. Conservar los datos y contratos nuevos de V4/V5; incorporar los tokens, estados y componentes visuales de esta rama sin sustituir su orquestador.

## Contratos pequeños para conservar

- theme::Theme y las paletas completas; colores semánticos en todas las superficies.
- Preferences.identity con serde(default) y sanitize, para compatibilidad de configuración.
- Identity::begin decide si repintar; paint solo dibuja. Los relojes de fondo y mascota son independientes.
- Companion recibe snapshots de los agentes internos y del puente. No debe inferir conexión de un CLI instalado.
- agent_bridge solo recibe eventos y responde al ticket revisado; no instala configuración ni ejecuta comandos del payload.
- workspace::cursor_position cuenta escalares Unicode sin copiar el prefijo del buffer.

## Checkpoint y pendientes

Implementados: temas, fondos vectoriales y ASCII configurables, Luma, monitor de eventos, superficies de aprobación, notificaciones, pausa por foco/minimización y optimización de la barra de estado.

Pendiente: conversación persistente y recuperación de sesiones; adaptadores sin hooks; aprobación oficial de otros proveedores; prueba interactiva autenticada de Claude; activación de terminal externo; navegador Obscura; themes externos importables; lector de pantalla, DPI global y movimiento entre monitores; medición de latencia física y carga con varios agentes reales.

Las capturas del contrato de aprobación están identificadas como fixtures. El test autenticado de Codex es independiente. Las mediciones de rendimiento tienen pocos controles válidos de foco y no justifican afirmar una mejora global de velocidad. Ver AUTONOMOUS_IMPROVEMENTS.md.
