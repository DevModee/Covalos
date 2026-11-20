# COVALOS — Checkpoint Cloud: motor autónomo V5 (2026-11-20)

## Estado

| Dato | Valor |
| --- | --- |
| Rama | `feat/autonomous-engine`, creada desde `feat/cloud-agents-foundation` |
| Base previa | `feat/cloud-agents-foundation` (PR #1, borrador) |
| Código nuevo | `crates/covalos-engine/` (crate independiente) |
| Cambios en `covalos-agents` | Solo extensiones compatibles (ver abajo) |
| Archivos de la app modificados | Ninguno: `src/`, `Cargo.toml` raíz, UI, temas, fondos y branding quedan intactos |

Documentación técnica: `crates/covalos-engine/README.md`.

## Qué quedó hecho y verificado en Linux

- `covalos-agents`: 40 pruebas.
- `covalos-engine`: 17 pruebas unitarias y 18 de integración. La suite se repitió 9 veces sin fallos intermitentes.
- `cargo fmt` y `cargo clippy --all-targets -- -D warnings` limpios en ambos crates.
- Planificación:
  - Planificador determinista para objetivos en español o inglés.
  - Validación de planes JSON (por ejemplo, la salida de un Architect): ciclos, dependencias faltantes y roles inexistentes.
- Grafo de dependencias con ejecución paralela acotada (`max_parallel`). Verificado: frontend corre en paralelo con base de datos y backend.
- Asignación de agentes por capacidades declaradas, probe real del binario, estado de la cuenta, modelos no marcados como no disponibles, carga y preferencias (orden de proveedores, roles fijados a una cuenta, exclusiones).
- Estados por tarea, reintentos con backoff exponencial y omisión de las dependientes cuando una tarea falla.
- Presupuestos de tiempo, tokens y costo, por tarea y por plan. El costo solo cuenta cuando el proveedor lo reporta: Claude informa `total_cost_usd`.
- Cancelación segura, pausa y reanudación.
- Recuperación tras un fallo: el journal se reproduce, los intentos en curso pasan a interrumpidos y se reencolan, y una firma pendiente se vuelve a pedir sin repetir el trabajo.
- Registro persistente (JSON Lines con sync por registro). Un registro truncado se corta al reabrir; este error real se detectó y corrigió durante las pruebas.
- Ocho roles predefinidos (Architect, Frontend, Backend, Rust, UI Designer, Reviewer, QA, Release) más roles personalizados validados. Cada uno tiene responsabilidades, capacidades, comandos preaprobados, autonomía, criterios de finalización verificados por el motor, presupuesto e intentos.
- Memoria compartida estructurada:
  - Tipos: contratos, resultados de pruebas, archivos modificados, tareas completadas, bloqueos, revisiones, etc.
  - Referencias verificables (archivo más digest, commit, tarea).
  - Historial de revisiones con `supersedes`.
  - Rechazo de textos que parecen secretos.
  - Sin transcripts.
- Seguridad:
  - Clasificador de comandos (Safe / Sensitive / Forbidden) sobre el sistema de aprobaciones existente.
  - Los comandos sensibles se preguntan siempre.
  - Las aprobaciones vencidas cuentan como denegadas.
  - Entorno depurado para procesos hijos, kill del grupo de procesos y límites de tiempo y salida.
  - Los worktrees con cambios nunca se eliminan.
- `CliExecutor` probado contra un binario falso que emite el `stream-json` de Claude: prompt por stdin, flags, uso, costo, entorno depurado, errores y cancelación.

## Extensiones compatibles en `covalos-agents`

- `RoleId` y `PlanId`.
- `Policy::grant` pasa a ser público.
- `ApprovalQueue::cancel_ticket`.
- `AgentEvent::Cost`.
- `LaunchPlan::isolated_command`.
- `SharedContext::publish` ahora recibe un `Draft` y devuelve `Result`. Ningún código fuera de los crates usaba la firma anterior.

## Qué no se verificó (pendientes reales)

- Ninguna sesión real con Claude, Codex o Gemini: no hay credenciales en este entorno. Solo se ejecutó `claude --version`. Codex y Gemini no están instalados.
- Gemini no tiene modo sin interfaz en el motor, así que `CliExecutor` lo rechaza explícitamente.
- Las aprobaciones de herramientas dentro de un turno real del proveedor no pasan por el motor. Ahí rige el sandbox del CLI (`--permission-mode` / `--sandbox`). Conectar los hooks de `src/agent_bridge.rs` a `Channel::request` queda para la integración.
- No hay límites de CPU, memoria ni red a nivel de sistema operativo. Los límites efectivos son entorno, tiempo, salida y grupo de procesos.
- La integración del trabajo de cada worktree a la rama principal (commit y merge) es manual. El motor no hace commits.
- Si se edita un rol entre un fallo y la recuperación, se usa la versión nueva del rol. Los presupuestos e intentos ya resueltos quedan en el journal.
- Nada se probó en Windows.

## Integración al volver a la PC

1. Mergear primero `feat/cloud-agents-foundation` (PR #1) y luego `feat/autonomous-engine`. Esta rama ya contiene la anterior.
2. Verificar en Windows:
   ```powershell
   cd crates\covalos-agents; cargo test; cargo clippy --all-targets -- -D warnings
   cd ..\covalos-engine; cargo test; cargo clippy --all-targets -- -D warnings
   ```
   Las pruebas de la CLI con binario falso son solo para Unix y se omiten en Windows.
3. En el `Cargo.toml` raíz: `[workspace] members = [".", "crates/covalos-agents", "crates/covalos-engine"]` y dependencia `covalos-engine = { path = "crates/covalos-engine" }`.
4. En la app:
   - Crear el `Engine` con `Workspace::Worktrees` sobre el proyecto abierto y llamar a `tick()` desde el loop de egui (no bloquea).
   - Mostrar `pending_approvals()` en el Agent Hub y llamar a `resolve`.
   - Mostrar `plan().tasks()` y `context()`.
5. Implementar un `CredentialStore` real (Windows Credential Manager) y probar una sesión real con `CliExecutor` antes de habilitar la autonomía.
