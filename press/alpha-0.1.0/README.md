# Covalos — Native Rust IDE with Multi-Agent AI · Alpha 0.1.0

Material para la presentación pública. **Es una versión Alpha**: usable para trabajo real, con limitaciones conocidas (abajo). Todas las capturas son de la aplicación real ejecutándose el 2026-11-20 en Windows 11, sobre un proyecto Vite + React + TypeScript (`npm create vite@latest -- --template react-ts`). Nada está simulado ni retocado.

| Archivo | Qué muestra (verificado) |
| --- | --- |
| `01-welcome.png` | Pantalla de bienvenida (Nocturne, Dither Field). |
| `02-editor-react.png` | Editor con el proyecto React abierto: Explorer, pestañas, resaltado TSX. |
| `03-lsp-error-and-problems.png` | Error de tipos real de TypeScript 7 (`tsc --lsp`): subrayado bajo la variable y la pestaña Problems con línea y columna. |
| `04-lsp-completion.png` | Autocompletado del servidor de TypeScript: `useSt` → `useState`. |
| `05-agent-beside-code.png` | Chat de agente en el panel lateral junto al código: una sesión real de Codex (cuenta ChatGPT por el CLI oficial, modo solo lectura) explicando `App.tsx`. |
| `06-terminal-and-preview.png` | Terminal interactiva (PowerShell en ConPTY) ejecutando `npm run dev` y la vista previa WebView2 del sitio en `localhost:5173`. Captura de la ventana de Covalos únicamente (incluye la vista web nativa). |
| `07-orchestrator.png` | Página del orquestador: planificador, asignación de roles a cuentas y presupuestos. La captura no muestra un plan en ejecución. |

## Qué se puede afirmar

- Editor nativo en Rust (egui/eframe), sin Electron.
- Diagnósticos, autocompletado, hover y formato con el servidor LSP estándar de TypeScript (verificado con TypeScript 7); diagnósticos de CSS verificados. HTML y JSON usan los servidores estándar, sin verificación propia todavía.
- Terminal interactiva real con pestañas, Ctrl+C y colores.
- Servidor de desarrollo, detección de puertos, vista previa integrada (WebView2) y enlace temporal de Cloudflare con confirmación previa.
- Agentes: sesiones reales de Codex verificadas (incluida la reanudación tras cancelar). Claude Code se conecta por su CLI oficial; en este equipo no se verificó un turno real porque la sesión del CLI estaba vencida.
- Orquestador conectado al motor `covalos-engine` (ya se ejecutó un objetivo real con Codex en una prueba anterior).

## Qué no afirmar todavía

- Gemini: solo detección e instalación guiada; no hay chat verificado.
- Antigravity y otros editores: no se integran (no tienen una interfaz oficial para hacerlo).
- Builds de Linux y macOS: preparados en el código, no compilados ni probados para esta versión.
- Sin instalador firmado ni actualizaciones automáticas en esta Alpha.

## Texto sugerido (borrador, revisar antes de publicar)

> Presento **Covalos Alpha 0.1.0**, un IDE nativo escrito en Rust con agentes de IA integrados. Esta Alpha ya permite abrir un proyecto React, editar con diagnósticos y autocompletado de TypeScript, correr `npm run dev` en una terminal real, ver el sitio en una vista previa integrada y trabajar con un agente (Codex) en un panel junto al código. Es una versión temprana: hay limitaciones conocidas y la estoy usando para construir el propio sitio de Covalos.
