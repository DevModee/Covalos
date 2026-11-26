//! Release checks and installer download for auto-update.
//!
//! - HTTP goes through curl with the request on stdin, like `cloud.rs`.
//! - Releases come from the public GitHub API; installers are the stable asset
//!   names published by the release workflow, each with a `.sha256` sidecar.
//! - Everything here is plain data and blocking IO. The editor runs the
//!   blocking calls on background threads and feeds the results to
//!   [`Updates`], which owns the state machine.
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    cmp::Ordering,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const LATEST_URL: &str = "https://api.github.com/repos/DevModee/Covalos/releases/latest";
const RELEASES_PAGE: &str = "https://github.com/DevModee/Covalos/releases";
const USER_AGENT: &str = "Covalos-Updater";
const MAX_API_RESPONSE: usize = 2 * 1024 * 1024;
const MAX_NOTES: usize = 4000;
const MAX_DOWNLOAD_SECONDS: &str = "3600";
const POLL: Duration = Duration::from_millis(120);
pub(crate) const NO_RELEASE: &str = "No public release found";

// ---------------------------------------------------------------- versions

/// A SemVer version. `pre` is the pre-release part, without the dash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub pre: Option<String>,
}

impl Version {
    /// Accepts `1.2.3`, `v1.2.3`, `1.2.3-beta.1` and `1.2.3+build` (build ignored).
    pub(crate) fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let text = text
            .strip_prefix('v')
            .or_else(|| text.strip_prefix('V'))
            .unwrap_or(text);
        let text = text.split('+').next().unwrap_or_default();
        let (core, pre) = match text.split_once('-') {
            Some((core, pre)) => (core, Some(pre)),
            None => (text, None),
        };
        let mut parts = core.split('.');
        let mut number = || -> Option<u64> {
            let part = parts.next()?;
            if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            part.parse().ok()
        };
        let (major, minor, patch) = (number()?, number()?, number()?);
        if parts.next().is_some() {
            return None;
        }
        let pre = match pre {
            None => None,
            Some(pre) => {
                let valid = !pre.is_empty()
                    && pre.split('.').all(|id| {
                        !id.is_empty() && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
                    });
                if !valid {
                    return None;
                }
                Some(pre.to_owned())
            }
        };
        Some(Self {
            major,
            minor,
            patch,
            pre,
        })
    }

    pub(crate) fn current() -> Self {
        Self::parse(env!("CARGO_PKG_VERSION")).unwrap_or(Self {
            major: 0,
            minor: 0,
            patch: 0,
            pre: None,
        })
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if let Some(pre) = &self.pre {
            write!(f, "-{pre}")?;
        }
        Ok(())
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.major, self.minor, self.patch)
            .cmp(&(other.major, other.minor, other.patch))
            .then_with(|| match (&self.pre, &other.pre) {
                (None, None) => Ordering::Equal,
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (Some(a), Some(b)) => compare_pre(a, b),
            })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// SemVer pre-release precedence: numeric ids compare as numbers and sort
/// below alphanumeric ids; a longer list wins when the shared part is equal.
fn compare_pre(a: &str, b: &str) -> Ordering {
    let numeric = |id: &str| id.bytes().all(|c| c.is_ascii_digit());
    let mut left = a.split('.');
    let mut right = b.split('.');
    loop {
        match (left.next(), right.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) => {
                let order = match (numeric(x), numeric(y)) {
                    (true, true) => {
                        let (x, y) = (x.trim_start_matches('0'), y.trim_start_matches('0'));
                        x.len().cmp(&y.len()).then_with(|| x.cmp(y))
                    }
                    (true, false) => Ordering::Less,
                    (false, true) => Ordering::Greater,
                    (false, false) => x.cmp(y),
                };
                if order != Ordering::Equal {
                    return order;
                }
            }
        }
    }
}

pub(crate) fn is_newer(latest: &Version, current: &Version) -> bool {
    latest > current
}

// ------------------------------------------------------- platform, assets

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Platform {
    pub os: &'static str,
    pub arch: &'static str,
}

impl Platform {
    pub(crate) fn current() -> Self {
        Self {
            os: std::env::consts::OS,
            arch: std::env::consts::ARCH,
        }
    }
}

/// True when this process runs from an AppImage (Linux).
pub(crate) fn running_appimage() -> bool {
    std::env::var_os("APPIMAGE").is_some_and(|v| !v.is_empty())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
}

/// The installer for this platform among a release's assets.
pub(crate) fn select_asset(platform: Platform, assets: &[Asset], appimage: bool) -> Option<&Asset> {
    let wanted = match (platform.os, platform.arch) {
        ("windows", "x86_64" | "aarch64") => "Covalos-Windows-x64-Setup.exe",
        ("macos", "aarch64") => "Covalos-macOS-arm64.dmg",
        ("macos", "x86_64") => "Covalos-macOS-x64.dmg",
        ("linux", "x86_64") if appimage => "Covalos-Linux-x86_64.AppImage",
        ("linux", "x86_64") => "Covalos-Linux-amd64.deb",
        _ => return None,
    };
    assets.iter().find(|asset| asset.name == wanted)
}

/// The `<name>.sha256` sidecar of an asset, when the release has one.
pub(crate) fn checksum_asset<'a>(name: &str, assets: &'a [Asset]) -> Option<&'a Asset> {
    let wanted = format!("{name}.sha256");
    assets.iter().find(|asset| asset.name == wanted)
}

// ---------------------------------------------------------------- releases

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Release {
    pub tag: String,
    pub version: Version,
    pub notes: String,
    pub html_url: String,
    pub assets: Vec<Asset>,
}

/// Parses the GitHub `releases/latest` response.
pub(crate) fn parse_release(json: &str) -> Result<Release, String> {
    let value: Value =
        serde_json::from_str(json).map_err(|_| "The update server sent an unreadable answer.")?;
    if value["draft"].as_bool() == Some(true) || value["prerelease"].as_bool() == Some(true) {
        return Err(NO_RELEASE.into());
    }
    let tag = value["tag_name"]
        .as_str()
        .filter(|t| !t.is_empty())
        .ok_or("The release has no version tag.")?
        .to_owned();
    let version = Version::parse(&tag).ok_or("The release tag is not a version number.")?;
    let notes = value["body"]
        .as_str()
        .unwrap_or_default()
        .trim()
        .chars()
        .take(MAX_NOTES)
        .collect();
    let html_url = value["html_url"]
        .as_str()
        .filter(|u| u.starts_with("https://"))
        .unwrap_or(RELEASES_PAGE)
        .to_owned();
    let assets = value["assets"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|item| {
                    let name = item["name"].as_str()?;
                    let url = item["browser_download_url"].as_str()?;
                    if name.is_empty() || !url.starts_with("https://") {
                        return None;
                    }
                    Some(Asset {
                        name: name.to_owned(),
                        url: url.to_owned(),
                        size: item["size"].as_u64().unwrap_or(0),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(Release {
        tag,
        version,
        notes,
        html_url,
        assets,
    })
}

// --------------------------------------------------------------- checksums

/// Reads the hash for `name` from a `.sha256` file. Accepts `hex  name`,
/// `hex *name` and a bare `hex`.
pub(crate) fn parse_checksum(text: &str, name: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let mut words = line.split_whitespace();
        let hash = words.next()?;
        if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        if let Some(file) = words.next() {
            let file = file.trim_start_matches('*');
            let base = file.rsplit(['/', '\\']).next().unwrap_or(file);
            if base != name {
                return None;
            }
        }
        Some(hash.to_ascii_lowercase())
    })
}

pub(crate) fn sha256_of(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|e| format!("Could not read the download. {e}"))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|e| format!("Could not read the download. {e}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

pub(crate) fn verify_sha256(path: &Path, expected: &str) -> Result<(), String> {
    let actual = sha256_of(path)?;
    if actual.eq_ignore_ascii_case(expected.trim()) {
        Ok(())
    } else {
        Err("The download did not match its checksum.".into())
    }
}

// ------------------------------------------------------------ state machine

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum UpdateState {
    Idle,
    Checking,
    UpToDate,
    Available(Release),
    Downloading {
        release: Release,
        received: u64,
        total: u64,
    },
    ReadyToRestart {
        release: Release,
        installer: PathBuf,
    },
    Error(String),
}

/// The update state plus the version the user dismissed. Transition methods
/// return whether they were accepted; invalid ones leave the state alone.
#[derive(Debug)]
pub(crate) struct Updates {
    pub state: UpdateState,
    dismissed: Option<String>,
}

impl Default for Updates {
    fn default() -> Self {
        Self {
            state: UpdateState::Idle,
            dismissed: None,
        }
    }
}

impl Updates {
    pub(crate) fn set_dismissed(&mut self, tag: Option<String>) {
        self.dismissed = tag;
    }

    pub(crate) fn dismissed(&self) -> Option<&str> {
        self.dismissed.as_deref()
    }

    /// True when a worker thread is expected to report back.
    pub(crate) fn is_busy(&self) -> bool {
        matches!(
            self.state,
            UpdateState::Checking | UpdateState::Downloading { .. }
        )
    }

    pub(crate) fn on_check_started(&mut self) -> bool {
        match self.state {
            UpdateState::Idle
            | UpdateState::UpToDate
            | UpdateState::Available(_)
            | UpdateState::Error(_) => {
                self.state = UpdateState::Checking;
                true
            }
            _ => false,
        }
    }

    /// A manual check shows everything, including a dismissed version and
    /// errors. A silent check hides both.
    pub(crate) fn on_checked(&mut self, result: Result<Option<Release>, String>, manual: bool) {
        if self.state != UpdateState::Checking {
            return;
        }
        self.state = match result {
            Ok(None) => UpdateState::UpToDate,
            Ok(Some(release)) => {
                if manual {
                    self.dismissed = None;
                }
                if self.dismissed.as_deref() == Some(release.tag.as_str()) {
                    UpdateState::Idle
                } else {
                    UpdateState::Available(release)
                }
            }
            Err(_) if !manual => UpdateState::Idle,
            Err(message) => UpdateState::Error(message),
        };
    }

    /// Hides the announced version until a newer one appears.
    pub(crate) fn dismiss(&mut self) -> bool {
        match &self.state {
            UpdateState::Available(release) => {
                self.dismissed = Some(release.tag.clone());
                self.state = UpdateState::Idle;
                true
            }
            UpdateState::Error(_) => {
                self.state = UpdateState::Idle;
                true
            }
            _ => false,
        }
    }

    pub(crate) fn on_download_started(&mut self, total: u64) -> bool {
        match &self.state {
            UpdateState::Available(release) => {
                self.state = UpdateState::Downloading {
                    release: release.clone(),
                    received: 0,
                    total,
                };
                true
            }
            _ => false,
        }
    }

    /// Progress never goes backwards and never passes a known total.
    pub(crate) fn on_progress(&mut self, bytes: u64) {
        if let UpdateState::Downloading {
            received, total, ..
        } = &mut self.state
        {
            let bytes = if *total > 0 { bytes.min(*total) } else { bytes };
            *received = (*received).max(bytes);
        }
    }

    pub(crate) fn on_downloaded(&mut self, result: Result<PathBuf, String>) {
        let UpdateState::Downloading { release, .. } = &self.state else {
            return;
        };
        self.state = match result {
            Ok(installer) => UpdateState::ReadyToRestart {
                release: release.clone(),
                installer,
            },
            Err(message) => UpdateState::Error(message),
        };
    }

    pub(crate) fn on_error(&mut self, message: String) -> bool {
        if self.state == UpdateState::Idle {
            return false;
        }
        self.state = UpdateState::Error(message);
        true
    }

    /// Download progress in 0..=1, when the size is known.
    pub(crate) fn fraction(&self) -> Option<f32> {
        match &self.state {
            UpdateState::Downloading {
                received, total, ..
            } if *total > 0 => Some((*received as f32 / *total as f32).clamp(0.0, 1.0)),
            _ => None,
        }
    }
}

// ------------------------------------------------------- dismissed version

fn state_file() -> Option<PathBuf> {
    crate::paths::config_dir().map(|dir| dir.join("update.json"))
}

/// The dismissed version from `update.json`. A missing or corrupt file means none.
pub(crate) fn load_dismissed() -> Option<String> {
    let text = fs::read_to_string(state_file()?).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    value["dismissed"]
        .as_str()
        .filter(|tag| !tag.is_empty())
        .map(str::to_owned)
}

pub(crate) fn save_dismissed(tag: Option<&str>) {
    let Some(path) = state_file() else { return };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(path, serde_json::json!({ "dismissed": tag }).to_string());
}

// -------------------------------------------------------------------- curl

fn quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out
}

fn curl() -> Command {
    let mut command = Command::new(if cfg!(windows) { "curl.exe" } else { "curl" });
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    command.args([
        "-q",
        "--proto",
        "=https",
        "--proto-redir",
        "=https",
        "--max-redirs",
        "5",
        "--connect-timeout",
        "10",
        "--silent",
        "--config",
        "-",
    ]);
    command
}

fn base_config(url: &str, accept: &str) -> String {
    format!(
        "url = \"{}\"\nheader = \"User-Agent: {USER_AGENT}\"\nheader = \"Accept: {}\"\nlocation\n",
        quote(url),
        quote(accept)
    )
}

/// GET a small response into memory. Returns the status and body.
fn get(url: &str, accept: &str, limit: usize) -> Result<(u16, String), String> {
    let mut config = base_config(url, accept);
    config.push_str("header = \"X-GitHub-Api-Version: 2022-11-28\"\n");
    config.push_str("write-out = \"\\n%{http_code}\"\n");
    let mut child = curl()
        .args(["--max-time", "25"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Could not start curl. {e}"))?;
    let mut stdin = child.stdin.take().expect("piped stdin");
    let written = stdin.write_all(config.as_bytes());
    drop(stdin);
    let mut data = Vec::new();
    let read = child
        .stdout
        .take()
        .expect("piped stdout")
        .take((limit + 8) as u64)
        .read_to_end(&mut data);
    if data.len() > limit || read.is_err() {
        let _ = child.kill();
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    written.map_err(|e| format!("Could not send the request. {e}"))?;
    if data.len() > limit {
        return Err("The update server answer is too large.".into());
    }
    let text = String::from_utf8_lossy(&data);
    let (body, code) = text.rsplit_once('\n').unwrap_or(("", &text));
    let code: u16 = code.trim().parse().unwrap_or(0);
    if code == 0 {
        return Err(if status.success() {
            "The update server did not answer.".into()
        } else {
            "Could not reach the update server. Check your connection.".into()
        });
    }
    Ok((code, body.to_owned()))
}

/// Asks GitHub for the latest release. `Ok(None)` when it is not newer than
/// this build.
pub(crate) fn check_latest() -> Result<Option<Release>, String> {
    let (status, body) = get(LATEST_URL, "application/vnd.github+json", MAX_API_RESPONSE)?;
    match status {
        200 => {}
        404 => return Err(NO_RELEASE.into()),
        403 | 429 => return Err("The update server is busy. Try again later.".into()),
        other => return Err(format!("The update server answered HTTP {other}.")),
    }
    let release = parse_release(&body)?;
    Ok(is_newer(&release.version, &Version::current()).then_some(release))
}

fn safe_file_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains(['/', '\\', ':', '\0'])
        && Path::new(name).file_name().is_some_and(|f| f == name)
}

fn fresh_download_dir() -> Result<PathBuf, String> {
    let root = std::env::temp_dir().join("covalos-update");
    // Older downloads are of no use. Locked files are left alone.
    let _ = fs::remove_dir_all(&root);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let dir = root.join(format!("{}-{nanos}", std::process::id()));
    fs::create_dir_all(&dir).map_err(|e| format!("Could not prepare the download folder. {e}"))?;
    Ok(dir)
}

/// Downloads the installer for `platform`, reporting bytes received, then
/// verifies the checksum when the release has a sidecar. Blocking.
pub(crate) fn download(
    release: &Release,
    platform: Platform,
    appimage: bool,
    progress: impl Fn(u64),
) -> Result<PathBuf, String> {
    let asset =
        select_asset(platform, &release.assets, appimage).ok_or("No installer for this system.")?;
    if !safe_file_name(&asset.name) {
        return Err("The installer name is not valid.".into());
    }
    let dir = fresh_download_dir()?;
    let path = dir.join(&asset.name);
    let result = download_to(release, asset, &path, &progress);
    if result.is_err() {
        let _ = fs::remove_file(&path);
    }
    result.map(|()| path)
}

fn download_to(
    release: &Release,
    asset: &Asset,
    path: &Path,
    progress: &impl Fn(u64),
) -> Result<(), String> {
    let mut config = base_config(&asset.url, "application/octet-stream");
    config.push_str("fail\n");
    config.push_str(&format!(
        "output = \"{}\"\n",
        quote(&path.to_string_lossy())
    ));
    let mut child = curl()
        .args(["--max-time", MAX_DOWNLOAD_SECONDS])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Could not start curl. {e}"))?;
    let mut stdin = child.stdin.take().expect("piped stdin");
    let written = stdin.write_all(config.as_bytes());
    drop(stdin);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if let Ok(meta) = fs::metadata(path) {
                    progress(meta.len());
                }
                thread::sleep(POLL);
            }
            Err(e) => return Err(format!("The download stopped. {e}")),
        }
    };
    written.map_err(|e| format!("Could not start the download. {e}"))?;
    if !status.success() {
        return Err("The download failed. Check your connection and try again.".into());
    }
    let size = fs::metadata(path).map_or(0, |m| m.len());
    if size == 0 || (asset.size > 0 && size != asset.size) {
        return Err("The download is incomplete.".into());
    }
    progress(size);
    if let Some(sidecar) = checksum_asset(&asset.name, &release.assets) {
        let (status, text) = get(&sidecar.url, "application/octet-stream", 4096)?;
        if status != 200 {
            return Err("Could not fetch the installer checksum.".into());
        }
        let expected =
            parse_checksum(&text, &asset.name).ok_or("The installer checksum is not valid.")?;
        verify_sha256(path, &expected)?;
    }
    Ok(())
}

// ---------------------------------------------------------- install, restart

/// True when [`restart_into`] replaces this process, so the caller must exit.
/// False when it only opens the installer and the app keeps running.
pub(crate) fn restart_exits(installer: &Path) -> bool {
    let name = installer
        .file_name()
        .map(|n| n.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    (cfg!(windows) && name.ends_with(".exe"))
        || (cfg!(target_os = "linux") && name.ends_with(".appimage"))
}

/// The cmd.exe line that installs silently, waits, then starts the app again.
/// `/S` makes cmd strip exactly the outer quotes.
#[cfg(any(windows, test))]
fn windows_helper_line(installer: &Path, exe: &Path) -> Result<String, String> {
    let (installer, exe) = (installer.to_string_lossy(), exe.to_string_lossy());
    for path in [&installer, &exe] {
        if path.contains(['"', '%', '\r', '\n']) {
            return Err("The installer path has characters that cannot be used.".into());
        }
    }
    Ok(format!(
        "/D /S /C \"ping -n 3 127.0.0.1 >nul & start \"\" /wait \"{installer}\" /VERYSILENT /SUPPRESSMSGBOXES /NORESTART /CLOSEAPPLICATIONS & start \"\" \"{exe}\"\""
    ))
}

#[cfg(windows)]
fn spawn_helper(line: &str, breakaway: bool) -> std::io::Result<()> {
    use std::os::windows::process::CommandExt;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
    let mut flags = CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP;
    if breakaway {
        flags |= CREATE_BREAKAWAY_FROM_JOB;
    }
    Command::new("cmd.exe")
        .raw_arg(line)
        .creation_flags(flags)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(drop)
}

#[cfg(windows)]
fn restart_impl(installer: &Path) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| format!("Could not find COVALOS. {e}"))?;
    let line = windows_helper_line(installer, &exe)?;
    // A job that forbids breakaway rejects the flag; start without it then.
    spawn_helper(&line, true)
        .or_else(|_| spawn_helper(&line, false))
        .map_err(|e| format!("Could not start the installer. {e}"))
}

#[cfg(target_os = "macos")]
fn restart_impl(installer: &Path) -> Result<(), String> {
    Command::new("open")
        .arg(installer)
        .spawn()
        .map(drop)
        .map_err(|e| format!("Could not open the installer. {e}"))
}

#[cfg(target_os = "linux")]
fn restart_impl(installer: &Path) -> Result<(), String> {
    use std::os::unix::{fs::PermissionsExt, process::CommandExt};
    let is_appimage = installer
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("appimage"));
    if !is_appimage {
        return Command::new("xdg-open")
            .arg(installer)
            .spawn()
            .map(drop)
            .map_err(|e| format!("Could not open the installer. {e}"));
    }
    let target = std::env::var_os("APPIMAGE")
        .map(PathBuf::from)
        .filter(|p| p.is_file())
        .ok_or("COVALOS is not running from an AppImage.")?;
    let dir = target.parent().ok_or("The AppImage has no folder.")?;
    // Copy next to the target so the rename stays on one filesystem.
    let staged = dir.join(".covalos-update.new");
    fs::copy(installer, &staged).map_err(|e| format!("Could not stage the update. {e}"))?;
    let finish = || -> std::io::Result<()> {
        fs::set_permissions(&staged, fs::Permissions::from_mode(0o755))?;
        fs::rename(&staged, &target)
    };
    if let Err(e) = finish() {
        let _ = fs::remove_file(&staged);
        return Err(format!("Could not replace the AppImage. {e}"));
    }
    Command::new(&target)
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(drop)
        .map_err(|e| format!("Could not start the new version. {e}"))
}

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
fn restart_impl(_installer: &Path) -> Result<(), String> {
    Err("Automatic updates are not available on this system.".into())
}

/// Starts the installer. When [`restart_exits`] is true the caller must now
/// exit the app cleanly so the helper can replace it and start it again.
pub(crate) fn restart_into(installer: &Path) -> Result<(), String> {
    if !installer.is_file() {
        return Err("The downloaded installer is gone. Check for updates again.".into());
    }
    restart_impl(installer)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(text: &str) -> Version {
        Version::parse(text).unwrap_or_else(|| panic!("{text} should parse"))
    }

    fn asset(name: &str) -> Asset {
        Asset {
            name: name.into(),
            url: format!("https://github.com/DevModee/Covalos/releases/download/v1/{name}"),
            size: 10,
        }
    }

    fn all_assets() -> Vec<Asset> {
        [
            "Covalos-Windows-x64-Setup.exe",
            "Covalos-Windows-x64-Setup.exe.sha256",
            "Covalos-macOS-arm64.dmg",
            "Covalos-macOS-arm64.dmg.sha256",
            "Covalos-macOS-x64.dmg",
            "Covalos-macOS-x64.dmg.sha256",
            "Covalos-Linux-x86_64.AppImage",
            "Covalos-Linux-x86_64.AppImage.sha256",
            "Covalos-Linux-amd64.deb",
            "Covalos-Linux-amd64.deb.sha256",
        ]
        .into_iter()
        .map(asset)
        .collect()
    }

    fn platform(os: &'static str, arch: &'static str) -> Platform {
        Platform { os, arch }
    }

    #[test]
    fn versions_order_like_semver() {
        assert!(v("0.8.9") < v("0.8.10"));
        assert!(v("0.8.10") < v("0.9.0"));
        assert!(v("0.9.0") < v("1.0.0"));
        assert!(v("1.0.0-beta") < v("1.0.0"));
        assert!(v("1.0.0-alpha") < v("1.0.0-beta"));
        assert!(v("1.0.0-beta.2") < v("1.0.0-beta.11"));
        assert!(v("1.0.0-1") < v("1.0.0-alpha"));
        assert!(v("1.0.0-beta") < v("1.0.0-beta.1"));
        assert_eq!(v("1.2.3+build5").cmp(&v("1.2.3")), Ordering::Equal);
        assert!(is_newer(&v("0.8.10"), &v("0.8.9")));
        assert!(!is_newer(&v("0.8.9"), &v("0.8.9")));
        assert!(!is_newer(&v("1.0.0-rc.1"), &v("1.0.0")));
    }

    #[test]
    fn version_text_accepts_a_v_prefix_and_rejects_garbage() {
        assert_eq!(v("v0.8.10"), v("0.8.10"));
        assert_eq!(v("V1.2.3").to_string(), "1.2.3");
        assert_eq!(v("1.0.0-beta.1").to_string(), "1.0.0-beta.1");
        for bad in [
            "",
            "v",
            "1",
            "1.2",
            "1.2.3.4",
            "a.b.c",
            "1.2.x",
            "1.2.3-",
            "1.2.3-a..b",
            "-1.2.3",
            "1.2.3 beta",
        ] {
            assert_eq!(Version::parse(bad), None, "{bad:?}");
        }
        assert!(Version::parse(env!("CARGO_PKG_VERSION")).is_some());
    }

    #[test]
    fn assets_are_picked_per_platform() {
        let assets = all_assets();
        let pick =
            |p: Platform, appimage| select_asset(p, &assets, appimage).map(|a| a.name.as_str());
        assert_eq!(
            pick(platform("windows", "x86_64"), false),
            Some("Covalos-Windows-x64-Setup.exe")
        );
        assert_eq!(
            pick(platform("windows", "aarch64"), false),
            Some("Covalos-Windows-x64-Setup.exe")
        );
        assert_eq!(
            pick(platform("macos", "aarch64"), false),
            Some("Covalos-macOS-arm64.dmg")
        );
        assert_eq!(
            pick(platform("macos", "x86_64"), false),
            Some("Covalos-macOS-x64.dmg")
        );
        assert_eq!(
            pick(platform("linux", "x86_64"), true),
            Some("Covalos-Linux-x86_64.AppImage")
        );
        assert_eq!(
            pick(platform("linux", "x86_64"), false),
            Some("Covalos-Linux-amd64.deb")
        );
        assert_eq!(pick(platform("freebsd", "x86_64"), false), None);
        assert_eq!(pick(platform("linux", "riscv64"), false), None);
    }

    #[test]
    fn a_missing_asset_selects_nothing() {
        let assets = vec![asset("Covalos-macOS-arm64.dmg")];
        assert!(select_asset(platform("windows", "x86_64"), &assets, false).is_none());
        assert!(select_asset(platform("linux", "x86_64"), &assets, true).is_none());
        assert!(select_asset(platform("windows", "x86_64"), &[], false).is_none());
    }

    #[test]
    fn checksum_sidecars_are_found_by_name() {
        let assets = all_assets();
        assert_eq!(
            checksum_asset("Covalos-macOS-x64.dmg", &assets).map(|a| a.name.as_str()),
            Some("Covalos-macOS-x64.dmg.sha256")
        );
        assert!(checksum_asset("Covalos-Other.zip", &assets).is_none());
        assert!(checksum_asset("Covalos-macOS-x64.dmg", &assets[..1]).is_none());
    }

    #[test]
    fn checksum_files_parse_in_every_common_format() {
        let hash = "ab".repeat(32);
        let upper = hash.to_ascii_uppercase();
        let name = "Covalos-Linux-amd64.deb";
        assert_eq!(
            parse_checksum(&format!("{hash}  {name}\n"), name),
            Some(hash.clone())
        );
        assert_eq!(
            parse_checksum(&format!("{hash} *{name}"), name),
            Some(hash.clone())
        );
        assert_eq!(
            parse_checksum(&format!("{hash}\n"), name),
            Some(hash.clone())
        );
        assert_eq!(
            parse_checksum(&format!("{upper}  dist\\{name}"), name),
            Some(hash.clone())
        );
        assert_eq!(parse_checksum(&format!("{hash}  other.deb"), name), None);
        assert_eq!(
            parse_checksum(&format!("{}  {name}", "ab".repeat(31)), name),
            None
        );
        assert_eq!(
            parse_checksum(&format!("{}  {name}", "zz".repeat(32)), name),
            None
        );
        assert_eq!(parse_checksum("", name), None);
        let two = format!("{} other\n{hash}  {name}\n", "cd".repeat(32));
        assert_eq!(parse_checksum(&two, name), Some(hash));
    }

    #[test]
    fn sha256_verification_accepts_a_match_and_rejects_a_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("file.bin");
        fs::write(&path, b"abc").unwrap();
        let known = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert_eq!(sha256_of(&path).unwrap(), known);
        assert!(verify_sha256(&path, known).is_ok());
        assert!(verify_sha256(&path, &format!(" {} \n", known.to_ascii_uppercase())).is_ok());
        assert!(verify_sha256(&path, &"0".repeat(64)).is_err());
        assert!(verify_sha256(&dir.path().join("missing"), known).is_err());
    }

    const SAMPLE: &str = r#"{
        "url": "https://api.github.com/repos/DevModee/Covalos/releases/1",
        "html_url": "https://github.com/DevModee/Covalos/releases/tag/v0.8.10",
        "tag_name": "v0.8.10",
        "name": "Covalos 0.8.10",
        "draft": false,
        "prerelease": false,
        "body": "Fixes the tab bar.\r\n- Faster start",
        "assets": [
            {"name": "Covalos-Windows-x64-Setup.exe", "size": 52428800, "browser_download_url": "https://github.com/DevModee/Covalos/releases/download/v0.8.10/Covalos-Windows-x64-Setup.exe"},
            {"name": "Covalos-Windows-x64-Setup.exe.sha256", "size": 100, "browser_download_url": "https://github.com/DevModee/Covalos/releases/download/v0.8.10/Covalos-Windows-x64-Setup.exe.sha256"},
            {"name": "Covalos-macOS-arm64.dmg", "size": 40000000, "browser_download_url": "https://github.com/DevModee/Covalos/releases/download/v0.8.10/Covalos-macOS-arm64.dmg"},
            {"name": "Covalos-macOS-x64.dmg", "size": 41000000, "browser_download_url": "https://github.com/DevModee/Covalos/releases/download/v0.8.10/Covalos-macOS-x64.dmg"},
            {"name": "Covalos-Linux-x86_64.AppImage", "size": 60000000, "browser_download_url": "https://github.com/DevModee/Covalos/releases/download/v0.8.10/Covalos-Linux-x86_64.AppImage"},
            {"name": "Covalos-Linux-amd64.deb", "size": 30000000, "browser_download_url": "https://github.com/DevModee/Covalos/releases/download/v0.8.10/Covalos-Linux-amd64.deb"},
            {"name": "no-url", "size": 1},
            {"name": "plain-http", "size": 1, "browser_download_url": "http://example.com/x"}
        ]
    }"#;

    #[test]
    fn a_release_parses_from_the_api_json() {
        let release = parse_release(SAMPLE).unwrap();
        assert_eq!(release.tag, "v0.8.10");
        assert_eq!(release.version, v("0.8.10"));
        assert_eq!(
            release.html_url,
            "https://github.com/DevModee/Covalos/releases/tag/v0.8.10"
        );
        assert!(release.notes.starts_with("Fixes the tab bar."));
        assert_eq!(
            release.assets.len(),
            6,
            "assets without an https url are dropped"
        );
        let picked = select_asset(platform("windows", "x86_64"), &release.assets, false).unwrap();
        assert_eq!(picked.size, 52_428_800);
        assert!(checksum_asset(&picked.name, &release.assets).is_some());
    }

    #[test]
    fn release_parsing_rejects_bad_input_and_truncates_notes() {
        assert!(parse_release("not json").is_err());
        assert!(parse_release("{}").is_err());
        assert!(parse_release(r#"{"tag_name":"nightly"}"#).is_err());
        assert_eq!(
            parse_release(r#"{"tag_name":"v1.0.0","draft":true}"#).unwrap_err(),
            NO_RELEASE
        );
        assert_eq!(
            parse_release(r#"{"tag_name":"v1.0.0","prerelease":true}"#).unwrap_err(),
            NO_RELEASE
        );
        let long = format!(
            r#"{{"tag_name":"v1.0.0","body":"{}","html_url":"javascript:x"}}"#,
            "x".repeat(9000)
        );
        let release = parse_release(&long).unwrap();
        assert_eq!(release.notes.chars().count(), MAX_NOTES);
        assert_eq!(release.html_url, RELEASES_PAGE);
        assert!(release.assets.is_empty());
    }

    fn release(tag: &str) -> Release {
        Release {
            tag: tag.into(),
            version: v(tag),
            notes: String::new(),
            html_url: RELEASES_PAGE.into(),
            assets: Vec::new(),
        }
    }

    #[test]
    fn the_happy_path_goes_through_every_state() {
        let mut updates = Updates::default();
        assert_eq!(updates.state, UpdateState::Idle);
        assert!(updates.on_check_started());
        assert!(updates.is_busy());
        updates.on_checked(Ok(Some(release("v0.9.0"))), false);
        assert_eq!(updates.state, UpdateState::Available(release("v0.9.0")));
        assert!(updates.on_download_started(1000));
        updates.on_progress(250);
        updates.on_progress(100);
        assert_eq!(updates.fraction(), Some(0.25), "progress never goes back");
        updates.on_progress(5000);
        assert_eq!(updates.fraction(), Some(1.0), "progress stops at the total");
        updates.on_downloaded(Ok(PathBuf::from("setup.exe")));
        assert_eq!(
            updates.state,
            UpdateState::ReadyToRestart {
                release: release("v0.9.0"),
                installer: PathBuf::from("setup.exe")
            }
        );
        assert!(!updates.is_busy());
    }

    #[test]
    fn a_check_with_nothing_newer_is_up_to_date() {
        let mut updates = Updates::default();
        updates.on_check_started();
        updates.on_checked(Ok(None), true);
        assert_eq!(updates.state, UpdateState::UpToDate);
        assert!(updates.on_check_started(), "can check again");
    }

    #[test]
    fn errors_are_quiet_unless_the_check_was_manual() {
        let mut updates = Updates::default();
        updates.on_check_started();
        updates.on_checked(Err(NO_RELEASE.into()), false);
        assert_eq!(updates.state, UpdateState::Idle);
        updates.on_check_started();
        updates.on_checked(Err(NO_RELEASE.into()), true);
        assert_eq!(updates.state, UpdateState::Error(NO_RELEASE.into()));
        assert!(updates.dismiss());
        assert_eq!(updates.state, UpdateState::Idle);
    }

    #[test]
    fn download_and_install_failures_become_errors() {
        let mut updates = Updates::default();
        updates.on_check_started();
        updates.on_checked(Ok(Some(release("v0.9.0"))), true);
        updates.on_download_started(10);
        updates.on_downloaded(Err("did not match".into()));
        assert_eq!(updates.state, UpdateState::Error("did not match".into()));
        assert!(updates.on_error("again".into()));
        assert_eq!(updates.state, UpdateState::Error("again".into()));
        assert!(updates.on_check_started(), "an error can be retried");
    }

    #[test]
    fn invalid_transitions_are_ignored() {
        let mut updates = Updates::default();
        updates.on_progress(50);
        updates.on_checked(Ok(Some(release("v1.0.0"))), true);
        updates.on_downloaded(Ok(PathBuf::from("x")));
        assert!(!updates.on_download_started(10));
        assert!(!updates.dismiss());
        assert!(!updates.on_error("x".into()));
        assert_eq!(updates.state, UpdateState::Idle);

        updates.on_check_started();
        assert!(!updates.on_check_started(), "one check at a time");
        assert!(!updates.on_download_started(10));
        updates.on_checked(Ok(Some(release("v1.0.0"))), true);
        updates.on_download_started(10);
        assert!(!updates.on_check_started(), "no check while downloading");
        updates.on_checked(Ok(None), true);
        assert!(matches!(updates.state, UpdateState::Downloading { .. }));
        updates.on_downloaded(Ok(PathBuf::from("x")));
        updates.on_progress(1);
        assert!(matches!(updates.state, UpdateState::ReadyToRestart { .. }));
    }

    #[test]
    fn a_dismissed_version_is_not_announced_again() {
        let mut updates = Updates::default();
        updates.on_check_started();
        updates.on_checked(Ok(Some(release("v0.9.0"))), false);
        assert!(updates.dismiss());
        assert_eq!(updates.dismissed(), Some("v0.9.0"));
        updates.on_check_started();
        updates.on_checked(Ok(Some(release("v0.9.0"))), false);
        assert_eq!(updates.state, UpdateState::Idle);
        updates.on_check_started();
        updates.on_checked(Ok(Some(release("v0.9.1"))), false);
        assert_eq!(updates.state, UpdateState::Available(release("v0.9.1")));
        // Asking by hand always shows the result.
        updates.set_dismissed(Some("v0.9.1".into()));
        updates.on_check_started();
        updates.on_checked(Ok(Some(release("v0.9.1"))), true);
        assert_eq!(updates.state, UpdateState::Available(release("v0.9.1")));
        assert_eq!(updates.dismissed(), None);
    }

    #[test]
    fn file_names_from_the_server_cannot_escape_the_download_folder() {
        assert!(safe_file_name("Covalos-Windows-x64-Setup.exe"));
        for bad in ["", "..", "../x.exe", "a/b", "a\\b", "C:x", "x\0"] {
            assert!(!safe_file_name(bad), "{bad:?}");
        }
    }

    #[test]
    fn the_windows_helper_installs_silently_then_starts_the_app() {
        let line = windows_helper_line(
            Path::new(r"C:\Temp\covalos-update\Covalos-Windows-x64-Setup.exe"),
            Path::new(r"C:\Users\Ana Maria\AppData\Local\Covalos\covalos.exe"),
        )
        .unwrap();
        assert!(line.starts_with("/D /S /C \"") && line.ends_with('"'));
        assert!(line.contains(
            r#"start "" /wait "C:\Temp\covalos-update\Covalos-Windows-x64-Setup.exe" /VERYSILENT /SUPPRESSMSGBOXES /NORESTART /CLOSEAPPLICATIONS"#
        ));
        assert!(
            line.contains(r#"& start "" "C:\Users\Ana Maria\AppData\Local\Covalos\covalos.exe""#)
        );
        assert!(windows_helper_line(Path::new("C:\\a%PATH%.exe"), Path::new("C:\\b.exe")).is_err());
        assert!(windows_helper_line(Path::new("C:\\a\".exe"), Path::new("C:\\b.exe")).is_err());
    }

    #[test]
    fn only_in_place_installers_close_the_app() {
        assert_eq!(
            restart_exits(Path::new("Covalos-Windows-x64-Setup.exe")),
            cfg!(windows)
        );
        assert!(!restart_exits(Path::new("Covalos-macOS-arm64.dmg")));
        assert!(!restart_exits(Path::new("Covalos-Linux-amd64.deb")));
        assert_eq!(
            restart_exits(Path::new("Covalos-Linux-x86_64.AppImage")),
            cfg!(target_os = "linux")
        );
        assert!(restart_into(Path::new("does-not-exist.exe")).is_err());
    }
}
