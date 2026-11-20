# COVALOS — Checkpoint de integración V4/V5 (2026-11-20)

Leé este archivo primero. Todo lo marcado como **verificado** se ejecutó de verdad en Windows con el CLI real de Codex. Lo marcado como **solo compila** o **no verificado** no tiene evidencia de ejecución.

## 1. Estado del repositorio

| Dato | Valor |
| --- | --- |
| Rama | `integration/v4-v5` (subida a `origin`) |
| Worktree activo | `E:\workspace\covalos-integration` |
| Base | `fix/minor-fixes` → merges `feat/windows-installer` → `origin/feat/cloud-agents-foundation` → `origin/feat/autonomous-engine` |
| Rama del instalador | `feat/windows-installer` (worktree `E:\workspace\covalos-release`, commit `46fbb72`). Los logs crudos del instalador (`artifacts/windows-installer/*.log`) quedaron sin commitear a propósito, solo en ese worktree. |
| `main` | Sin tocar. No se hizo merge a `main`. |
| Identidad Git | `DevModee`, sin líneas `Co-Authored-By`. |
| Ejecutable Release | `E:\workspace\covalos-integration-build\release\covalos.exe` (COVALOS 0.4.0.0) |
| Target de debug/tests | `E:\workspace\covalos-build\integration` |

Commits de esta integración (en orden): `5e99370` merge del instalador, `d61c4a5` merge de agents, `8236ad6` merge del engine, `0dd76ce` fix del journal en Windows, `93a2941` Agent Hub y engine conectados al editor, `521882a` endurecimiento del runtime y legibilidad. Después siguen el commit de evidencia y este checkpoint.

## 2. Qué funciona de verdad (verificado)

| Función | Evidencia |
| --- | --- |
| Detección de Claude Code 2.1.295, Codex CLI 0.167 (el que trae la app de Codex en `%LOCALAPPDATA%\OpenAI\Codex\bin\<hash>`), Gemini (no instalado) y Antigravity (solo launcher) | `artifacts/agent-hub/agents/01-providers-detected-*.png` |
| Varias cuentas por proveedor: sistema, login oficial aislado (`CLAUDE_CONFIG_DIR` / `CODEX_HOME`) y API key | `02-accounts-multi`, `replay/01-accounts` |
| Estado de sign-in leído con el comando oficial de cada CLI (`claude auth status --text`, `codex login status`), sin e-mails | reporte `accounts[]` |
| Windows Credential Manager (`CredWriteW`/`CredReadW`/`CredDeleteW`) | test `credentials_windows::round_trip_through_credential_manager` contra el almacén real |
| **Meta 1:** sesión real de Codex iniciada desde COVALOS, eventos en vivo (started, agent, tool, usage, done) e ID de sesión del proveedor persistido | `agents/03..05`, `replay/02..03` |
| **Meta 2:** dos agentes en paralelo, en worktrees distintos y con tareas distintas. A modificó solo `calc.py` (agregó `mul`); B creó solo `test_calc.py`; el checkout principal quedó limpio | `git status` sobre `E:\workspace\covalos-agent-run-3` |
| Contexto compartido: A y B publican sus resultados; C responde usando solo ese contexto | `agents/06-shared-context` (run 1), `replay` |
| Cancelación: el turno se corta y el árbol de procesos termina (`taskkill /T` + Job Object kill-on-close). No quedaron procesos huérfanos: los dos `codex.exe` vivos pertenecen a la app de Codex del usuario | `agents/07-cancelled`, `replay/05` |
| Recuperación de sesiones: si COVALOS se cae a mitad de turno, al reabrir la sesión aparece como **Interrupted** con su ID de proveedor | `replay/04-session-c-recovered` |
| Reanudación (`codex exec … resume <id>`) con el ID persistido: respondió `RESUMED` (ver bug 1) | prueba manual en la sesión `01a1f9db-…` |
| **Meta 3:** objetivo autónomo con el engine (rol Reviewer, worktree propio). El engine excluyó las 4 cuentas desconectadas, asignó `codex-main`, el agente publicó una entrada `review` y el criterio se cumplió. Plan `succeeded` en 1,7 min, ≈210k tokens | `artifacts/agent-hub/engine/*.png`, `reports/agent-hub-report.json`, `config-snapshot/plans/*.jsonl` |
| PreToolUse gate (`covalos.exe --agent-gate`): permite escrituras dentro del workdir y comandos seguros; niega escrituras afuera, comandos destructivos y comandos sensibles si no hay nadie para aprobar. Deja un log de auditoría redactado | protocolo real con el `covalos.exe` Release (ver §5) y 5 tests unitarios |
| Rediseño de legibilidad del Agent Hub, revisado en capturas reales de 1366×768 y 1100×700 | `artifacts/agent-hub/replay/` |
| Release: `covalos.exe` arranca, la ventana se llama `COVALOS`, responde y usa ≈106 MB | lanzado con PowerShell (ver §6) |

## 3. Parcial, solo compila o no verificado

- **Claude Code sin turnos reales.** El login del CLI del sistema está vencido ("Login: Expired") y las cuentas aisladas no tienen login. El gate nunca se ejercitó dentro de un turno real de Claude: solo se probó su protocolo invocando el ejecutable directamente.
- **Gemini CLI** no está instalado. COVALOS no tiene adaptador headless verificado para Gemini, así que solo hay detección y cuentas con API key.
- **Antigravity:** solo se detecta el launcher. No hay interfaz headless documentada ni se reutiliza su sesión.
- **Cuotas:** solo se muestran si el proveedor las informa en su stream (`rate_limit_event` / `rate_limits`). En estas corridas Codex no las informó.
- **Codex con API key** (`codex login --with-api-key` dentro de un `CODEX_HOME` aislado): implementado, pero sin ejecutar porque no se usaron claves.
- **Aprobaciones en la UI** (Allow once / Allow for session / Deny): implementadas y probadas con tests de archivos, pero ningún turno real llegó a pedir aprobación (Codex no usa el gate).
- **Zoom del 125 %:** el harness cambia `pixels_per_point` en cada frame y egui descarta todos los frames, así que la captura queda en blanco. Es un problema del harness, no está verificado en la app. El zoom propio de COVALOS (preferencias) no se probó con el Agent Hub.
- **No se tomaron capturas del exe Release en sí:** el usuario denegó el control de pantalla. Las capturas salen del harness, que ejecuta el mismo código de UI en perfil Release.

## 4. Bugs conocidos

1. **Reanudar justo después de matar un turno se cuelga.** Si COVALOS mata un turno de Codex en curso y enseguida reanuda el mismo hilo, Codex 0.167 no emite eventos durante más de 240 s. El mismo `resume` funciona cuando el lock del CLI se libera (comprobado a mano: respondió `RESUMED`). Pendiente: agregar un timeout de primer evento con reintento, o esperar a que el proceso anterior termine antes de reanudar. Evidencia: `artifacts/agent-hub-run2` y la corrida 3.
2. Codex informa `input_tokens` sumando la caché (≈400–860k por turno chico). COVALOS los muestra tal cual y no los convierte en costo.
3. La cuenta Codex aislada creada bajo `%TEMP%` imprime un `WARNING` de PATH aliases. Ya se filtra del estado; no aparece con rutas normales.

Bugs encontrados y corregidos con las corridas reales: truncado del journal en Windows; cuentas desconectadas asignadas por el engine (401); falso "no instalado" al reabrir; procesos huérfanos tras una caída (ahora Job Object); rutas `\\?\`; roles con criterio `Published` imposibles de completar (ahora líneas `COVALOS-CONTEXT`); overlaps y desbordes del layout.

## 5. Permisos: qué controla cada capa

- **Gate de COVALOS (solo Claude Code):** se instala un hook PreToolUse por sesión que aplica:
  - Policy por preset; escrituras fuera del workdir siempre denegadas.
  - Clasificador de comandos: Forbidden se niega; Sensitive pide aprobación siempre.
  - Rutas con pinta de credenciales: requieren aprobación.
  - Si la UI no responde (heartbeat), se niega.
  - Auditoría en `<config>/agents/gate/audit.jsonl`.
- **Codex NO pasa por el gate.** Su límite efectivo es su propio `--sandbox` (`read-only` / `workspace-write`). La UI lo dice así para cada sesión de Codex.
- **El proceso sigue siendo del usuario de Windows.** Un worktree aísla archivos para revisarlos; no aísla el proceso. No hay límites de CPU, memoria ni red.
- **Entorno de los hijos:** solo pasan las variables de la allowlist (`INHERITED_ENV`). Las variables `CLAUDE_*` / `ANTHROPIC_*` / `OPENAI_*` del proceso anfitrión no se heredan (el shell de Claude desktop sí las tiene).
- **Secretos:**
  - API keys: en Credential Manager, pasadas solo por la variable documentada de cada proveedor.
  - Tokens de login: en la carpeta propia de cada CLI. COVALOS no los lee ni los copia.
  - Texto con pinta de secreto: se redacta en logs, auditoría y contexto compartido.
- Prueba del gate contra el Release, todas con el resultado esperado:

  | Llamada | Resultado |
  | --- | --- |
  | `Write` dentro del workdir | allow |
  | `Write` en `C:/Windows` | deny |
  | `git status` | allow |
  | `rm -rf /` | deny (Destructive) |
  | `git push` sin UI abierta | deny |

## 6. Cómo reproducir

```powershell
cd E:\workspace\covalos-integration
cargo fmt --all --check
cargo clippy --workspace --all-targets --target-dir E:\workspace\covalos-build\integration -- -D warnings
cargo test --workspace --target-dir E:\workspace\covalos-build\integration   # 56 + 40 + 17 + 16
cargo build --release --target-dir E:\workspace\covalos-integration-build

# Repetir capturas sin gastar cuota (estado grabado de la corrida 3):
$env:COVALOS_EVIDENCE_DIR = "artifacts/agent-hub"; $env:COVALOS_AGENT_RUN = "E:\workspace\covalos-agent-run-3"
cargo test --release --target-dir E:\workspace\covalos-integration-build capture_agent_hub_replay -- --ignored
# Un objetivo real con el engine (1 turno de Codex): agregar $env:COVALOS_AGENT_ENGINE = "1"
# Escenario completo con proveedores reales (~8 turnos): capture_agent_hub_review con COVALOS_AGENT_SANDBOX y COVALOS_AGENT_RUN apuntando a una carpeta nueva.
```

- Proyecto descartable para pruebas: `E:\workspace\covalos-agent-sandbox`. Copias de cada corrida: `E:\workspace\covalos-agent-run-2` y `E:\workspace\covalos-agent-run-3` (proyecto, worktrees y config).
- Evidencia:
  - `artifacts/agent-hub/`: corrida 3, `replay/`, `engine/`, `reports/agent-hub-report.json`, `config-snapshot/`.
  - `artifacts/agent-hub-run1/` y `artifacts/agent-hub-run2/`: corridas con los bugs encontrados.
- Resultado del test de tokens: no se encontraron secretos en la evidencia. Los `CODEX_HOME` capturados se ignoran en `.gitignore`.

## 7. Ajuste final de bienvenida (verificado)

- **Explorador contextual:** sin proyecto abierto, el panel lateral se oculta y la bienvenida queda centrada en el área de trabajo. La barra de actividad sigue visible. Al abrir una carpeta, el Explorador vuelve solo. Sin proyecto, la barra de actividad lo abre y cierra a pedido (`sidebar_requested`). No había ni hay una preferencia para cerrar el Explorador a mano, así que no hay nada más que respetar. Tampoco existe una acción de "cerrar proyecto", por lo que ese caso no se pudo probar. No cambian el layout del asistente ni el zoom.
- **Marca visible:** `Covalos` en la bienvenida, el título de la ventana, About, los temas (`Covalos Nocturne`…), Discord, Library Studio, Agent Hub y el gate. Sin cambios en identificadores técnicos: paquete `covalos`, variables `COVALOS_*`, user agent `COVALOS-Library-Studio`, salida CLI de `library.rs`, rutas y metadatos PE. Los temas se guardan por variante serde, así que las preferencias siguen siendo compatibles.
- **Accesos rápidos:** sin cambios de diseño (iconos Lucide, hover existente).
  - Abrir carpeta y Abrir archivo muestran sus atajos (Ctrl O / Ctrl Shift O).
  - Crear proyecto y Agentes no tienen atajo en COVALOS. No se inventó uno, por la regla de no agregar funciones.
  - Probado con el harness: un clic real en Agentes abre el Agent Hub. Abrir carpeta, Abrir archivo y Crear proyecto abren diálogos nativos, que el harness no puede manejar.
- **Evidencia:** `artifacts/welcome-review/welcome/` (sin proyecto, Agentes desde la bienvenida, Explorador a pedido, proyecto abierto). El exe Release reconstruido abre con el título `Covalos` y responde.
- Una instancia de COVALOS (PID 14824) ya estaba corriendo desde esta carpeta de build y no se cerró.

## 8. Próximas tres prioridades

1. **Arreglar el bug 1** (reanudar tras un kill). Antes de reanudar, esperar a que salga el proceso anterior y aplicar un timeout de primer evento con un reintento. Verificarlo con un solo turno.
2. **Verificar Claude Code real:** el usuario tiene que correr `claude auth login` (o "Sign in (official)" en Accounts). Después, una sesión con preset Supervised que pida un comando sensible, para validar el gate y las aprobaciones de punta a punta.
3. **Instalar el Gemini CLI oficial** y escribir y probar su decodificador headless. Recién entonces habilitar sesiones de Gemini. No reutilizar sesiones de Antigravity.
