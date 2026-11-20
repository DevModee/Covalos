//! Discord Rich Presence over the local IPC transport documented by Discord
//! (`discord-ipc-N` named pipe, framed JSON: little-endian opcode, length, payload).
//!
//! Off by default and privacy-first: nothing leaves the machine except to the local
//! Discord desktop client, and only coarse activity is shared unless the user opts in.
//! Discord requires an Application ID registered in the Developer Portal; COVALOS does
//! not ship one, so the user (or the project owner) enters it in Settings.
use std::sync::{
    Arc, Mutex,
    mpsc::{self, RecvTimeoutError, Sender},
};
use std::time::Duration;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub(super) struct DiscordPreferences {
    pub(super) enabled: bool,
    /// Application ID of the COVALOS app in the Discord Developer Portal.
    pub(super) application_id: String,
    pub(super) show_language: bool,
    /// Project names can be private; off by default.
    pub(super) show_project: bool,
    pub(super) show_activity: bool,
    pub(super) show_elapsed: bool,
}

impl Default for DiscordPreferences {
    fn default() -> Self {
        Self {
            enabled: false,
            application_id: String::new(),
            show_language: true,
            show_project: false,
            show_activity: true,
            show_elapsed: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Activity {
    pub(super) details: String,
    pub(super) state: Option<String>,
    pub(super) started: Option<u64>,
}

enum Message {
    Set(Activity),
}

pub(super) struct Presence {
    sender: Option<Sender<Message>>,
    client: String,
    last: Option<Activity>,
    status: Arc<Mutex<&'static str>>,
}

impl Default for Presence {
    fn default() -> Self {
        Self {
            sender: None,
            client: String::new(),
            last: None,
            status: Arc::new(Mutex::new("Off")),
        }
    }
}

impl Presence {
    /// Called every frame; only sends when the activity actually changes.
    pub(super) fn update(&mut self, prefs: &DiscordPreferences, activity: Activity) {
        let id = prefs.application_id.trim();
        let valid = !id.is_empty() && id.chars().all(|c| c.is_ascii_digit());
        if !prefs.enabled || !valid {
            if self.sender.take().is_some() {
                self.last = None;
            }
            *self.status.lock().unwrap_or_else(|e| e.into_inner()) = if !prefs.enabled {
                "Off"
            } else {
                "Enter a valid Application ID"
            };
            return;
        }
        if self.sender.is_none() || self.client != id {
            // Dropping the old sender clears the old presence and stops its thread.
            self.sender = Some(spawn(id.to_owned(), self.status.clone()));
            self.client = id.to_owned();
            self.last = None;
        }
        if self.last.as_ref() != Some(&activity)
            && let Some(sender) = &self.sender
            && sender.send(Message::Set(activity.clone())).is_ok()
        {
            self.last = Some(activity);
        }
    }

    pub(super) fn status(&self) -> &'static str {
        *self.status.lock().unwrap_or_else(|e| e.into_inner())
    }
}

fn spawn(client: String, status: Arc<Mutex<&'static str>>) -> Sender<Message> {
    let (sender, receiver) = mpsc::channel::<Message>();
    std::thread::spawn(move || {
        let set = |s: &'static str| *status.lock().unwrap_or_else(|e| e.into_inner()) = s;
        let mut desired: Option<Activity> = None;
        let mut pipe: Option<ipc::Pipe> = None;
        let mut dirty = false;
        loop {
            match receiver.recv_timeout(Duration::from_secs(10)) {
                Ok(Message::Set(activity)) => {
                    desired = Some(activity);
                    dirty = true;
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    if let Some(p) = pipe.as_mut() {
                        let _ = p.set_activity(None);
                    }
                    set("Off");
                    return;
                }
            }
            if pipe.is_none() {
                match ipc::Pipe::connect(&client) {
                    Ok(p) => {
                        pipe = Some(p);
                        dirty = true;
                    }
                    Err(_) => {
                        set("Discord is not running");
                        continue;
                    }
                }
            }
            if dirty && let Some(p) = pipe.as_mut() {
                match p.set_activity(desired.as_ref()) {
                    Ok(()) => {
                        dirty = false;
                        set("Connected");
                    }
                    Err(_) => {
                        pipe = None;
                        set("Discord is not running");
                    }
                }
            }
        }
    });
    sender
}

/// Frame encoding shared by every platform transport.
fn frame(op: u32, payload: &serde_json::Value) -> Vec<u8> {
    let body = payload.to_string().into_bytes();
    let mut out = Vec::with_capacity(body.len() + 8);
    out.extend_from_slice(&op.to_le_bytes());
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    out
}

fn activity_payload(activity: Option<&Activity>, nonce: u64) -> serde_json::Value {
    let activity = activity.map(|a| {
        let mut value = serde_json::json!({
            "details": a.details,
            "assets": { "large_image": "covalos", "large_text": "Covalos" },
        });
        if let Some(state) = &a.state {
            value["state"] = serde_json::json!(state);
        }
        if let Some(start) = a.started {
            value["timestamps"] = serde_json::json!({ "start": start });
        }
        value
    });
    serde_json::json!({
        "cmd": "SET_ACTIVITY",
        "args": { "pid": std::process::id(), "activity": activity },
        "nonce": nonce.to_string(),
    })
}

#[cfg(windows)]
mod ipc {
    use std::io::{Read, Write};

    pub(super) struct Pipe {
        file: std::fs::File,
        nonce: u64,
    }

    impl Pipe {
        pub(super) fn connect(client: &str) -> std::io::Result<Self> {
            let mut last = std::io::Error::other("no Discord pipe");
            for n in 0..10 {
                let path = format!(r"\\.\pipe\discord-ipc-{n}");
                match std::fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(&path)
                {
                    Ok(file) => {
                        let mut pipe = Self { file, nonce: 0 };
                        pipe.write(0, &serde_json::json!({ "v": 1, "client_id": client }))?;
                        pipe.read()?;
                        return Ok(pipe);
                    }
                    Err(error) => last = error,
                }
            }
            Err(last)
        }

        fn write(&mut self, op: u32, payload: &serde_json::Value) -> std::io::Result<()> {
            self.file.write_all(&super::frame(op, payload))
        }

        fn read(&mut self) -> std::io::Result<serde_json::Value> {
            let mut header = [0_u8; 8];
            self.file.read_exact(&mut header)?;
            let len = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
            if len > 1 << 20 {
                return Err(std::io::Error::other("oversized Discord frame"));
            }
            let mut body = vec![0; len];
            self.file.read_exact(&mut body)?;
            serde_json::from_slice(&body).map_err(std::io::Error::other)
        }

        pub(super) fn set_activity(
            &mut self,
            activity: Option<&super::Activity>,
        ) -> std::io::Result<()> {
            self.nonce += 1;
            self.write(1, &super::activity_payload(activity, self.nonce))?;
            let reply = self.read()?;
            if reply.get("evt").and_then(|e| e.as_str()) == Some("ERROR") {
                return Err(std::io::Error::other(reply.to_string()));
            }
            Ok(())
        }
    }
}

#[cfg(not(windows))]
mod ipc {
    pub(super) struct Pipe;
    impl Pipe {
        pub(super) fn connect(_client: &str) -> std::io::Result<Self> {
            Err(std::io::Error::other(
                "Rich Presence is implemented for Windows only",
            ))
        }
        pub(super) fn set_activity(
            &mut self,
            _activity: Option<&super::Activity>,
        ) -> std::io::Result<()> {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_and_payloads_follow_the_ipc_format_and_stay_private() {
        let payload = activity_payload(
            Some(&Activity {
                details: "Developing with Covalos".into(),
                state: Some("Editing Rust".into()),
                started: Some(1_700_000_000),
            }),
            7,
        );
        let bytes = frame(1, &payload);
        assert_eq!(u32::from_le_bytes(bytes[0..4].try_into().unwrap()), 1);
        assert_eq!(
            u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize,
            bytes.len() - 8
        );
        let text = payload.to_string();
        assert!(text.contains("SET_ACTIVITY") && text.contains("Editing Rust"));
        assert!(activity_payload(None, 8)["args"]["activity"].is_null());
        // Disabled or missing ID never starts a connection.
        let mut presence = Presence::default();
        let activity = Activity {
            details: "x".into(),
            state: None,
            started: None,
        };
        presence.update(&DiscordPreferences::default(), activity.clone());
        assert_eq!(presence.status(), "Off");
        let prefs = DiscordPreferences {
            enabled: true,
            application_id: "not-an-id".into(),
            ..Default::default()
        };
        presence.update(&prefs, activity);
        assert!(presence.sender.is_none());
    }
}
