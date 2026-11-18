# AEROS Agent Companion

## Qué funciona

Luma y Live agents muestran las solicitudes internas de Workspace, General y revisión independiente. Los estados proceden de Run y sus eventos, con propuestas de Assist marcadas como pendientes. No se crea una conexión por detectar un ejecutable instalado. Open session enfoca la conversación interna.

La vista compacta junto al proyecto muestra solicitudes activas y aprobaciones. Hay notificaciones internas y solicitud de atención de Windows mediante la barra de tareas, con modo silencioso e intervalo configurable. No se han añadido notificaciones Toast.

## Hooks externos

El propio ejecutable implementa el modo --agent-hook. Lee un objeto JSON desde stdin, limitado a 64 KiB, y publica un evento en la carpeta de configuración de AEROS. El IDE supervisa hasta 64 sesiones, sin leer credenciales ni modificar ajustes de proveedores.

Campos compatibles: hook_event_name, session_id, coucou_agent, cwd, tool_name y tool_input. Se admiten SessionStart, UserPromptSubmit, PreToolUse, PostToolUse, PostToolUseFailure, SubagentStart, SubagentStop, PermissionRequest, Notification, Stop, StopFailure y SessionEnd. Las sesiones externas muestran el último estado recibido, no una prueba de que su proceso siga vivo. La ausencia de eventos no se interpreta como una desconexión.

Para conectar Claude Code, se puede añadir un hook de tipo command a su configuración existente:

    E:\workspace\aeros-quality-build\release\aeros-editor.exe --agent-hook

Se puede registrar ese comando para los eventos anteriores. PermissionRequest necesita un timeout de al menos 95 segundos. El esquema y los nombres de eventos se verificaron en https://code.claude.com/docs/en/hooks. Esta entrega no instala hooks automáticamente. Si se usa AEROS_CONFIG_DIR, el IDE y el hook deben compartir ese valor.

## Decisiones

Las aprobaciones ejecutables solo se ofrecen para PermissionRequest de Claude con un ticket vigente y una operación completa de hasta 8 KiB. Se muestran operación, agente, archivo y alcance. Allow once y Deny escriben una decisión vinculada al ticket; el hook devuelve el objeto oficial hookSpecificOutput con el comportamiento elegido. No hay Always allow ni una decisión implícita.

El registro decisions.jsonl conserva agente, sesión, operación, alcance, decisión y hora. Una decisión duplicada, cambiada o vencida falla. Sin una UI activa, al cerrarla o al vencer el plazo, el hook no concede permisos y deja el flujo en su terminal. Otros proveedores y notificaciones de permiso sin soporte muestran Answer in terminal.

Las propuestas internas de Assist conservan su revisión existente, comprobación del buffer y guardado explícito. El companion enlaza al cambio propuesto.

## Validación

La prueba autenticada Assist de Codex generó un reemplazo real, esperó aprobación y dejó el archivo en disco intacto hasta Save. Las pruebas del puente cubren decisiones duplicadas, tickets cambiados, proveedores sin soporte y solicitudes vencidas.

La captura companion.png usa un hook real del ejecutable con un payload de contrato identificado como INTEGRATION TEST. El test lo rechaza después de capturarlo. Ese payload no procede de Claude ni ejecuta una herramienta. La conversación que aparece detrás es un fixture visual. No debe presentarse esa captura como una sesión de IA real.

## Pendiente

Validación con una sesión interactiva autenticada de Claude y hooks instalados; adaptadores de aprobación para otros proveedores; activación de una ventana exacta de terminal externa; detección de sesiones sin hooks; historial de conversaciones persistente; integración del navegador Obscura.
