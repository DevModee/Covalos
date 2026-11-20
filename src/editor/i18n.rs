//! Interface localization. English source strings are the keys; Spanish (neutral) is the
//! first translation. Unknown keys fall back to English, so agent output, file names,
//! commands and product names are never altered.
use serde::{Deserialize, Serialize};
use std::cell::Cell;
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Language {
    #[default]
    English,
    Spanish,
}

impl Language {
    pub(super) const ALL: [Self; 2] = [Self::English, Self::Spanish];
    pub(super) fn native_name(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::Spanish => "Español",
        }
    }
    /// Spanish when the Windows UI language is Spanish, English otherwise.
    pub(super) fn system() -> Self {
        #[cfg(windows)]
        {
            // LANGID: the low 10 bits are the primary language; 0x0A is Spanish.
            let id = unsafe { windows_sys::Win32::Globalization::GetUserDefaultUILanguage() };
            if id & 0x3ff == 0x0a {
                return Self::Spanish;
            }
        }
        #[cfg(not(windows))]
        if std::env::var("LANG").is_ok_and(|l| l.starts_with("es")) {
            return Self::Spanish;
        }
        Self::English
    }
}

thread_local! {
    static ACTIVE: Cell<Language> = const { Cell::new(Language::English) };
}

pub(super) fn set(language: Language) {
    ACTIVE.with(|a| a.set(language));
}

pub(super) fn active() -> Language {
    ACTIVE.with(Cell::get)
}

fn spanish() -> &'static HashMap<&'static str, &'static str> {
    static TABLE: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    TABLE.get_or_init(|| SPANISH.iter().copied().collect())
}

/// Translate an interface string for the active language.
pub(super) fn tr(text: &str) -> &str {
    if active() == Language::Spanish
        && let Some(es) = spanish().get(text)
    {
        return es;
    }
    text
}

/// Translate a template and substitute `{key}` placeholders.
pub(super) fn trf(template: &'static str, values: &[(&str, String)]) -> String {
    let mut out = tr(template).to_owned();
    for (key, value) in values {
        out = out.replace(&format!("{{{key}}}"), value);
    }
    out
}

const SPANISH: &[(&str, &str)] = &[
    ("Close diff", "Cerrar diferencias"),
    ("Copy path", "Copiar ruta"),
    ("No repository", "Sin repositorio"),
    ("Show diff", "Ver diferencias"),
    ("Tracking", "Siguiendo"),
    ("Working tree clean", "Árbol de trabajo limpio"),
    ("Merge conflicts", "Conflictos de fusión"),
    ("Staged changes", "Cambios preparados"),
    ("Untracked files", "Archivos sin seguimiento"),
    ("Refresh changes", "Actualizar cambios"),
    ("Back to changes", "Volver a los cambios"),
    (
        "This folder is not a Git repository.",
        "Esta carpeta no es un repositorio Git.",
    ),
    ("No differences.", "Sin diferencias."),
    ("Working tree", "Árbol de trabajo"),
    ("Follow theme", "Seguir el tema"),
    ("Code colors", "Colores del código"),
    ("Editor density", "Densidad del editor"),
    ("Compact", "Compacta"),
    ("Comfortable", "Cómoda"),
    ("Detach to floating editor", "Desprender a editor flotante"),
    ("Dock back into tabs", "Acoplar de nuevo en pestañas"),
    ("Floating editors", "Editores flotantes"),
    ("Dock all", "Acoplar todos"),
    ("Focus mode", "Modo enfoque"),
    ("Changes", "Cambios"),
    // Chrome, navigation and Home
    ("File", "Archivo"),
    ("Search files and commands", "Buscar archivos y comandos"),
    ("Quick access", "Acceso rápido"),
    ("Close window", "Cerrar ventana"),
    ("Maximize or restore", "Maximizar o restaurar"),
    ("Minimize", "Minimizar"),
    ("Toggle Codex panel", "Mostrar u ocultar el panel de Codex"),
    ("Explorer", "Explorador"),
    ("Search", "Buscar"),
    ("Git", "Git"),
    ("Source control", "Control de código"),
    ("Settings", "Configuración"),
    ("New file", "Nuevo archivo"),
    ("Refresh files", "Actualizar archivos"),
    ("Open folder", "Abrir carpeta"),
    ("Open file", "Abrir archivo"),
    ("Create project", "Crear proyecto"),
    ("New folder", "Nueva carpeta"),
    ("Open folder…", "Abrir carpeta…"),
    ("Open file…", "Abrir archivo…"),
    ("New file…", "Nuevo archivo…"),
    ("Create project…", "Crear proyecto…"),
    ("Save", "Guardar"),
    ("Save all", "Guardar todo"),
    ("Close tab", "Cerrar pestaña"),
    ("Saved all files", "Todos los archivos se guardaron"),
    (
        "Find a file or explore your workspace.",
        "Busca un archivo o explora tu espacio de trabajo.",
    ),
    (
        "Open a project or continue your work.",
        "Abre un proyecto o retoma tu trabajo.",
    ),
    ("Recent projects", "Proyectos recientes"),
    ("Welcome to AEROS", "Te damos la bienvenida a AEROS"),
    ("Terminal", "Terminal"),
    ("Status", "Estado"),
    ("Loading…", "Cargando…"),
    (
        "Open a folder to browse files.",
        "Abre una carpeta para explorar sus archivos.",
    ),
    (
        "Directory scan was interrupted.",
        "La lectura de la carpeta se interrumpió.",
    ),
    // Palette commands
    ("Search workspace", "Buscar en el espacio de trabajo"),
    ("Find in active file", "Buscar en el archivo activo"),
    ("Save active file", "Guardar archivo activo"),
    ("Toggle terminal", "Mostrar u ocultar la terminal"),
    ("Toggle Agent Hub", "Mostrar u ocultar Agent Hub"),
    ("Open settings", "Abrir configuración"),
    (
        "Ask Codex in this workspace",
        "Preguntar a Codex en este espacio de trabajo",
    ),
    (
        "Find a file, > command, @ project, ? Codex",
        "Buscar archivo, > comando, @ proyecto, ? Codex",
    ),
    ("Enter a command…", "Escribe un comando…"),
    ("Enter  Open", "Enter  Abrir"),
    ("Esc", "Esc"),
    ("No matching items.", "No hay coincidencias."),
    // Find and search
    ("Find in file", "Buscar en el archivo"),
    ("Find text in files", "Buscar texto en los archivos"),
    ("Close find", "Cerrar búsqueda"),
    ("Next match (F3)", "Siguiente coincidencia (F3)"),
    (
        "Previous match (Shift+F3)",
        "Coincidencia anterior (Mayús+F3)",
    ),
    ("No matches", "Sin coincidencias"),
    // Files and dialogs
    ("New file here…", "Nuevo archivo aquí…"),
    ("New folder here…", "Nueva carpeta aquí…"),
    ("Rename or move…", "Renombrar o mover…"),
    ("Rename or move", "Renombrar o mover"),
    ("Delete…", "Eliminar…"),
    ("Delete", "Eliminar"),
    ("Deleted from workspace", "Eliminado del espacio de trabajo"),
    (
        "Path relative to the workspace",
        "Ruta relativa al espacio de trabajo",
    ),
    ("Create", "Crear"),
    ("Cancel", "Cancelar"),
    ("Confirm", "Confirmar"),
    ("Discard", "Descartar"),
    ("Keep editing", "Seguir editando"),
    ("Save your changes?", "¿Guardar los cambios?"),
    (
        "This file has unsaved changes.",
        "Este archivo tiene cambios sin guardar.",
    ),
    (
        "One or more files have unsaved changes.",
        "Uno o más archivos tienen cambios sin guardar.",
    ),
    (
        "This permanently deletes a file or an empty folder.",
        "Esto elimina de forma permanente un archivo o una carpeta vacía.",
    ),
    (
        "Save or close the modified file before deleting it.",
        "Guarda o cierra el archivo modificado antes de eliminarlo.",
    ),
    (
        "Choose a directory to open as a workspace.",
        "Elige una carpeta para abrirla como espacio de trabajo.",
    ),
    (
        "Conversation could not be saved",
        "No se pudo guardar la conversación",
    ),
    (
        "Your latest chat changes are still in memory.",
        "Tus últimos cambios del chat siguen en memoria.",
    ),
    ("Retry save and close", "Reintentar guardar y cerrar"),
    ("Close without saving chat", "Cerrar sin guardar el chat"),
    ("UTF-8", "UTF-8"),
    // Settings
    ("Appearance", "Apariencia"),
    ("Theme", "Tema"),
    ("Language", "Idioma"),
    (
        "Tune the workspace for long editing sessions.",
        "Ajusta el espacio de trabajo para sesiones largas de edición.",
    ),
    ("Editor font size", "Tamaño de fuente del editor"),
    ("High contrast controls", "Controles de alto contraste"),
    ("Compact interface", "Interfaz compacta"),
    ("Reduce motion", "Reducir movimiento"),
    (
        "Quiet agent notifications",
        "Silenciar notificaciones de agentes",
    ),
    (
        "Taskbar attention interval",
        "Intervalo de aviso en la barra de tareas",
    ),
    ("Background & ASCII", "Fondo y ASCII"),
    ("Preset", "Estilo"),
    ("Animate backgrounds", "Animar fondos"),
    ("Pointer reactions", "Reacciones al puntero"),
    ("Also show behind code", "Mostrar también detrás del código"),
    (
        "Off by default. Keep the code area quiet.",
        "Desactivado por defecto. Mantiene tranquila el área de código.",
    ),
    ("Custom color", "Color personalizado"),
    ("Ambient FPS", "FPS en reposo"),
    (
        "Animation pauses without focus, when minimized, or with Reduce motion.",
        "La animación se pausa sin foco, al minimizar o con Reducir movimiento.",
    ),
    ("Speed", "Velocidad"),
    ("Density", "Densidad"),
    ("Intensity", "Intensidad"),
    ("Pattern", "Patrón"),
    ("Luma companion", "Luma, la compañera"),
    (
        "Show Luma in workspace",
        "Mostrar a Luma en el espacio de trabajo",
    ),
    ("Animate Luma", "Animar a Luma"),
    (
        "Follow pointer and open companion",
        "Seguir el puntero y abrir el companion",
    ),
    ("Motion intensity", "Intensidad del movimiento"),
    ("Size", "Tamaño"),
    (
        "Original celestial silk moth. Its state follows actual agent and search activity.",
        "Personaje original de AEROS. Su estado sigue la actividad real de agentes y búsquedas.",
    ),
    ("Agent editing", "Edición con agentes"),
    (
        "Control how proposed changes reach your editor.",
        "Controla cómo llegan al editor los cambios propuestos.",
    ),
    (
        "Allow Auto buffer edits",
        "Permitir ediciones automáticas del búfer",
    ),
    ("Off", "Desactivado"),
    ("Minimal", "Mínimo"),
    ("High Contrast", "Alto contraste"),
    // Agent Hub
    ("Agent Hub", "Agent Hub"),
    ("Conversations", "Conversaciones"),
    ("Live agents", "Agentes en vivo"),
    ("Tasks", "Tareas"),
    ("Project memory", "Memoria del proyecto"),
    ("Skills", "Skills"),
    ("Back to editor", "Volver al editor"),
    ("Workspace", "Espacio de trabajo"),
    ("General", "General"),
    ("AGENTS", "AGENTES"),
    ("Detect installed agents", "Detectar agentes instalados"),
    ("Ready", "Listo"),
    ("CLI not found", "CLI no encontrada"),
    (
        "Detected · adapter pending",
        "Detectado · adaptador pendiente",
    ),
    ("Adapter pending", "Adaptador pendiente"),
    ("New chat", "Nuevo chat"),
    ("Chat ↗", "Chatear ↗"),
    (
        "Start a fresh conversation",
        "Iniciar una conversación nueva",
    ),
    ("Workspace · read only", "Espacio de trabajo · solo lectura"),
    (
        "Open a workspace to begin",
        "Abre un espacio de trabajo para empezar",
    ),
    (
        "General · no workspace or active file",
        "General · sin espacio de trabajo ni archivo activo",
    ),
    (
        "Isolated conversation · workspace tools disabled",
        "Conversación aislada · herramientas del espacio de trabajo desactivadas",
    ),
    (
        "Restored · send to reconnect",
        "Restaurada · envía un mensaje para reconectar",
    ),
    ("Open a workspace", "Abre un espacio de trabajo"),
    (
        "Explore a file, work through an idea or plan your next change.",
        "Explora un archivo, desarrolla una idea o planifica tu próximo cambio.",
    ),
    (
        "Choose a project to give this conversation context.",
        "Elige un proyecto para darle contexto a esta conversación.",
    ),
    ("Explain the active file", "Explica el archivo activo"),
    ("Find risky code paths", "Busca rutas de código riesgosas"),
    ("Summarize this project", "Resume este proyecto"),
    ("Message {name}", "Escribe a {name}"),
    ("Message {name}…", "Escribe a {name}…"),
    ("Send · Enter", "Enviar · Enter"),
    ("Stop", "Detener"),
    ("Copy", "Copiar"),
    ("Copied", "Copiado"),
    ("Copy message", "Copiar mensaje"),
    ("Something went wrong", "Algo salió mal"),
    ("Active file", "Archivo activo"),
    (
        "Attach up to 24 KB from the current buffer, including unsaved edits.",
        "Adjunta hasta 24 KB del búfer actual, incluidos los cambios sin guardar.",
    ),
    ("Waiting for response", "Esperando respuesta"),
    (
        "Inspecting workspace",
        "Inspeccionando el espacio de trabajo",
    ),
    ("Stopping", "Deteniendo"),
    ("Request stopped.", "Solicitud detenida."),
    ("Awaiting approval", "Esperando aprobación"),
    (
        "Plan a change or review an edit before applying it.",
        "Planifica un cambio o revisa una edición antes de aplicarla.",
    ),
    (
        "Keep goals and decisions with this workspace.",
        "Guarda objetivos y decisiones junto a este espacio de trabajo.",
    ),
    (
        "Review reusable instructions and choose them for tasks.",
        "Revisa instrucciones reutilizables y elígelas para tus tareas.",
    ),
    (
        "Ask creates a plan and review. Assist proposes a change to your active file.",
        "Preguntar crea un plan y una revisión. Asistir propone un cambio en tu archivo activo.",
    ),
    ("Ask · plan and review", "Preguntar · plan y revisión"),
    ("Assist · propose edit", "Asistir · proponer edición"),
    ("Auto · edit buffer", "Auto · editar búfer"),
    (
        "Assist requires a workspace file under 24 KB.",
        "Asistir requiere un archivo del espacio de trabajo de menos de 24 KB.",
    ),
    (
        "Auto applies an eligible replacement to the active buffer. Use Save to write the file.",
        "Auto aplica un reemplazo elegible en el búfer activo. Usa Guardar para escribir el archivo.",
    ),
    (
        "Ask Claude to review the Codex plan",
        "Pedir a Claude que revise el plan de Codex",
    ),
    (
        "Sends the task text and generated plan to Claude in an isolated directory.",
        "Envía la tarea y el plan generado a Claude en una carpeta aislada.",
    ),
    ("Describe the task…", "Describe la tarea…"),
    ("Task", "Tarea"),
    ("Review", "Revisión"),
    ("Claude review", "Revisión de Claude"),
    ("Plan and review", "Plan y revisión"),
    ("Before", "Antes"),
    ("After", "Después"),
    ("Diff", "Diferencias"),
    ("Staged", "Preparado"),
    ("Apply to editor buffer", "Aplicar al búfer del editor"),
    ("Reject", "Rechazar"),
    (
        "The proposed edit was based on a different buffer. Request a fresh edit.",
        "La edición propuesta se basaba en otro búfer. Solicita una nueva edición.",
    ),
    (
        "Apply a proposed replacement to the editor buffer",
        "Aplicar un reemplazo propuesto al búfer del editor",
    ),
    ("Independent review", "Revisión independiente"),
    ("View Codex conversation", "Ver la conversación de Codex"),
    (
        "Install the CLI to start this conversation.",
        "Instala la CLI para iniciar esta conversación.",
    ),
    ("Project notes", "Notas del proyecto"),
    (
        "Goals, decisions, and references…",
        "Objetivos, decisiones y referencias…",
    ),
    ("Save project notes", "Guardar notas del proyecto"),
    (
        "Project notes saved locally",
        "Notas del proyecto guardadas localmente",
    ),
    (
        "Include saved notes in agent tasks",
        "Incluir las notas guardadas en las tareas de agentes",
    ),
    (
        "Choose whether these saved notes are included in an agent task.",
        "Elige si estas notas se incluyen en una tarea de agente.",
    ),
    (
        "Open a workspace to keep project notes.",
        "Abre un espacio de trabajo para guardar notas del proyecto.",
    ),
    (
        "Open a workspace to run a task.",
        "Abre un espacio de trabajo para ejecutar una tarea.",
    ),
    ("GitHub skill URL", "URL de la skill en GitHub"),
    ("Fetch preview", "Obtener vista previa"),
    ("Install disabled", "Instalar desactivada"),
    ("Enabled", "Activada"),
    ("Use in tasks", "Usar en tareas"),
    ("Deselect for tasks", "Quitar de las tareas"),
    (
        "Review GitHub instructions before enabling them for tasks.",
        "Revisa las instrucciones de GitHub antes de activarlas para tus tareas.",
    ),
    (
        "Use a public GitHub URL ending in /blob/<ref>/…/SKILL.md.",
        "Usa una URL pública de GitHub que termine en /blob/<ref>/…/SKILL.md.",
    ),
    ("Hide", "Ocultar"),
    // Companion and Luma
    ("Luma", "Luma"),
    ("Companion ↗", "Companion ↗"),
    ("LUMA  ·  COMPANION", "LUMA  ·  COMPANION"),
    ("Your decision is needed", "Se necesita tu decisión"),
    ("Work in motion", "Trabajo en curso"),
    ("A quiet workspace", "Un espacio tranquilo"),
    ("{n} running", "{n} en curso"),
    ("{n} waiting", "{n} en espera"),
    ("{n} done", "{n} listos"),
    ("{n} approval needed", "{n} aprobación pendiente"),
    ("{n} approvals needed", "{n} aprobaciones pendientes"),
    ("{n} agent working", "{n} agente trabajando"),
    ("{n} agents working", "{n} agentes trabajando"),
    ("All quiet", "Todo en calma"),
    ("Close companion", "Cerrar companion"),
    ("No agents running", "No hay agentes en ejecución"),
    (
        "Sessions from Agent Hub and connected tools appear here with live status, approvals and results.",
        "Las sesiones de Agent Hub y de las herramientas conectadas aparecen aquí con su estado en vivo, aprobaciones y resultados.",
    ),
    (
        "Review the operation above. Allowing applies once.",
        "Revisa la operación. El permiso se aplica una sola vez.",
    ),
    ("Allow once", "Permitir una vez"),
    ("Deny", "Denegar"),
    ("Allowed once", "Permitido una vez"),
    ("Denied", "Denegado"),
    (
        "Answer this request in its original terminal.",
        "Responde a esta solicitud en su terminal original.",
    ),
    ("Copy session ID", "Copiar ID de sesión"),
    ("Open workspace ↗", "Abrir espacio de trabajo ↗"),
    ("Open in Agent Hub ↗", "Abrir en Agent Hub ↗"),
    (
        "Buffer change ready for review",
        "Cambio en el búfer listo para revisar",
    ),
    (
        "The proposed edit is staged in Agent Hub. Nothing has been saved to disk.",
        "La edición propuesta está preparada en Agent Hub. No se guardó nada en el disco.",
    ),
    ("Review change", "Revisar cambio"),
    ("ACTIVITY", "ACTIVIDAD"),
    (
        "Connected tools report their own activity. Approvals stay in the original tool unless a request appears here.",
        "Las herramientas conectadas informan su propia actividad. Las aprobaciones se gestionan en la herramienta original, salvo que la solicitud aparezca aquí.",
    ),
    ("Show agents ↗", "Ver agentes ↗"),
    (
        "The reported workspace is unavailable.",
        "El espacio de trabajo informado no está disponible.",
    ),
    ("Ready when you are", "Lista cuando quieras"),
    ("Thinking", "Pensando"),
    ("Coding", "Programando"),
    ("Searching", "Buscando"),
    ("Waiting for approval", "Esperando aprobación"),
    ("Completed", "Completado"),
    ("Needs attention", "Requiere atención"),
    ("Resting", "Descansando"),
    ("Running", "En curso"),
    ("Idle", "Inactivo"),
    ("Failed", "Falló"),
    ("Disconnected", "Desconectado"),
    // Tools panel
    ("Reading Git…", "Leyendo Git…"),
    ("No changes to display.", "No hay cambios para mostrar."),
    ("Agent log", "Registro de agentes"),
    (
        "Open a project to run commands.",
        "Abre un proyecto para ejecutar comandos.",
    ),
    (
        "Run a command from the project folder.",
        "Ejecuta un comando desde la carpeta del proyecto.",
    ),
    (
        "Editor operations appear here.",
        "Las operaciones del editor aparecen aquí.",
    ),
    (
        "Agent activity appears here.",
        "La actividad de los agentes aparece aquí.",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spanish_falls_back_to_english_and_switches_live() {
        set(Language::Spanish);
        assert_eq!(tr("Open folder"), "Abrir carpeta");
        assert_eq!(tr("main.rs"), "main.rs");
        assert_eq!(
            trf("{n} agents working", &[("n", "3".into())]),
            "3 agentes trabajando"
        );
        set(Language::English);
        assert_eq!(tr("Open folder"), "Open folder");
    }

    #[test]
    fn every_routed_interface_string_has_a_spanish_translation() {
        let sources = [
            include_str!("panels.rs"),
            include_str!("hub.rs"),
            include_str!("shell.rs"),
            include_str!("companion_ui.rs"),
            include_str!("files.rs"),
            include_str!("tools.rs"),
            include_str!("skills.rs"),
            include_str!("../editor.rs"),
        ];
        let table = spanish();
        let mut missing = Vec::new();
        for source in sources {
            for marker in ["tr(\"", "trf(\""] {
                for (at, _) in source.match_indices(marker) {
                    // Whole-word calls only: skip `str("…` and similar.
                    let before = source[..at].chars().next_back().unwrap_or(' ');
                    if before.is_alphanumeric() || before == '_' {
                        continue;
                    }
                    let rest = &source[at + marker.len()..];
                    let key = rest.split('"').next().unwrap_or_default();
                    if !table.contains_key(key) {
                        missing.push(key.to_owned());
                    }
                }
            }
        }
        missing.sort();
        missing.dedup();
        assert!(missing.is_empty(), "untranslated keys: {missing:#?}");
        let mut seen = HashMap::new();
        for (en, es) in SPANISH {
            if let Some(prev) = seen.insert(*en, *es) {
                assert_eq!(prev, *es, "conflicting translations for {en}");
            }
        }
    }
}
