# COVALOS — Checkpoint Cloud: base del Agent Hub V4 (2026-11-20)

## Estado

| Dato | Valor |
| --- | --- |
| Rama | `feat/cloud-agents-foundation`, creada desde `main` (`733627a`) |
| Entorno | Sesión de Claude en la nube (Linux), sin acceso a `E:\` |
| Identidad Git | `DevModee <caelien2.0@gmail.com>` |
| Código nuevo | `crates/covalos-agents/` (crate independiente, con su propio `Cargo.lock`) |
| Archivos existentes modificados | Ninguno. No se tocaron `src/`, `Cargo.toml`, `Cargo.lock`, temas, fondos, zoom, paneles, branding ni instalador. |

La documentación técnica está en `crates/covalos-agents/README.md`.

## Qué quedó hecho y verificado en Linux

- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` y `cargo test`: 36 pruebas, todas satisfactorias.
- Proveedores: Claude Code, Codex, Gemini CLI y proveedores personalizados validados.
- Varias cuentas por proveedor:
  - Sistema (CLI ya configurada por el usuario).
  - Login oficial aislado: `CLAUDE_CONFIG_DIR` / `CODEX_HOME`.
  - API key: `ANTHROPIC_API_KEY` / `GEMINI_API_KEY`.
- Credenciales: `Secret` con salida redactada, `CredentialStore` como trait y perfiles sin secretos (comprobado por prueba).
- Sesiones independientes con máquina de estados, eventos normalizados y decodificadores de Claude `stream-json` y Codex `exec --json`.
- Permisos:
  - Presets `ReadOnly`, `Supervised` y `Worktree`.
  - Cola de aprobaciones con vencimiento.
  - Bloqueo de escrituras fuera del worktree, incluidos `..`, rutas absolutas y symlinks.
- Contexto compartido acotado entre agentes.
- Worktrees por sesión (`covalos/agent/<sesión>`), probados contra un repositorio git real.
- Catálogo de modelos por cuenta (declarados u observados) y cuotas solo de origen oficial.
- `TaskGraph` con dependencias, como base para orquestación autónoma.
- `claude --version` ejecutado de verdad (2.1.329) mediante la ruta de probe.

## Qué no se verificó (pendientes reales)

- Ningún login ni turno real contra Claude, Codex o Gemini: no hay cuentas configuradas en este entorno. Codex y Gemini no están instalados.
- Codex con API key: no se codificó variable porque no se pudo confirmar la oficial. Por ahora usar `OfficialLogin` (`codex login` bajo `CODEX_HOME`).
- Gemini:
  - Sin aislamiento de login: no se confirmó variable de directorio de configuración.
  - Sin decodificador headless.
  - Funciona con `SystemCli` y `ApiKey`.
- Cuotas: no hay parser. Falta identificar un campo oficial por proveedor antes de llenar `QuotaSnapshot`.
- Almacén de credenciales del sistema operativo (Windows Credential Manager, Keychain, Secret Service): solo existe el trait y la versión en memoria.
- `WorktreeManager` no se probó en Windows.
- Integración con la UI: deliberadamente no hecha.

## Integración al volver a la PC

1. Primero cerrar y subir `fix/minor-fixes` y `feat/covalos-rebrand` como estén planeadas.
2. Traer esta rama:
   ```powershell
   cd E:\workspace\covalos-minor-fixes
   git fetch origin feat/cloud-agents-foundation
   git merge --no-ff origin/feat/cloud-agents-foundation
   ```
   No debería haber conflictos: la rama solo agrega `crates/covalos-agents/`, `docs/CHECKPOINT_CLOUD_AGENTS.md` y `.gitignore` del crate.
3. Verificar el crate en Windows:
   ```powershell
   cd crates\covalos-agents
   cargo fmt --check
   cargo clippy --all-targets -- -D warnings
   cargo test
   ```
4. Conectarlo a la app, en un commit propio:
   - En el `Cargo.toml` raíz agregar `covalos-agents = { path = "crates/covalos-agents" }`. Opcionalmente, declarar `[workspace] members = [".", "crates/covalos-agents"]` y borrar `crates/covalos-agents/Cargo.lock` para usar un único lock.
   - Reemplazar gradualmente `src/assistant/providers.rs` por `covalos_agents::launch::plan` y el parser `Responses` de `src/assistant.rs` por `ClaudeDecoder` / `CodexDecoder`.
   - Mapear los estados de `src/agent_bridge.rs` (hooks) a `AgentEvent::ApprovalRequested` / `ApprovalResolved`.
   - Usar como directorio de perfiles `paths` de COVALOS (por ejemplo `<data>/agents`) para `ProfileStore::open`.
5. Implementar `CredentialStore` sobre Windows Credential Manager. `windows-sys` ya es dependencia; agregar la feature `Win32_Security_Credentials`.
6. Luego, la UI del Agent Hub multiproveedor sobre estas APIs.
