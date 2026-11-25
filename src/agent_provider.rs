use covalos_agents::ProviderKind;
use covalos_engine::process::{contain, kill_tree, own_group};
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError},
    },
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Model {
    pub id: String,
    pub name: String,
    pub efforts: Vec<String>,
    pub default_effort: Option<String>,
    pub config: Option<String>,
    pub is_default: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Identity {
    pub email: Option<String>,
    pub plan: Option<String>,
}

#[derive(Default)]
pub(crate) struct Catalog {
    pub identity: Identity,
    pub models: Vec<Model>,
    pub warning: Option<String>,
}

pub(crate) struct Protocol {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<Value>,
    next_id: u64,
    deadline: Instant,
    probe_only: bool,
}

impl Protocol {
    fn start(mut command: Command, timeout: Duration) -> Result<Self, String> {
        own_group(&mut command);
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| e.to_string())?;
        contain(&child);
        let stdin = child.stdin.take().expect("piped");
        let stdout = child.stdout.take().expect("piped");
        let stderr = child.stderr.take().expect("piped");
        let (tx, lines) = mpsc::sync_channel(128);
        let diagnostics = tx.clone();
        thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                if let Some(url) = oauth_link(&line)
                    && diagnostics
                        .send(json!({"method":"covalos/oauth_url","params":{"url":url}}))
                        .is_err()
                {
                    break;
                }
            }
        });
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                let value = serde_json::from_str(&line).ok().or_else(|| {
                    oauth_link(&line)
                        .map(|url| json!({"method":"covalos/oauth_url","params":{"url":url}}))
                });
                if let Some(value) = value
                    && tx.send(value).is_err()
                {
                    break;
                }
            }
        });
        Ok(Self {
            child,
            stdin: Some(stdin),
            lines,
            next_id: 1,
            deadline: Instant::now() + timeout,
            probe_only: false,
        })
    }

    fn write(&mut self, value: Value) -> Result<(), String> {
        let stdin = self.stdin.as_mut().ok_or("The provider input is closed.")?;
        writeln!(stdin, "{value}")
            .and_then(|()| stdin.flush())
            .map_err(|e| e.to_string())
    }

    fn start_phase(&mut self, timeout: Duration) {
        self.deadline = Instant::now() + timeout;
    }

    fn finish(mut self) {
        drop(self.stdin.take());
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if self.child.try_wait().is_ok_and(|status| status.is_some()) {
                break;
            }
            thread::sleep(Duration::from_millis(40));
        }
    }

    fn receive(&self, cancel: &AtomicBool) -> Result<Value, String> {
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err("Sign-in cancelled.".into());
            }
            if Instant::now() >= self.deadline {
                return Err("The provider did not respond in time.".into());
            }
            match self.lines.recv_timeout(Duration::from_millis(100)) {
                Ok(value) => return Ok(value),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    return Err("The provider closed the connection.".into());
                }
            }
        }
    }

    fn call(
        &mut self,
        method: &str,
        params: Value,
        cancel: &AtomicBool,
        mut notify: impl FnMut(&Value),
    ) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id += 1;
        self.write(rpc(id, method, params))?;
        loop {
            let message = self.receive(cancel)?;
            if message["id"] == id && message.get("method").is_none() {
                return response(&message);
            }
            notify(&message);
            if (self.probe_only || method == "session/new")
                && message["method"] == "covalos/oauth_url"
            {
                return Err("Authentication required.".into());
            }
            if let Some(request) = message
                .get("id")
                .filter(|_| message.get("method").is_some())
            {
                self.write(json!({"jsonrpc":"2.0","id":request,"error":{"code":-32601,"message":"Unsupported during setup"}}))?;
            }
        }
    }
}

impl Drop for Protocol {
    fn drop(&mut self) {
        if !self.child.try_wait().is_ok_and(|status| status.is_some()) {
            kill_tree(&mut self.child);
        }
    }
}

const ANTIGRAVITY_OAUTH_PREFIX: &str = "Open the following link to authenticate the ACP server: ";
pub(crate) const ANTIGRAVITY_SETUP_TIMEOUT: Duration = Duration::from_secs(90);
const ANTIGRAVITY_OAUTH_TIMEOUT: Duration = Duration::from_secs(5 * 60);

pub(crate) fn antigravity_auth_method(key: bool) -> &'static str {
    if key {
        "gemini-api-key"
    } else {
        "oauth-personal"
    }
}

pub(crate) fn report_antigravity_browser_url() -> ! {
    if let Some(url) = std::env::args()
        .nth(2)
        .as_deref()
        .and_then(google_oauth_url)
    {
        eprintln!("{ANTIGRAVITY_OAUTH_PREFIX}{url}");
        std::process::exit(0);
    }
    std::process::exit(2);
}

fn google_oauth_url(candidate: &str) -> Option<&str> {
    let candidate = candidate.trim();
    (candidate.starts_with("https://accounts.google.com/")
        && candidate.len() <= 16_384
        && !candidate
            .chars()
            .any(|c| c.is_whitespace() || c.is_control()))
    .then_some(candidate)
}

pub(crate) fn oauth_link(line: &str) -> Option<String> {
    let line = line.trim();
    google_oauth_url(line.strip_prefix(ANTIGRAVITY_OAUTH_PREFIX).unwrap_or(line)).map(str::to_owned)
}

pub(crate) fn rpc(id: u64, method: &str, params: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"method":method,"params":params})
}

pub(crate) fn response(message: &Value) -> Result<Value, String> {
    if let Some(error) = message.get("error") {
        return Err(error["message"]
            .as_str()
            .unwrap_or("Provider request failed.")
            .to_owned());
    }
    message
        .get("result")
        .cloned()
        .ok_or_else(|| "Invalid provider response.".into())
}

pub(crate) fn acp_init() -> Value {
    json!({"protocolVersion":1,"clientInfo":{"name":"covalos","version":env!("CARGO_PKG_VERSION")},"clientCapabilities":{}})
}

pub(crate) fn string(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.chars().any(char::is_control))
        .map(str::to_owned)
}

fn identity(value: &Value) -> Identity {
    Identity {
        email: string(&value["email"]).or_else(|| string(&value["emailAddress"])),
        plan: ["subscriptionType", "planType", "plan", "tierName"]
            .into_iter()
            .find_map(|key| string(&value[key])),
    }
}

fn valid_id(value: &Value) -> Option<String> {
    string(value).filter(|s| !s.starts_with('-') && s.len() <= 512)
}

fn entries(value: &Value) -> impl Iterator<Item = &Value> {
    value.as_array().into_iter().flatten()
}

fn normalize_effort(value: &Value) -> Option<String> {
    let raw = value.as_str()?;
    matches!(
        raw,
        "none" | "minimal" | "low" | "medium" | "high" | "xhigh" | "max" | "ultra"
    )
    .then(|| raw.to_owned())
}

fn model_list(items: &Value, kind: &ProviderKind) -> Vec<Model> {
    entries(items)
        .filter_map(|item| {
            let (id, efforts) = match kind {
                ProviderKind::ClaudeCode => (
                    &item["value"],
                    entries(&item["supportedEffortLevels"])
                        .filter_map(normalize_effort)
                        .collect(),
                ),
                ProviderKind::Codex => (
                    &item["model"],
                    entries(&item["supportedReasoningEfforts"])
                        .filter_map(|e| normalize_effort(&e["reasoningEffort"]))
                        .collect(),
                ),
                _ => return None,
            };
            let id = valid_id(id)?;
            if item["hidden"] == true
                || item["available"] == false
                || item["isAvailable"] == false
                || (kind == &ProviderKind::ClaudeCode
                    && !["haiku", "sonnet", "opus", "fable"].contains(&id.as_str())
                    && !id.starts_with("claude-"))
                || (kind == &ProviderKind::Codex
                    && !(id.starts_with("gpt-")
                        || id.starts_with("codex-")
                        || matches!(id.as_str(), "o1" | "o3" | "o4-mini")))
            {
                return None;
            }
            Some(Model {
                name: string(&item["displayName"]).unwrap_or_else(|| id.clone()),
                is_default: item["isDefault"] == true,
                default_effort: normalize_effort(&item["defaultReasoningEffort"])
                    .or_else(|| normalize_effort(&item["defaultEffort"])),
                id,
                efforts,
                config: None,
            })
        })
        .collect()
}

fn codex_models(client: &mut Protocol) -> Result<Vec<Model>, String> {
    let cancel = AtomicBool::new(false);
    let mut cursor = Value::Null;
    let mut models = Vec::new();
    for _ in 0..20 {
        let page = client.call(
            "model/list",
            json!({"limit":100,"includeHidden":false,"cursor":cursor}),
            &cancel,
            |_| {},
        )?;
        models.extend(model_list(&page["data"], &ProviderKind::Codex));
        if page["nextCursor"].is_null() || page["nextCursor"] == cursor {
            let mut seen = std::collections::HashSet::new();
            models.retain(|model| seen.insert(model.id.clone()));
            return Ok(models);
        }
        cursor = page["nextCursor"].clone();
    }
    Err("The provider returned too many catalog pages.".into())
}

pub(crate) fn codex(mut command: Command, key: bool) -> Result<Option<Catalog>, String> {
    command.args(["-c", "model_provider=\"openai\"", "app-server"]);
    let cancel = AtomicBool::new(false);
    let started = std::time::SystemTime::now();
    let mut client = Protocol::start(command, Duration::from_secs(40))?;
    let init = client.call(
        "initialize",
        json!({"clientInfo":{"name":"covalos","version":env!("CARGO_PKG_VERSION")}}),
        &cancel,
        |_| {},
    )?;
    client.write(json!({"method":"initialized"}))?;
    let auth = client.call(
        "account/read",
        json!({"refreshToken":!key}),
        &cancel,
        |_| {},
    )?;
    if auth["account"].is_null() {
        return Ok(None);
    }
    let expected_type = if key { "apiKey" } else { "chatgpt" };
    if auth["account"]["type"] != expected_type {
        return Err(if key {
            "Codex did not confirm the API-key login."
        } else {
            "The provider did not confirm a subscription login."
        }
        .into());
    }
    let mut catalog = Catalog {
        identity: identity(&auth["account"]),
        ..Catalog::default()
    };
    if !key {
        let limits = client.call("account/rateLimits/read", json!({}), &cancel, |_| {})?;
        if let Some(plan) =
            string(&limits["rateLimits"]["planType"]).filter(|plan| plan != "unknown")
        {
            catalog.identity.plan = Some(plan);
        }
    }
    // The first list triggers Codex's refresh. Publish only a catalog written
    // after this authenticated process started, since the old list could be local.
    let first = codex_models(&mut client);
    if key {
        catalog.models = first?;
    } else {
        let cache = string(&init["codexHome"])
            .map(|home| std::path::PathBuf::from(home).join("models_cache.json"));
        if let Some(cache) = cache {
            let refresh_deadline = (Instant::now() + Duration::from_secs(10)).min(client.deadline);
            while !catalog_refreshed_after(&cache, started) && Instant::now() < refresh_deadline {
                if client.lines.recv_timeout(Duration::from_millis(150))
                    == Err(RecvTimeoutError::Disconnected)
                {
                    break;
                }
            }
            if catalog_refreshed_after(&cache, started) {
                match codex_models(&mut client) {
                    Ok(models) => catalog.models = models,
                    Err(error) => catalog.warning = Some(error),
                }
            }
        }
        if catalog.models.is_empty() && catalog.warning.is_none() {
            catalog.warning = Some(
                "Codex could not confirm a fresh catalog for this account. Check sign-in to retry."
                    .into(),
            );
        }
    }
    Ok(Some(catalog))
}

#[derive(Clone, Debug)]
pub(crate) struct LimitWindow {
    pub label: String,
    pub left: f64,
    pub resets_at: Option<i64>,
}

fn limit_window(label: &str, used: &Value, resets: &Value) -> Option<LimitWindow> {
    let used = used.as_f64().filter(|v| (0.0..=100.0).contains(v))?;
    let resets_at = resets.as_i64().or_else(|| {
        time::OffsetDateTime::parse(
            resets.as_str()?,
            &time::format_description::well_known::Rfc3339,
        )
        .ok()
        .map(|d| d.unix_timestamp())
    });
    Some(LimitWindow {
        label: label.into(),
        left: 100.0 - used,
        resets_at,
    })
}

fn claude_limits(data: &Value) -> Vec<LimitWindow> {
    let limits = &data["rate_limits"];
    let mut rows = Vec::new();
    for (key, label) in [("five_hour", "Session"), ("seven_day", "Weekly")] {
        let bucket = &limits[key];
        rows.extend(limit_window(
            label,
            &bucket["utilization"],
            &bucket["resets_at"],
        ));
    }
    for bucket in entries(&limits["model_scoped"]) {
        if let Some(name) = bucket["display_name"].as_str() {
            rows.extend(limit_window(
                name,
                &bucket["utilization"],
                &bucket["resets_at"],
            ));
        }
    }
    rows
}

// On-demand, read-only RPCs on the selected account's own CLI process.
pub(crate) fn read_limits(
    mut command: Command,
    kind: &ProviderKind,
) -> Result<Vec<LimitWindow>, String> {
    let cancel = AtomicBool::new(false);
    if *kind == ProviderKind::Codex {
        command.args(["-c", "model_provider=\"openai\"", "app-server"]);
        let mut client = Protocol::start(command, Duration::from_secs(25))?;
        client.call(
            "initialize",
            json!({"clientInfo":{"name":"covalos","version":env!("CARGO_PKG_VERSION")}}),
            &cancel,
            |_| {},
        )?;
        client.write(json!({"method":"initialized"}))?;
        let limits = client.call("account/rateLimits/read", json!({}), &cancel, |_| {})?;
        let mut rows = Vec::new();
        for key in ["primary", "secondary"] {
            let bucket = &limits["rateLimits"][key];
            let label = match bucket["windowDurationMins"].as_u64() {
                Some(300) => "Session".to_owned(),
                Some(10080) => "Weekly".to_owned(),
                Some(n) => format!("{n} min"),
                None => key.to_owned(),
            };
            rows.extend(limit_window(
                &label,
                &bucket["usedPercent"],
                &bucket["resetsAt"],
            ));
        }
        return Ok(rows);
    }
    command.args([
        "--print",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        "--setting-sources",
        "user",
    ]);
    let mut client = Protocol::start(command, Duration::from_secs(25))?;
    for (id, method) in [("init", "initialize"), ("limits", "get_usage")] {
        client.write(
            json!({"type":"control_request","request_id":id,"request":{"subtype":method}}),
        )?;
        loop {
            let message = client.receive(&cancel)?;
            let response = &message["response"];
            if message["type"] != "control_response" || response["request_id"] != id {
                continue;
            }
            if response["subtype"] != "success" {
                return Err("Usage limits unavailable".to_owned());
            }
            if id == "limits" {
                return Ok(claude_limits(&response["response"]));
            }
            break;
        }
    }
    Ok(Vec::new())
}

fn catalog_refreshed_after(cache: &Path, started: std::time::SystemTime) -> bool {
    std::fs::metadata(cache)
        .and_then(|file| file.modified())
        .is_ok_and(|modified| modified >= started)
}

pub(crate) fn claude(
    mut status: Command,
    mut command: Command,
    key: bool,
) -> Result<Option<Catalog>, String> {
    let confirmed = if key {
        Identity::default()
    } else {
        status.args(["auth", "status", "--json"]);
        let (ok, text) = super::agents_runtime::capture_status(status, Duration::from_secs(20))?;
        let auth: Value = serde_json::from_str(text.trim())
            .map_err(|_| "Claude did not return a valid authentication status.")?;
        if auth["loggedIn"] == false {
            return Ok(None);
        }
        if !ok || auth["loggedIn"] != true || auth["authMethod"] != "claude.ai" {
            return Err("The provider did not confirm a subscription login.".into());
        }
        identity(&auth)
    };
    command.args([
        "--print",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        "--setting-sources",
        "user",
    ]);
    let mut client = Protocol::start(command, Duration::from_secs(25))?;
    client.write(
        json!({"type":"control_request","request_id":"catalog","request":{"subtype":"initialize"}}),
    )?;
    let cancel = AtomicBool::new(false);
    loop {
        let message = client.receive(&cancel)?;
        let reply = &message["response"];
        if message["type"] == "control_response" && reply["request_id"] == "catalog" {
            if reply["subtype"] != "success" {
                return Err("Claude did not return a model catalog.".into());
            }
            let payload = &reply["response"];
            let reported = identity(&payload["account"]);
            return Ok(Some(Catalog {
                identity: Identity {
                    email: confirmed.email.or(reported.email),
                    plan: confirmed.plan.or(reported.plan),
                },
                models: model_list(&payload["models"], &ProviderKind::ClaudeCode),
                warning: None,
            }));
        }
    }
}

pub(crate) fn configure_acp(command: &mut Command, kind: &ProviderKind) {
    if *kind != ProviderKind::Antigravity {
        return;
    }
    let helper = Path::new(command.get_program()).with_file_name(if cfg!(windows) {
        "localharness_external.exe"
    } else {
        "localharness_external"
    });
    if helper.is_file() {
        command.env("ANTIGRAVITY_HARNESS_PATH", helper);
    }
    command.env("PYTHONUNBUFFERED", "1");
    command.env("AGY_CLI_DISABLE_AUTO_UPDATE", "1");
    if let Ok(exe) = std::env::current_exe() {
        // The ACP server launches the browser through Python. Route the URL
        // through Covalos so only an explicit sign-in may open a tab.
        let exe = exe.to_string_lossy();
        let exe = if cfg!(windows) {
            exe.replace('\\', "/")
        } else {
            exe.into_owned()
        };
        let exe = exe.replace('\'', "'\"'\"'");
        command.env("BROWSER", format!("'{exe}' --antigravity-auth-url %s"));
    }
}

pub(crate) fn acp_args(command: &mut Command, kind: &ProviderKind) {
    configure_acp(command, kind);
    match kind {
        ProviderKind::Gemini => {
            command.args(["--acp", "--approval-mode", "plan"]);
        }
        ProviderKind::OpenCode => {
            command.arg("acp");
        }
        ProviderKind::Antigravity if cfg!(target_os = "linux") => {
            command.arg("--uid=");
        }
        _ => {}
    }
}

/// The same profile Gemini CLI reads. On-disk data can trigger verification,
/// but only the provider's ACP session confirms sign-in.
pub(crate) fn gemini_config(command: &Command) -> Option<PathBuf> {
    let candidates = [
        "GEMINI_CLI_HOME",
        if cfg!(windows) { "USERPROFILE" } else { "HOME" },
    ];
    candidates.iter().find_map(|name| {
        command
            .get_envs()
            .find_map(|(key, value)| (key == *name).then_some(value).flatten())
            .filter(|value| !value.is_empty())
            .map(|home| Path::new(home).join(".gemini"))
    })
}

fn gemini_cached_email(config: &Path) -> Option<String> {
    let settings = gemini_profile_json(config, "settings.json")?;
    if settings["security"]["auth"]["selectedType"] != "oauth-personal" {
        return None;
    }
    let accounts = gemini_profile_json(config, "google_accounts.json")?;
    string(&accounts["active"]).filter(|email| email.contains('@') && email.len() <= 320)
}

fn gemini_profile_json(config: &Path, file: &str) -> Option<Value> {
    let path = config.join(file);
    if std::fs::metadata(&path).ok()?.len() > 65_536 {
        return None;
    }
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

pub(crate) fn option_values(option: &Value) -> impl Iterator<Item = &Value> {
    entries(&option["options"]).flat_map(|item| match item["options"].as_array() {
        Some(group) => group.as_slice(),
        None => std::slice::from_ref(item),
    })
}

pub(crate) fn thinking_option(config: &Value) -> Option<&Value> {
    entries(&config["configOptions"]).find(|option| option["category"] == "thought_level")
}

pub(crate) fn acp_session(params: &Value) -> Vec<Model> {
    let models = &params["models"];
    for option in entries(&params["configOptions"]) {
        if option["category"] != "model" && option["id"] != "model" {
            continue;
        }
        let options = option_values(option);
        let mut catalog: Vec<Model> = options
            .into_iter()
            .filter_map(|item| {
                let id = valid_id(&item["value"])?;
                Some(Model {
                    name: string(&item["name"]).unwrap_or_else(|| id.clone()),
                    is_default: option["currentValue"] == id,
                    config: string(&option["id"]),
                    id,
                    efforts: Vec::new(),
                    default_effort: None,
                })
            })
            .collect();
        if !catalog.is_empty() {
            if let Some(model) = catalog.iter_mut().find(|model| model.is_default)
                && let Some(effort) = thinking_option(params)
            {
                model.efforts = option_values(effort)
                    .filter_map(|item| string(&item["value"]))
                    .collect();
                model.default_effort = string(&effort["currentValue"]);
            }
            return catalog;
        }
    }
    entries(&models["availableModels"])
        .filter_map(|item| {
            let id = valid_id(&item["modelId"])?;
            Some(Model {
                name: string(&item["name"]).unwrap_or_else(|| id.clone()),
                is_default: models["currentModelId"] == id,
                id,
                efforts: Vec::new(),
                default_effort: None,
                config: None,
            })
        })
        .collect()
}

pub(crate) fn acp_probe(
    mut command: Command,
    cwd: &Path,
    kind: &ProviderKind,
    key: bool,
) -> Result<Option<Catalog>, String> {
    let gemini = *kind == ProviderKind::Gemini;
    let config = gemini.then(|| gemini_config(&command)).flatten();
    if gemini {
        // A passive check must not open another browser login.
        command.env("NO_BROWSER", "true");
    }
    acp_args(&mut command, kind);
    let antigravity = *kind == ProviderKind::Antigravity;
    let timeout = if antigravity {
        ANTIGRAVITY_SETUP_TIMEOUT
    } else {
        Duration::from_secs(30)
    };
    let mut client = Protocol::start(command, timeout)?;
    client.probe_only = gemini || antigravity;
    let cancel = AtomicBool::new(false);
    let init = match client.call("initialize", acp_init(), &cancel, |_| {}) {
        Err(error) if gemini && signed_out(&error) => return Ok(None),
        result => result?,
    };
    if init["protocolVersion"] != 1 {
        return Err("The provider did not confirm ACP protocol version 1.".into());
    }
    if antigravity {
        client.start_phase(ANTIGRAVITY_SETUP_TIMEOUT);
        match client.call(
            "authenticate",
            json!({"methodId":antigravity_auth_method(key)}),
            &cancel,
            |_| {},
        ) {
            Err(error) if signed_out(&error) => return Ok(None),
            result => {
                result?;
            }
        }
        client.start_phase(ANTIGRAVITY_SETUP_TIMEOUT);
    }
    let mut interactive = false;
    let result = client.call(
        "session/new",
        json!({"cwd":cwd,"mcpServers":[]}),
        &cancel,
        |message| {
            interactive |= message
                .get("method")
                .and_then(Value::as_str)
                .is_some_and(|method| method.contains("auth") || method.contains("elicitation"));
        },
    );
    match result {
        Err(error) if gemini && !key && gemini_oauth_retired(&error) => Err(error),
        Err(e) if signed_out(&e) || interactive => Ok(None),
        Err(e) => Err(e),
        Ok(session) if !interactive && string(&session["sessionId"]).is_some() => {
            let mut catalog = acp_catalog(&session, key)?;
            if antigravity {
                discover_model_efforts(&mut client, &session, &mut catalog, &cancel)?;
            }
            if !key && let Some(config) = config.as_deref() {
                catalog.identity.email = catalog
                    .identity
                    .email
                    .or_else(|| gemini_cached_email(config));
            }
            Ok(Some(catalog))
        }
        _ => Ok(None),
    }
}

// ACP returns the thinking options for its current model only. Ask the disposable
// discovery session for each model's schema, without sending a user turn.
fn discover_model_efforts(
    client: &mut Protocol,
    session: &Value,
    catalog: &mut Catalog,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let Some(initial) = catalog.models.iter().find(|m| m.is_default).cloned() else {
        return Ok(());
    };
    let Some(config) = initial.config.as_ref() else {
        return Ok(());
    };
    let set = |client: &mut Protocol, model: &str| {
        client.call(
            "session/set_config_option",
            json!({"sessionId":session["sessionId"],"configId":config,"value":model}),
            cancel,
            |_| {},
        )
    };
    let mut changed = false;
    for model in catalog.models.iter_mut().filter(|m| m.id != initial.id) {
        if cancel.load(Ordering::Relaxed) {
            return Err("Sign-in cancelled.".into());
        }
        let response = match set(client, &model.id) {
            Ok(response) => response,
            Err(_) if !cancel.load(Ordering::Relaxed) && Instant::now() < client.deadline => {
                continue;
            }
            Err(error) => return Err(error),
        };
        changed = true;
        if let Some(selected) = acp_session(&response)
            .into_iter()
            .find(|entry| entry.is_default && entry.id == model.id)
        {
            model.efforts = selected.efforts;
            model.default_effort = selected.default_effort;
        }
    }
    if changed {
        set(client, &initial.id)?;
    }
    Ok(())
}

fn acp_catalog(session: &Value, key: bool) -> Result<Catalog, String> {
    if string(&session["sessionId"]).is_none() {
        return Err("The provider did not return a session ID.".into());
    }
    let models = acp_session(session);
    if models.is_empty() && !key {
        return Err("The provider did not confirm an authenticated model catalog.".into());
    }
    let account = session
        .get("account")
        .or_else(|| session["_meta"].get("account"))
        .unwrap_or(&Value::Null);
    let mut identity = identity(account);
    identity.plan = identity
        .plan
        .or_else(|| string(&session["_meta"]["antigravity"]["tier"]["name"]));
    Ok(Catalog {
        identity,
        models,
        warning: None,
    })
}

pub(crate) fn gemini_oauth_retired(error: &str) -> bool {
    let text = error
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    text.contains("this client is no longer supported for gemini code assist for individuals")
        || (text.contains("unsupported_client") && text.contains("antigravity"))
}

pub(crate) fn signed_out(error: &str) -> bool {
    if gemini_oauth_retired(error) {
        return false;
    }
    let error = error.to_ascii_lowercase();
    [
        "authentication required",
        "not authenticated",
        "not logged in",
        "not signed in",
        "unauthorized",
        "login required",
        "manual authorization is required",
        "no browser is available here",
        "gemini api key is missing",
        "401",
    ]
    .iter()
    .any(|text| error.contains(text))
}

pub(crate) fn antigravity_login(
    mut command: Command,
    cwd: &Path,
    cancel: &AtomicBool,
    url: impl Fn(String),
    verify: impl FnOnce(),
) -> Result<Catalog, String> {
    acp_args(&mut command, &ProviderKind::Antigravity);
    let mut client = Protocol::start(command, ANTIGRAVITY_SETUP_TIMEOUT)?;
    let init = client.call("initialize", acp_init(), cancel, |_| {})?;
    if init["protocolVersion"] != 1 {
        return Err("The provider did not confirm ACP protocol version 1.".into());
    }
    let method = entries(&init["authMethods"])
        .find(|item| item["id"] == "oauth-personal")
        .ok_or("The runtime did not offer Google sign-in.")?;
    let report = |value: &Value| {
        for key in ["authUrl", "authorizationUrl", "url"] {
            if let Some(link) = value[key]
                .as_str()
                .and_then(google_oauth_url)
                .map(str::to_owned)
            {
                url(link);
            }
        }
    };
    report(&method["_meta"]);
    client.start_phase(ANTIGRAVITY_OAUTH_TIMEOUT);
    let result = client.call(
        "authenticate",
        json!({"methodId":antigravity_auth_method(false)}),
        cancel,
        |message| report(&message["params"]),
    )?;
    report(&result);
    verify();
    client.start_phase(ANTIGRAVITY_SETUP_TIMEOUT);
    let session = client.call(
        "session/new",
        json!({"cwd":cwd,"mcpServers":[]}),
        cancel,
        |_| {},
    )?;
    let mut catalog = acp_catalog(&session, false)?;
    discover_model_efforts(&mut client, &session, &mut catalog, cancel)?;
    client.finish();
    Ok(catalog)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn antigravity_accepts_its_native_sign_in_link_only() {
        let url = "https://accounts.google.com/o/oauth2/v2/auth?state=test";
        assert_eq!(oauth_link(url).as_deref(), Some(url));
        assert_eq!(
            oauth_link(&format!("{ANTIGRAVITY_OAUTH_PREFIX}{url}")).as_deref(),
            Some(url)
        );
        assert!(oauth_link("https://accounts.google.com.example.invalid/path").is_none());
        assert!(oauth_link(&format!("{url} extra text")).is_none());
    }

    #[test]
    fn antigravity_authenticates_and_verifies_models_on_the_same_process() {
        let root = tempfile::tempdir().unwrap();
        let fixture = root.path().join("acp.cjs");
        std::fs::write(&fixture, r#"
const readline = require('node:readline');
let authenticated = false;
const options = model => [
  {category:'model',id:'model',currentValue:model,options:['actual-fixture','alternate'].map(value=>({name:value,value}))},
  {category:'thought_level',id:'thinking',currentValue:model==='alternate'?'high':'off',
    options:[{group:'levels',options:(model==='alternate'?['high','low']:['off']).map(value=>({value}))}]}
];
readline.createInterface({input:process.stdin}).on('line', line => {
  const req = JSON.parse(line);
  const respond = result => console.log(JSON.stringify({jsonrpc:'2.0',id:req.id,result}));
  if (req.method === 'initialize') {
    respond({protocolVersion:1,authMethods:[{id:'oauth-personal'}]});
  } else if (req.method === 'authenticate' && req.params.methodId === 'oauth-personal') {
    console.log('Open the following link to authenticate the ACP server: https://accounts.google.com/o/oauth2/test');
    authenticated = true;
    respond(null);
  } else if (req.method === 'session/new' && authenticated) {
    respond({sessionId:'verified',account:{email:'fixture@example.invalid'},configOptions:options('actual-fixture')});
  } else if (req.method === 'session/set_config_option' && authenticated) {
    respond({configOptions:options(req.params.value)});
  } else {
    process.exit(2);
  }
});
"#).unwrap();
        let mut command = Command::new("node");
        command.arg(fixture);
        let saw_url = std::cell::Cell::new(false);
        let verified = std::cell::Cell::new(false);
        let catalog = antigravity_login(
            command,
            root.path(),
            &AtomicBool::new(false),
            |url| {
                assert_eq!(url, "https://accounts.google.com/o/oauth2/test");
                saw_url.set(true);
            },
            || verified.set(true),
        )
        .unwrap();
        assert!(saw_url.get() && verified.get());
        assert_eq!(
            catalog.identity.email.as_deref(),
            Some("fixture@example.invalid")
        );
        assert_eq!(catalog.models[0].id, "actual-fixture");
        assert_eq!(catalog.models[0].efforts, ["off"]);
        assert_eq!(catalog.models[1].efforts, ["high", "low"]);
        assert_eq!(catalog.models[1].default_effort.as_deref(), Some("high"));
    }

    #[test]
    fn catalogs_use_provider_model_ids_and_reported_effort() {
        let items = json!([
            {"value":"opus","displayName":"Opus","supportedEffortLevels":["low","max","unknown"]},
            {"value":"default","displayName":"Default (recommended)"},
            {"value":"gemini-fake"},
            {"value":"claude-disabled","available":false}
        ]);
        let catalog = model_list(&items, &ProviderKind::ClaudeCode);
        assert_eq!(catalog.len(), 1);
        assert_eq!(catalog[0].id, "opus");
        assert_eq!(catalog[0].efforts, ["low", "max"]);
        let items = json!([
            {"id":"picker-id","model":"gpt-real","supportedReasoningEfforts":[{"reasoningEffort":"xhigh"}]},
            {"model":"gpt-hidden","hidden":true}
        ]);
        let catalog = model_list(&items, &ProviderKind::Codex);
        assert_eq!(catalog.len(), 1);
        assert_eq!(catalog[0].id, "gpt-real");
        assert_eq!(catalog[0].efforts, ["xhigh"]);
    }

    #[test]
    fn gemini_consumer_oauth_retirement_is_distinct_from_signed_out() {
        let rejection =
            "401: This client is no longer supported for Gemini Code Assist\n for individuals.";
        assert!(gemini_oauth_retired(rejection));
        assert!(!signed_out(rejection));
        assert!(gemini_oauth_retired(
            "UNSUPPORTED_CLIENT. Migrate to Antigravity."
        ));
        assert!(!gemini_oauth_retired(
            "Authentication required. Sign in again."
        ));
        assert!(!gemini_oauth_retired("UNSUPPORTED_CLIENT"));
    }

    #[test]
    fn gemini_identity_uses_the_same_oauth_profile_as_the_cli() {
        let home = tempfile::tempdir().unwrap();
        let mut command = Command::new("gemini");
        command.env("GEMINI_CLI_HOME", home.path());
        let config = gemini_config(&command).unwrap();
        std::fs::create_dir(&config).unwrap();
        std::fs::write(
            config.join("google_accounts.json"),
            r#"{"active":"profile@example.invalid","old":["other@example.invalid"]}"#,
        )
        .unwrap();
        assert_eq!(gemini_cached_email(&config), None);
        std::fs::write(
            config.join("settings.json"),
            r#"{"security":{"auth":{"selectedType":"oauth-personal"}}}"#,
        )
        .unwrap();
        assert_eq!(
            gemini_cached_email(&config).as_deref(),
            Some("profile@example.invalid")
        );
    }

    #[test]
    fn quota_uses_provider_percentages_and_renews_at_the_reported_time() {
        let windows = claude_limits(&json!({"rate_limits":{
            "five_hour":{"utilization":0},
            "seven_day":{"utilization":78,"resets_at":"2026-01-01T00:00:00Z"}
        }}));
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].left, 100.0);
        assert_eq!(windows[1].left, 22.0);
        assert_eq!(windows[1].resets_at, Some(1767225600));
        assert!(claude_limits(&json!({})).is_empty());
    }

    #[test]
    fn acp_models_use_config_option_id_and_grouped_values() {
        let session = json!({"configOptions":[{
            "id":"model-select","category":"model","currentValue":"actual",
            "options":[{"group":"Google","options":[{"value":"actual","name":"Actual"}]}]
        }]});
        let catalog = acp_session(&session);
        assert_eq!(catalog[0].config.as_deref(), Some("model-select"));
        assert!(catalog[0].is_default);
        assert!(catalog[0].efforts.is_empty());
    }
}
