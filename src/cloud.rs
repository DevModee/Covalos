//! COVALOS cloud account on Supabase: endpoint configuration, PKCE browser
//! sign-in, token refresh and small REST calls.
//!
//! - HTTP goes through curl with the whole request (URL, headers, body) on
//!   stdin, so tokens never show up in process arguments.
//! - The refresh token lives only in the OS credential store. Access tokens
//!   stay in memory.
//! - The endpoint can be the hosted project or a local server, so the same
//!   build works against both.
use covalos_agents::credentials::{CredentialKey, CredentialStore, Secret};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    thread,
    time::{Duration, Instant},
};

pub(crate) const URL_ENV: &str = "COVALOS_SUPABASE_URL";
pub(crate) const ANON_KEY_ENV: &str = "COVALOS_SUPABASE_ANON_KEY";
const MAX_RESPONSE: usize = 1024 * 1024;
const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(300);

/// Where the account lives. The anon key is public by design; row level
/// security on the server decides what each user can read or write.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Endpoint {
    pub url: String,
    pub anon_key: String,
    /// "settings" or "environment", shown in Settings.
    pub source: &'static str,
}

impl Endpoint {
    /// Settings win when filled in; otherwise the environment variables.
    pub(crate) fn resolve(url: &str, anon_key: &str) -> Result<Self, String> {
        if !url.trim().is_empty() {
            return Self::from_parts(url, anon_key, "settings");
        }
        let url = std::env::var(URL_ENV).unwrap_or_default();
        let key = std::env::var(ANON_KEY_ENV).unwrap_or_default();
        if url.trim().is_empty() {
            return Err(format!(
                "Set {URL_ENV} and {ANON_KEY_ENV}, or enter a server in Settings."
            ));
        }
        Self::from_parts(&url, &key, "environment")
    }

    fn from_parts(url: &str, anon_key: &str, source: &'static str) -> Result<Self, String> {
        let url = url.trim().trim_end_matches('/').to_owned();
        let anon_key = anon_key.trim().to_owned();
        let host = host_of(&url).ok_or("The server address must start with https://.")?;
        let local = is_local(host);
        if !(url.starts_with("https://") || (local && url.starts_with("http://"))) {
            return Err("Use https://, or http:// only for a server on this computer.".into());
        }
        if url[url.find("://").map_or(0, |i| i + 3)..].contains(['?', '#', ' ', '"']) {
            return Err("The server address cannot contain a query or spaces.".into());
        }
        if anon_key.is_empty() || !anon_key.chars().all(|c| c.is_ascii_graphic()) {
            return Err("Enter the project's public anon key.".into());
        }
        Ok(Self {
            url,
            anon_key,
            source,
        })
    }

    pub(crate) fn host(&self) -> &str {
        host_of(&self.url).unwrap_or_default()
    }

    pub(crate) fn realtime_url(&self) -> String {
        let ws = if self.url.starts_with("https://") {
            self.url.replacen("https://", "wss://", 1)
        } else {
            self.url.replacen("http://", "ws://", 1)
        };
        format!(
            "{ws}/realtime/v1/websocket?apikey={}&vsn=1.0.0",
            encode(&self.anon_key)
        )
    }

    /// One refresh token per server, so a local server and the hosted
    /// project never overwrite each other.
    pub(crate) fn credential_key(&self) -> CredentialKey {
        CredentialKey {
            service: "covalos".into(),
            account: format!("cloud/{}/refresh-token", self.host()),
        }
    }
}

fn host_of(url: &str) -> Option<&str> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let authority = rest.split('/').next()?;
    let host = authority
        .rsplit_once(':')
        .filter(|(_, port)| port.chars().all(|c| c.is_ascii_digit()))
        .map_or(authority, |(host, _)| host);
    (!host.is_empty()).then_some(host)
}

fn is_local(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "[::1]") || host.ends_with(".localhost")
}

/// A signed-in account. Debug output never shows the tokens.
#[derive(Debug)]
pub(crate) struct Session {
    pub user_id: String,
    pub email: Option<String>,
    pub access: Secret,
    pub refresh: Secret,
    pub expires_at_ms: u64,
}

impl Session {
    /// Refresh a minute early so requests never race the expiry.
    pub(crate) fn needs_refresh(&self, now_ms: u64) -> bool {
        now_ms + 60_000 >= self.expires_at_ms
    }
}

fn parse_session(body: &str) -> Result<Session, String> {
    let value: Value =
        serde_json::from_str(body).map_err(|_| "The server sent an unreadable session.")?;
    let text = |v: &Value| v.as_str().filter(|s| !s.is_empty()).map(str::to_owned);
    let access = text(&value["access_token"]).ok_or("The server sent no access token.")?;
    let refresh = text(&value["refresh_token"]).ok_or("The server sent no refresh token.")?;
    let user_id = text(&value["user"]["id"]).ok_or("The server sent no user id.")?;
    if !user_id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err("The server sent an invalid user id.".into());
    }
    let expires_in = value["expires_in"]
        .as_u64()
        .unwrap_or(3600)
        .clamp(60, 86_400);
    Ok(Session {
        user_id,
        email: text(&value["user"]["email"]),
        access: Secret::new(access),
        refresh: Secret::new(refresh),
        expires_at_ms: now_ms() + expires_in * 1000,
    })
}

/// The credential store used for the refresh token. Outside Windows the
/// token stays in memory, so the account signs out when COVALOS closes.
pub(crate) fn store() -> Box<dyn CredentialStore + Send> {
    #[cfg(windows)]
    {
        Box::new(covalos_agents::credentials_windows::WindowsCredentialStore::default())
    }
    #[cfg(not(windows))]
    {
        Box::new(covalos_agents::credentials::MemoryStore::default())
    }
}

pub(crate) fn store_is_persistent() -> bool {
    cfg!(windows)
}

/// PKCE verifier (43 URL-safe characters) and its S256 challenge.
pub(crate) fn pkce_pair() -> Result<(String, String), String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|_| "Could not create a sign-in code.")?;
    let verifier = base64_url(&bytes);
    let challenge = challenge_for(&verifier);
    Ok((verifier, challenge))
}

fn challenge_for(verifier: &str) -> String {
    use sha2::Digest;
    base64_url(&sha2::Sha256::digest(verifier.as_bytes()))
}

fn base64_url(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

pub(crate) fn authorize_url(
    endpoint: &Endpoint,
    provider: &str,
    redirect: &str,
    challenge: &str,
) -> String {
    format!(
        "{}/auth/v1/authorize?provider={}&redirect_to={}&code_challenge={}&code_challenge_method=s256",
        endpoint.url,
        encode(provider),
        encode(redirect),
        encode(challenge)
    )
}

/// A browser sign-in in progress. Dropping it stops the local listener.
pub(crate) struct SignIn {
    pub url: String,
    pub result: Receiver<Result<Session, String>>,
    cancel: Arc<AtomicBool>,
}

impl Drop for SignIn {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

/// Starts a loopback listener for the OAuth redirect and returns the URL to
/// open in the browser. The code is exchanged with the PKCE verifier, so a
/// code delivered to the listener by anything else is useless.
pub(crate) fn begin_sign_in(
    endpoint: &Endpoint,
    provider: &str,
    wake: impl Fn() + Send + 'static,
) -> Result<SignIn, String> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|e| format!("Could not open the sign-in listener. {e}"))?;
    listener
        .set_nonblocking(true)
        .map_err(|e| format!("Could not open the sign-in listener. {e}"))?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let redirect = format!("http://127.0.0.1:{port}/callback");
    let (verifier, challenge) = pkce_pair()?;
    let url = authorize_url(endpoint, provider, &redirect, &challenge);
    let cancel = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::channel();
    let stop = cancel.clone();
    let endpoint = endpoint.clone();
    thread::spawn(move || {
        let outcome = wait_for_code(&listener, &stop)
            .and_then(|code| exchange_code(&endpoint, &code, &verifier));
        if !stop.load(Ordering::Relaxed) {
            let _ = tx.send(outcome);
            wake();
        }
    });
    Ok(SignIn {
        url,
        result: rx,
        cancel,
    })
}

fn wait_for_code(listener: &TcpListener, stop: &AtomicBool) -> Result<String, String> {
    let started = Instant::now();
    while !stop.load(Ordering::Relaxed) {
        if started.elapsed() > SIGN_IN_TIMEOUT {
            return Err("Sign-in timed out. Try again.".into());
        }
        match listener.accept() {
            Ok((stream, _)) => {
                if let Some(result) = handle_callback(stream) {
                    return result;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(100));
            }
            Err(e) => return Err(format!("The sign-in listener stopped. {e}")),
        }
    }
    Err("Sign-in cancelled.".into())
}

/// Reads one request. `None` for anything that is not the callback (a
/// favicon request, say), so the listener keeps waiting.
fn handle_callback(mut stream: TcpStream) -> Option<Result<String, String>> {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut buffer = [0u8; 4096];
    let read = stream.read(&mut buffer).ok()?;
    let request = String::from_utf8_lossy(&buffer[..read]);
    let line = request.lines().next().unwrap_or_default();
    let result = parse_callback(line)?;
    let message = if result.is_ok() {
        "Signed in. You can close this tab and return to COVALOS."
    } else {
        "Sign-in did not finish. Return to COVALOS and try again."
    };
    let page = format!(
        "<!doctype html><meta charset=utf-8><title>COVALOS</title>\
         <body style=\"font-family:sans-serif;background:#0b0b0d;color:#e6e6e6;padding:48px\">\
         <p>{message}</p></body>"
    );
    let _ = write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n{page}",
        page.len()
    );
    Some(result)
}

fn parse_callback(request_line: &str) -> Option<Result<String, String>> {
    let target = request_line
        .strip_prefix("GET ")?
        .split(' ')
        .next()
        .unwrap_or_default();
    let query = target.strip_prefix("/callback")?;
    let query = query.strip_prefix('?').unwrap_or_default();
    let mut code = None;
    let mut error = None;
    for pair in query.split('&') {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        match key {
            "code" => code = Some(decode(value)),
            "error_description" => error = Some(decode(value)),
            "error" if error.is_none() => error = Some(decode(value)),
            _ => {}
        }
    }
    Some(match (code, error) {
        (Some(code), _) if !code.is_empty() && code.len() < 512 => Ok(code),
        (_, Some(error)) => Err(format!(
            "The server refused the sign-in: {}",
            error.chars().take(200).collect::<String>()
        )),
        _ => Err("The browser came back without a sign-in code.".into()),
    })
}

fn exchange_code(endpoint: &Endpoint, code: &str, verifier: &str) -> Result<Session, String> {
    let body = json!({"auth_code": code, "code_verifier": verifier}).to_string();
    let (status, text) = request(
        endpoint,
        None,
        "POST",
        "/auth/v1/token?grant_type=pkce",
        &[],
        Some(&body),
    )?;
    if status != 200 {
        return Err(server_error("Could not finish the sign-in", status, &text));
    }
    parse_session(&text)
}

/// Trades a refresh token for a new session. Supabase rotates refresh
/// tokens, so the caller must store the new one.
pub(crate) fn refresh(endpoint: &Endpoint, refresh_token: &str) -> Result<Session, String> {
    let body = json!({"refresh_token": refresh_token}).to_string();
    let (status, text) = request(
        endpoint,
        None,
        "POST",
        "/auth/v1/token?grant_type=refresh_token",
        &[],
        Some(&body),
    )?;
    if status != 200 {
        return Err(server_error("The session expired", status, &text));
    }
    parse_session(&text)
}

pub(crate) fn sign_out(endpoint: &Endpoint, access: &str) {
    let _ = request(endpoint, Some(access), "POST", "/auth/v1/logout", &[], None);
}

/// Short, token-free error for the UI.
pub(crate) fn server_error(what: &str, status: u16, body: &str) -> String {
    let value: Value = serde_json::from_str(body).unwrap_or_default();
    let detail = ["error_description", "msg", "message", "error"]
        .iter()
        .find_map(|k| value[*k].as_str())
        .map(|s| {
            crate::agent_gate::redact(s)
                .chars()
                .take(160)
                .collect::<String>()
        });
    match detail {
        Some(detail) => format!("{what} ({status}): {detail}"),
        None => format!("{what} (HTTP {status})."),
    }
}

/// One HTTP request to the endpoint. Returns the status and body.
pub(crate) fn request(
    endpoint: &Endpoint,
    access: Option<&str>,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: Option<&str>,
) -> Result<(u16, String), String> {
    debug_assert!(path.starts_with('/'));
    let mut config = String::new();
    let mut line = |key: &str, value: &str| {
        config.push_str(key);
        config.push_str(" = \"");
        config.push_str(&quote(value));
        config.push_str("\"\n");
    };
    line("url", &format!("{}{path}", endpoint.url));
    line("request", method);
    line("header", &format!("apikey: {}", endpoint.anon_key));
    if let Some(access) = access {
        line("header", &format!("Authorization: Bearer {access}"));
    }
    if body.is_some() {
        line("header", "Content-Type: application/json");
    }
    for (key, value) in headers {
        line("header", &format!("{key}: {value}"));
    }
    if let Some(body) = body {
        line("data-binary", body);
    }
    line("write-out", "\n%{http_code}");
    let protocols = if endpoint.url.starts_with("https://") {
        "=https"
    } else {
        "=http"
    };
    let mut command = Command::new(if cfg!(windows) { "curl.exe" } else { "curl" });
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command
        .args([
            "-q",
            "--proto",
            protocols,
            "--connect-timeout",
            "8",
            "--max-time",
            "25",
            "--silent",
            "--config",
            "-",
        ])
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
        .take((MAX_RESPONSE + 8) as u64)
        .read_to_end(&mut data);
    if data.len() > MAX_RESPONSE || read.is_err() {
        let _ = child.kill();
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    written.map_err(|e| format!("Could not send the request. {e}"))?;
    if data.len() > MAX_RESPONSE {
        return Err("The server response is too large.".into());
    }
    let text = String::from_utf8_lossy(&data);
    let (body, code) = text.rsplit_once('\n').unwrap_or(("", &text));
    let code: u16 = code.trim().parse().unwrap_or(0);
    if code == 0 {
        return Err(if status.success() {
            "The server did not answer.".into()
        } else {
            "Could not reach the server. Check the address and your connection.".into()
        });
    }
    Ok((code, body.to_owned()))
}

/// Escapes a value for a double-quoted curl config string.
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

pub(crate) fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = text
            .get(i + 1..i + 3)
            .and_then(|h| u8::from_str_radix(h, 16).ok());
        match (bytes[i], hex) {
            (b'+', _) => out.push(b' '),
            (b'%', Some(byte)) => {
                out.push(byte);
                i += 2;
            }
            (other, _) => out.push(other),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn endpoint() -> Endpoint {
        Endpoint::from_parts("https://abc.supabase.co/", "anon-key", "settings").unwrap()
    }

    #[test]
    fn endpoints_require_https_except_on_this_computer() {
        assert_eq!(endpoint().url, "https://abc.supabase.co");
        assert_eq!(endpoint().host(), "abc.supabase.co");
        assert!(Endpoint::from_parts("http://abc.supabase.co", "k", "settings").is_err());
        assert!(Endpoint::from_parts("https://abc.supabase.co?x=1", "k", "settings").is_err());
        assert!(Endpoint::from_parts("https://abc.supabase.co", "", "settings").is_err());
        let local = Endpoint::from_parts("http://127.0.0.1:54321", "k", "settings").unwrap();
        assert_eq!(local.host(), "127.0.0.1");
        assert_eq!(
            local.realtime_url(),
            "ws://127.0.0.1:54321/realtime/v1/websocket?apikey=k&vsn=1.0.0"
        );
        assert!(
            endpoint()
                .realtime_url()
                .starts_with("wss://abc.supabase.co/")
        );
        assert_ne!(
            endpoint().credential_key(),
            local.credential_key(),
            "each server keeps its own token"
        );
    }

    #[test]
    fn pkce_matches_the_rfc_7636_example() {
        assert_eq!(
            challenge_for("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
        let (verifier, challenge) = pkce_pair().unwrap();
        assert_eq!(verifier.len(), 43);
        assert_eq!(challenge_for(&verifier), challenge);
        let url = authorize_url(
            &endpoint(),
            "github",
            "http://127.0.0.1:5/callback",
            &challenge,
        );
        assert!(url.contains("redirect_to=http%3A%2F%2F127.0.0.1%3A5%2Fcallback"));
        assert!(url.ends_with("code_challenge_method=s256"));
    }

    #[test]
    fn callback_lines_yield_the_code_or_the_server_error() {
        assert_eq!(
            parse_callback("GET /callback?code=ab%2Dc&x=1 HTTP/1.1"),
            Some(Ok("ab-c".into()))
        );
        assert!(matches!(
            parse_callback(
                "GET /callback?error=access_denied&error_description=User+said+no HTTP/1.1"
            ),
            Some(Err(e)) if e.contains("User said no")
        ));
        assert_eq!(parse_callback("GET /favicon.ico HTTP/1.1"), None);
        assert_eq!(parse_callback("POST /callback?code=x HTTP/1.1"), None);
        assert_eq!(decode("100%"), "100%");
        assert_eq!(decode("%C3%A9%zz"), "é%zz");
    }

    #[test]
    fn sessions_parse_and_never_print_tokens() {
        let session = parse_session(
            r#"{"access_token":"at-1","refresh_token":"rt-1","expires_in":3600,
                "user":{"id":"8d0f-11aa","email":"me@example.com"}}"#,
        )
        .unwrap();
        assert_eq!(session.user_id, "8d0f-11aa");
        assert!(!session.needs_refresh(now_ms()));
        let printed = format!("{session:?}");
        assert!(!printed.contains("at-1") && !printed.contains("rt-1"));
        assert!(
            parse_session(r#"{"access_token":"a","refresh_token":"r","user":{"id":"x/y"}}"#)
                .is_err()
        );
    }

    #[test]
    fn curl_config_values_are_escaped() {
        assert_eq!(quote(r#"{"a":"b\c"}"#), r#"{\"a\":\"b\\c\"}"#);
        assert_eq!(quote("x\ny"), "x\\ny");
        assert_eq!(encode("a b/é"), "a%20b%2F%C3%A9");
        assert!(server_error("Failed", 400, r#"{"msg":"bad"}"#).contains("bad"));
    }
}
