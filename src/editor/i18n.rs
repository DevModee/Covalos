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
    ("Copy output", "Copiar salida"),
    (
        "Copy the terminal text, including scrollback",
        "Copiar el texto de la terminal, incluido el historial",
    ),
    (
        "Interrupt the running program",
        "Interrumpir el programa en ejecución",
    ),
    ("Already formatted.", "El archivo ya tiene formato."),
    (
        "The language server could not format this file.",
        "El servidor de lenguaje no pudo formatear este archivo.",
    ),
    (
        "No language server can format this file.",
        "Ningún servidor de lenguaje puede formatear este archivo.",
    ),
    ("+ Add AI", "+ Añadir IA"),
    ("+ New conversation", "+ Nueva conversación"),
    ("Add", "Añadir"),
    ("Add AI", "Añadir IA"),
    (
        "Add a decision or contract for all agents",
        "Agregá una decisión o contrato para todos los agentes",
    ),
    ("Add an account first.", "Primero agregá una cuenta."),
    ("Add another", "Añadir otra"),
    (
        "Agents are unavailable.",
        "Los agentes no están disponibles.",
    ),
    ("Allow for session", "Permitir en la sesión"),
    (
        "An agent is writing the plan…",
        "Un agente está escribiendo el plan…",
    ),
    (
        "Anyone with the link can open it until you stop the tunnel. No Cloudflare account is used.",
        "Cualquiera con el enlace puede abrirlo hasta que detengas el túnel. No se usa ninguna cuenta de Cloudflare.",
    ),
    ("Automatic", "Automático"),
    ("Cancel plan", "Cancelar plan"),
    ("Check again", "Comprobar de nuevo"),
    ("Check sign-in", "Comprobar sesión"),
    (
        "Choose the planning account",
        "Elegí la cuenta que planifica",
    ),
    ("Close", "Cerrar"),
    ("Close preview", "Cerrar vista previa"),
    ("Close split", "Cerrar división"),
    (
        "Close this conversation (its worktree is removed only if unchanged)",
        "Cerrar esta conversación (su worktree se borra solo si no tiene cambios)",
    ),
    (
        "Close this terminal and stop its process",
        "Cerrar esta terminal y detener su proceso",
    ),
    (
        "Cloudflare creates a temporary trycloudflare.com address that forwards to",
        "Cloudflare crea una dirección temporal trycloudflare.com que redirige a",
    ),
    ("Connect an AI to start", "Conectá una IA para empezar"),
    ("Copy address", "Copiar dirección"),
    ("Copy link", "Copiar enlace"),
    ("Covalos Orchestrator", "Orquestador de Covalos"),
    ("Create link", "Crear enlace"),
    (
        "Creating the temporary link…",
        "Creando el enlace temporal…",
    ),
    ("Default", "Predeterminado"),
    (
        "Default model (optional)",
        "Modelo predeterminado (opcional)",
    ),
    (
        "Describe an objective; the orchestrator plans and assigns it",
        "Describí un objetivo; el orquestador lo planifica y lo asigna",
    ),
    ("Detect again", "Detectar de nuevo"),
    (
        "Each task in its own git worktree",
        "Cada tarea en su propio worktree de git",
    ),
    (
        "Files changed in this folder",
        "Archivos cambiados en esta carpeta",
    ),
    (
        "Finish signing in in the console window.",
        "Terminá de iniciar sesión en la ventana de consola.",
    ),
    ("How to connect", "Cómo conectar"),
    ("Keep mine", "Conservar los míos"),
    ("Label", "Etiqueta"),
    ("Loading the page…", "Cargando la página…"),
    ("Message the agent", "Escribile al agente"),
    ("Model", "Modelo"),
    ("Name (e.g. Work)", "Nombre (p. ej. Trabajo)"),
    ("New conversation", "Nueva conversación"),
    ("New terminal", "Nueva terminal"),
    (
        "No changes in this folder.",
        "No hay cambios en esta carpeta.",
    ),
    ("No problems reported.", "No hay problemas reportados."),
    (
        "No textual diff (new or binary file).",
        "Sin diff de texto (archivo nuevo o binario).",
    ),
    ("Not installed on this PC", "No está instalado en esta PC"),
    ("Open Orchestrator", "Abrir Orquestador"),
    ("Open a project folder", "Abrí una carpeta de proyecto"),
    ("Open in its own window", "Abrir en su propia ventana"),
    (
        "Open in the Covalos preview",
        "Abrir en la vista previa de Covalos",
    ),
    ("Open in the browser", "Abrir en el navegador"),
    ("Open in the editor", "Abrir en el editor"),
    ("Orchestrator", "Orquestador"),
    ("Output", "Salida"),
    ("Paste the API key", "Pegá la clave de API"),
    ("Pause", "Pausar"),
    ("Permissions", "Permisos"),
    (
        "Plans an objective and assigns roles to your accounts",
        "Planifica un objetivo y asigna roles a tus cuentas",
    ),
    ("Ports", "Puertos"),
    ("Preview", "Vista previa"),
    ("Problems", "Problemas"),
    ("Public link for port", "Enlace público para el puerto"),
    ("Publish decision", "Publicar decisión"),
    ("Recover", "Recuperar"),
    ("Refresh", "Actualizar"),
    ("Reload", "Recargar"),
    ("Reload from disk", "Recargar desde el disco"),
    ("Remove", "Quitar"),
    ("Resume", "Reanudar"),
    (
        "Running… open another tab with + to keep working",
        "En ejecución… abrí otra pestaña con + para seguir trabajando",
    ),
    ("Send", "Enviar"),
    ("Set API key…", "Configurar clave de API…"),
    (
        "Share this port publicly?",
        "¿Compartir este puerto públicamente?",
    ),
    ("Share…", "Compartir…"),
    ("Show system ports", "Mostrar puertos del sistema"),
    (
        "Show two terminals side by side",
        "Mostrar dos terminales lado a lado",
    ),
    ("Sign in", "Iniciar sesión"),
    ("Sign in (official)", "Iniciar sesión (oficial)"),
    ("Sign in now", "Iniciar sesión ahora"),
    ("Split", "Dividir"),
    (
        "Split: show a second conversation",
        "Dividir: mostrar una segunda conversación",
    ),
    ("Start objective", "Iniciar objetivo"),
    (
        "Stop the process and its children",
        "Detener el proceso y sus hijos",
    ),
    ("Stop tunnel", "Detener túnel"),
    ("Stopping…", "Deteniendo…"),
    (
        "Stopping… waiting for the provider to exit",
        "Deteniendo… esperando que el proveedor termine",
    ),
    ("Talk to", "Hablar con"),
    ("Technical log", "Registro técnico"),
    (
        "Tell the agent which file is open in the editor",
        "Indicarle al agente qué archivo está abierto en el editor",
    ),
    (
        "The embedded preview needs Windows WebView2. Use Open in the browser.",
        "La vista previa integrada necesita WebView2 de Windows. Usá Abrir en el navegador.",
    ),
    (
        "This account is signed out.",
        "Esta cuenta no tiene la sesión iniciada.",
    ),
    ("Watch its conversation", "Ver su conversación"),
    (
        "WebView2 could not start. Use Open in the browser.",
        "WebView2 no pudo iniciar. Usá Abrir en el navegador.",
    ),
    (
        "Work in its own git worktree",
        "Trabajar en su propio worktree de git",
    ),
    ("Working…", "Trabajando…"),
    (
        "Working… you can stop the turn",
        "Trabajando… podés detener el turno",
    ),
    ("added", "añadida"),
    ("default model", "modelo predeterminado"),
    ("detected", "detectado"),
    ("events", "eventos"),
    ("is not installed", "no está instalado"),
    ("is running", "está en ejecución"),
    ("is starting…", "está iniciando…"),
    ("not found", "no encontrado"),
    ("other model id", "otro id de modelo"),
    ("stopped", "se detuvo"),
    ("Checking…", "Comprobando…"),
    ("Signed out", "Sesión cerrada"),
    ("API key stored", "Clave de API guardada"),
    ("No API key", "Sin clave de API"),
    ("Status unknown", "Estado desconocido"),
    ("Not installed", "No instalado"),
    ("Read only", "Solo lectura"),
    ("Supervised", "Supervisado"),
    ("Edit inside workdir", "Editar dentro de la carpeta"),
    ("CLI sign-in", "Sesión del CLI"),
    ("Isolated sign-in", "Sesión aislada"),
    ("API key", "Clave de API"),
    ("Template planner", "Planificador de plantilla"),
    ("One role", "Un rol"),
    ("An agent writes the plan", "Un agente escribe el plan"),
    ("Port", "Puerto"),
    ("Address", "Dirección"),
    ("Process", "Proceso"),
    (
        "Use my existing CLI sign-in",
        "Usar mi sesión actual del CLI",
    ),
    ("New isolated sign-in", "Nueva sesión aislada"),
    ("Anthropic API key", "Clave de API de Anthropic"),
    ("OpenAI API key", "Clave de API de OpenAI"),
    ("Gemini API key", "Clave de API de Gemini"),
    ("Quick chat", "Chat rápido"),
    ("Roles", "Roles"),
    ("Budget", "Presupuesto"),
    ("Objective", "Objetivo"),
    ("Results", "Resultados"),
    ("Live run", "Ejecución en vivo"),
    ("Shared context", "Contexto compartido"),
    ("Interrupted plans", "Planes interrumpidos"),
    ("Agents", "Agentes"),
    (
        "Agents — sessions, providers and the side chat (Ctrl+Shift+A)",
        "Agentes — sesiones, proveedores y chat lateral (Ctrl+Shift+A)",
    ),
    ("Explorer zoom", "Zoom del Explorador"),
    ("Toggle sidebar", "Mostrar u ocultar barra lateral"),
    ("Filter files…", "Filtrar archivos…"),
    ("Clear filter", "Limpiar filtro"),
    ("Focus mode", "Modo enfoque"),
    ("Notifications", "Notificaciones"),
    ("Search notifications", "Buscar notificaciones"),
    ("No notifications yet.", "Todavía no hay notificaciones."),
    ("Clear all", "Limpiar todo"),
    ("All", "Todo"),
    ("Covalos", "Covalos"),
    ("Agent", "Agente"),
    ("GitHub", "GitHub"),
    ("Close", "Cerrar"),
    ("Initial setup", "Configuración inicial"),
    ("Choose a few defaults before you start.", "Elegí algunos valores iniciales antes de empezar."),
    ("Done", "Listo"),
    ("Import", "Importar"),
    ("Import settings from another editor", "Importar configuración de otro editor"),
    ("Import from another editor", "Importar desde otro editor"),
    ("Import matching setup from VS Code, Cursor, Windsurf, or Zed.", "Importar configuración compatible de VS Code, Cursor, Windsurf o Zed."),
    ("Import font size, theme, and density from VS Code, Cursor, Windsurf, or Zed.", "Importar tamaño de fuente, tema y densidad de VS Code, Cursor, Windsurf o Zed."),
    ("Use tighter spacing across code and lists.", "Usar espaciado más compacto en código y listas."),
    ("Disable animations and ambient transitions.", "Desactivar animaciones y transiciones ambientales."),
    ("Search settings", "Buscar configuración"),
    ("Editor", "Editor"),
    ("Integrations", "Integraciones"),
    ("Version", "Versión"),
    ("Switch interface language live.", "Cambiar el idioma de la interfaz en vivo."),
    ("Show the quick setup checklist on the Welcome view.", "Mostrar la lista de configuración rápida en la pantalla de inicio."),
    ("Show again", "Mostrar de nuevo"),
    ("Visible", "Visible"),
    ("Keyboard shortcut to open Library Studio.", "Atajo de teclado para abrir Library Studio."),
    ("Syntax palette independent of the UI theme.", "Paleta de sintaxis independiente del tema de la interfaz."),
    ("Suppress taskbar attention when agents finish.", "Silenciar avisos en la barra de tareas cuando terminan los agentes."),
    ("Pull Requests", "Pull Requests"),
    ("Issues", "Issues"),
    ("Install GitHub CLI (gh) and run gh auth login to view PRs and issues.", "Instalá GitHub CLI (gh) y ejecutá gh auth login para ver PRs e issues."),
    ("No open items found.", "No se encontraron elementos abiertos."),
    ("Commit message", "Mensaje del commit"),
    ("Stage all", "Preparar todo"),
    ("Commit", "Commit"),
    ("Filter changes", "Filtrar cambios"),
    ("History", "Historial"),
    ("Search message, author, hash…", "Buscar mensaje, autor, hash…"),
    ("Files", "Archivos"),
    ("Navigation", "Navegación"),
    ("Surface brightness", "Claridad de superficies"),
    ("Adjust background clarity across chrome, Explorer, and editor.", "Ajustá la claridad de fondo en la barra, el Explorador y el editor."),
    ("Projects", "Proyectos"),
    (
        "Agents work inside a project. Open a folder, then start a conversation here.",
        "Los agentes trabajan dentro de un proyecto. Abrí una carpeta para iniciar una conversación acá.",
    ),
    (
        "Covalos drives Claude Code and Codex through their official CLIs. Your sign-in stays with each CLI.",
        "Covalos ejecuta Claude Code y Codex mediante sus CLIs oficiales. Tu inicio de sesión queda en cada CLI.",
    ),
    ("Where should we begin?", "¿Por dónde empezamos?"),
    ("Ask anything, @ to add context", "Preguntá lo que quieras, @ para agregar contexto"),
    ("Auto skills", "Skills automáticas"),
    ("Manual skills", "Skills manuales"),
    (
        "Automatically attach matching enabled skills without asking.",
        "Adjuntar automáticamente las skills activas compatibles sin preguntar.",
    ),
    ("Review my changes", "Revisar mis cambios"),
    ("Plan an implementation", "Planificar una implementación"),
    ("Find and fix a bug", "Buscar y corregir un bug"),
    ("Write tests for a change", "Escribir tests para un cambio"),
    ("Importing skill from GitHub…", "Importando skill desde GitHub…"),
    ("Connected accounts & CLIs", "Cuentas y CLIs conectados"),
    (
        "Use your Claude, Codex, or Gemini subscription via official CLI or connect an API key.",
        "Usá tu suscripción de Claude, Codex o Gemini mediante el CLI oficial o conectá una API key.",
    ),
    ("Install CLI", "Instalar CLI"),
    ("Installing CLI…", "Instalando CLI…"),
    ("Connect with your subscription CLI or an API key.", "Conectá con tu suscripción por CLI o una API key."),
    ("Ask anything. @ files, $ skills, / commands", "Preguntá lo que quieras. @ archivos, $ skills, / comandos"),
    ("Effort", "Esfuerzo"),
    ("Low", "Bajo"),
    ("Medium", "Medio"),
    ("High", "Alto"),
    ("View usage", "Ver uso"),
    ("turns", "turnos"),
    ("Agent usage", "Uso de agentes"),
    ("Input tokens", "Tokens de entrada"),
    ("Output tokens", "Tokens de salida"),
    ("Session total", "Total de sesiones"),
    ("Local workspaces", "Espacios locales"),
    ("Turns", "Turnos"),
    ("Completed turns", "Turnos completados"),
    ("Start a conversation to see token usage.", "Iniciá una conversación para ver el uso de tokens."),
    ("Models", "Modelos"),
    ("Accounts", "Cuentas"),
    ("Detecting…", "Detectando…"),
    ("Sessions", "Sesiones"),
    ("Global agent rules (local)", "Reglas globales de agentes (local)"),
    (
        "Saved locally on this PC (agent-rules.md). Applied to models, Agent Hub, and Luma.",
        "Guardado localmente en esta PC (agent-rules.md). Se aplica a los modelos, Agent Hub y Luma.",
    ),
    (
        "Write strict rules for all AI models, Agent Hub, and Luma…",
        "Escribí reglas estrictas para todos los modelos de IA, Agent Hub y Luma…",
    ),
    ("Save rules", "Guardar reglas"),
    ("Saved local agent rules.", "Reglas locales de agentes guardadas."),
    ("Thinking effort", "Esfuerzo de razonamiento"),
    ("Extra High", "Extra alto"),
    ("Interface scale", "Escala de la interfaz"),
    ("Reset editor zoom", "Restablecer zoom del editor"),
    ("Reset Explorer zoom", "Restablecer zoom del Explorador"),
    ("Reset interface scale", "Restablecer escala"),
    (
        "Ctrl + wheel or Ctrl +/− zooms the panel under the pointer; Ctrl+0 resets it.",
        "Ctrl + rueda o Ctrl +/− amplía el panel bajo el puntero; Ctrl+0 lo restablece.",
    ),
    ("Navigation transitions", "Transiciones de navegación"),
    ("Subtle", "Sutiles"),
    ("Expressive", "Expresivas"),
    ("Discord Rich Presence", "Discord Rich Presence"),
    (
        "Show Covalos activity in Discord",
        "Mostrar la actividad de Covalos en Discord",
    ),
    ("Application ID", "Application ID"),
    ("Show current activity", "Mostrar la actividad actual"),
    ("Show language", "Mostrar el lenguaje"),
    ("Show project name", "Mostrar el nombre del proyecto"),
    ("Show session time", "Mostrar el tiempo de sesión"),
    ("Discord is not running", "Discord no está abierto"),
    ("Connected", "Conectado"),
    (
        "Enter a valid Application ID",
        "Ingresá un Application ID válido",
    ),
    (
        "Paths, file names, prompts and accounts are never sent. Only the local Discord app receives the status.",
        "Nunca se envían rutas, nombres de archivo, prompts ni cuentas. Solo la app local de Discord recibe el estado.",
    ),
    ("Claude · Codex · Gemini", "Claude · Codex · Gemini"),
    ("Library Studio", "Library Studio"),
    ("Library Studio shortcut", "Atajo de Library Studio"),
    ("Open Library Studio", "Abrir Library Studio"),
    ("Close Library Studio", "Cerrar Library Studio"),
    ("Accent color", "Color de acento"),
    ("Add to favorites", "Agregar a favoritos"),
    ("Remove from favorites", "Quitar de favoritos"),
    ("Add to project", "Añadir al proyecto"),
    (
        "Already part of Covalos. Use the Playground tab to tune and apply it.",
        "Ya forma parte de Covalos. Usá la pestaña Playground para ajustarlo y aplicarlo.",
    ),
    ("Available offline", "Disponible sin conexión"),
    ("Back", "Volver"),
    ("CATEGORIES", "CATEGORÍAS"),
    ("LIBRARY", "BIBLIOTECA"),
    ("FILTERS", "FILTROS"),
    ("Cached locally.", "Guardado en caché local."),
    ("Not stored by Covalos.", "Covalos no lo almacena."),
    ("Categories", "Categorías"),
    ("Close details", "Cerrar detalles"),
    ("Code", "Código"),
    ("Commands", "Comandos"),
    ("Compatible", "Compatible"),
    ("Confirm and install", "Confirmar e instalar"),
    (
        "Conflicts (existing files)",
        "Conflictos (archivos existentes)",
    ),
    ("Copy install command", "Copiar comando de instalación"),
    ("Created", "Creados"),
    ("Modified", "Modificados"),
    ("Damping ratio", "Amortiguamiento"),
    ("Stiffness", "Rigidez"),
    ("Pixel size", "Tamaño de píxel"),
    ("Dependencies", "Dependencias"),
    ("Dev dependencies", "Dependencias de desarrollo"),
    ("npm dependencies", "Dependencias npm"),
    ("Registry dependencies", "Dependencias del registro"),
    ("Discover", "Descubrir"),
    ("Done", "Listo"),
    ("External", "Externo"),
    ("Favorites", "Favoritos"),
    (
        "Fetching from the official registry…",
        "Descargando desde el registro oficial…",
    ),
    ("Files", "Archivos"),
    ("For this workspace", "Para este workspace"),
    ("compatible results first", "primero los compatibles"),
    ("Framework", "Framework"),
    ("Free", "Gratis"),
    ("Free + Pro", "Gratis + Pro"),
    ("Paid", "Pago"),
    ("Included in Covalos", "Incluido en Covalos"),
    ("Ready to install", "Listo para instalar"),
    ("Needs setup", "Requiere configuración"),
    ("Not compatible", "No compatible"),
    ("Install", "Instalar"),
    ("Installation failed", "La instalación falló"),
    ("Installed", "Instalados"),
    ("Installing {id}…", "Instalando {id}…"),
    ("License", "Licencia"),
    (
        "License: {license}. A backup of package.json, lockfiles and config is taken first.",
        "Licencia: {license}. Antes se respaldan package.json, los lockfiles y la configuración.",
    ),
    ("Link only", "Solo enlace"),
    ("Load source", "Cargar código"),
    (
        "Loads the component source from its official registry and caches it locally (permissive license).",
        "Carga el código del componente desde su registro oficial y lo guarda en caché local (licencia permisiva).",
    ),
    (
        "Loads the component source from its official registry for viewing. Covalos does not store it: the license forbids redistribution.",
        "Carga el código del componente desde su registro oficial solo para verlo. Covalos no lo almacena: la licencia prohíbe redistribuirlo.",
    ),
    (
        "Native Covalos resource. Its implementation lives in the Covalos source tree:",
        "Recurso nativo de Covalos. Su implementación está en el código de Covalos:",
    ),
    ("Native to Covalos", "Nativo de Covalos"),
    (
        "Rendered by the editor itself, available offline.",
        "Renderizado por el propio editor, disponible sin conexión.",
    ),
    ("No framework detected", "Sin framework detectado"),
    (
        "No results. Try a broader term or turn off a filter.",
        "Sin resultados. Probá un término más amplio o quitá un filtro.",
    ),
    ("No workspace open", "Sin workspace abierto"),
    ("Not synced yet", "Sin sincronizar"),
    (
        "Official registries reviewed for Library Studio. Only metadata is stored for sources that forbid redistribution.",
        "Registros oficiales revisados para Library Studio. Para las fuentes que prohíben redistribuir solo se guardan metadatos.",
    ),
    (
        "Official registries, licenses and what Covalos may store.",
        "Registros oficiales, licencias y qué puede almacenar Covalos.",
    ),
    ("Offline", "Sin conexión"),
    ("Open license", "Licencia abierta"),
    (
        "Only Covalos resources are available offline. Sync the catalog to add the official registries.",
        "Sin conexión solo están los recursos de Covalos. Sincronizá el catálogo para sumar los registros oficiales.",
    ),
    (
        "Open a workspace to install components into it.",
        "Abrí un workspace para instalar componentes en él.",
    ),
    ("Open documentation", "Abrir documentación"),
    ("Open live demo", "Abrir demo en vivo"),
    ("Overview", "Resumen"),
    (
        "Overwrite the conflicting files",
        "Sobrescribir los archivos en conflicto",
    ),
    ("Playground", "Playground"),
    ("Pricing", "Precio"),
    (
        "Reading the registry item and checking the workspace…",
        "Leyendo el ítem del registro y revisando el workspace…",
    ),
    ("Recently viewed", "Vistos recientemente"),
    ("Registry item", "Ítem del registro"),
    ("Repository", "Repositorio"),
    ("Requires", "Requiere"),
    (
        "Restores package.json, lockfiles and config from the backup and deletes the created files.",
        "Restaura package.json, lockfiles y configuración desde el respaldo y elimina los archivos creados.",
    ),
    ("Retry", "Reintentar"),
    ("Revert changes", "Revertir cambios"),
    ("Revert failed", "No se pudo revertir"),
    (
        "Reverted. Packages already in node_modules are pruned by your package manager on the next install.",
        "Revertido. Los paquetes que ya están en node_modules los limpia tu gestor de paquetes en la próxima instalación.",
    ),
    ("Review before installing", "Revisá antes de instalar"),
    (
        "Search components, animations, backgrounds, icons…",
        "Buscar componentes, animaciones, fondos, iconos…",
    ),
    ("Source", "Fuente"),
    ("Sources", "Fuentes"),
    (
        "Source is not shown: the license is commercial or could not be verified. Open the official site instead.",
        "El código no se muestra: la licencia es comercial o no se pudo verificar. Abrí el sitio oficial.",
    ),
    ("Sync catalog", "Sincronizar catálogo"),
    (
        "Sync the catalog to see components for this workspace.",
        "Sincronizá el catálogo para ver componentes para este workspace.",
    ),
    ("Syncing {n} sources…", "Sincronizando {n} fuentes…"),
    ("Target: {path}", "Destino: {path}"),
    (
        "The preview above updates live.",
        "La vista previa se actualiza en vivo.",
    ),
    (
        "Unreachable on last sync",
        "Inaccesible en la última sincronización",
    ),
    ("Use as Home background", "Usar como fondo de inicio"),
    ("Variants", "Variantes"),
    (
        "Web components run in a browser runtime. Covalos keeps its renderer native, so a sandboxed web preview is not part of this build. Open the official demo, or install the component and run your project's dev server.",
        "Los componentes web se ejecutan en un navegador. Covalos mantiene su renderer nativo, así que esta versión no incluye una vista previa web aislada. Abrí la demo oficial, o instalá el componente y ejecutá el servidor de desarrollo de tu proyecto.",
    ),
    ("just now", "recién"),
    ("{n} days ago", "hace {n} días"),
    ("{n} entries cached", "{n} entradas en caché"),
    (
        "{n} entries · offline catalog",
        "{n} entradas · catálogo sin conexión",
    ),
    (
        "{n} entries · synced {age}",
        "{n} entradas · sincronizado {age}",
    ),
    ("{n} h ago", "hace {n} h"),
    ("{n} min ago", "hace {n} min"),
    (
        "{n} more — refine the search to see them",
        "{n} más — refiná la búsqueda para verlos",
    ),
    ("{n} results", "{n} resultados"),
    ("Components", "Componentes"),
    ("Animations", "Animaciones"),
    ("Backgrounds", "Fondos"),
    ("Icons", "Iconos"),
    ("Typography", "Tipografía"),
    ("Templates", "Plantillas"),
    ("AI Components", "Componentes de IA"),
    ("Data Visualization", "Visualización de datos"),
    ("Open source", "Código abierto"),
    (
        "Use only, no redistribution",
        "Uso permitido, sin redistribución",
    ),
    ("Commercial", "Comercial"),
    ("License not verified", "Licencia no verificada"),
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
    ("Welcome to Covalos", "Te damos la bienvenida a Covalos"),
    ("About Covalos", "Acerca de Covalos"),
    (
        "Native Rust IDE with multi-agent orchestration",
        "IDE nativo en Rust con orquestación multiagente",
    ),
    (
        "Nocturne Design System · Native Rust renderer (eframe/egui).",
        "Sistema de diseño Nocturne · Renderer nativo en Rust (eframe/egui).",
    ),
    (
        "Third-party assets: Lucide Icons (ISC), Material Icon Theme (MIT), Geist & IBM Plex Mono (OFL).",
        "Recursos de terceros: Lucide Icons (ISC), Material Icon Theme (MIT), Geist e IBM Plex Mono (OFL).",
    ),
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
        "Personaje original de Covalos. Su estado sigue la actividad real de agentes y búsquedas.",
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
            include_str!("agent_chat.rs"),
            include_str!("agent_hub.rs"),
            include_str!("terminal.rs"),
            include_str!("preview.rs"),
            include_str!("lsp.rs"),
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
