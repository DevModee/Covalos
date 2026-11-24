//! Remote bridge between this computer and the COVALOS phone app.
//!
//! While the user is signed in and has opted in, a background thread keeps a
//! Supabase Realtime connection (Phoenix channels over WebSocket) on the
//! private channel `covalos-remote:<user id>`. Row level security on
//! `realtime.messages` limits that channel to the same account.
//!
//! The thread only moves messages. It publishes the status summary the
//! editor gives it and hands incoming commands back to the editor, which
//! runs them through the same checks as local input (Luma's effort ceiling,
//! the agent gate's approval rules). Nothing here executes anything.
use serde_json::{Value, json};
use std::{
    net::TcpStream,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender, TryRecvError},
    },
    thread,
    time::{Duration, Instant},
};
use tungstenite::{Message, WebSocket, stream::MaybeTlsStream};

use crate::cloud::Endpoint;

const HEARTBEAT: Duration = Duration::from_secs(25);
const READ_TICK: Duration = Duration::from_millis(250);
const MAX_TEXT: usize = 8_000;
/// Commands older than this are ignored, so nothing queued while the PC
/// slept runs by surprise.
const MAX_COMMAND_AGE_MS: u64 = 120_000;

/// A command from the phone, already shape-checked. The editor still decides
/// whether it may run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Command {
    /// An order for Luma, same as typing it in Luma's box.
    LumaOrder { text: String },
    /// A message for one agent conversation.
    ChatPrompt { session: String, text: String },
    /// Answer to a pending `--agent-gate` request. Never remembered.
    Approval { ticket: String, allow: bool },
    /// Ask for a fresh status summary.
    Refresh,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Incoming {
    pub id: String,
    pub command: Command,
}

#[derive(Debug)]
pub(crate) enum Event {
    Connected,
    Disconnected(String),
    Command(Incoming),
}

enum Outgoing {
    State(Value),
    Reply {
        id: String,
        ok: bool,
        message: String,
    },
    Token(String),
}

/// Handle to the background connection. Dropping it closes the connection.
pub(crate) struct Bridge {
    tx: Sender<Outgoing>,
    rx: Receiver<Event>,
    stop: Arc<AtomicBool>,
}

impl Drop for Bridge {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl Bridge {
    pub(crate) fn start(
        endpoint: Endpoint,
        user_id: &str,
        access_token: String,
        wake: impl Fn() + Send + 'static,
    ) -> Self {
        let (tx, out_rx) = mpsc::channel();
        let (event_tx, rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let mut link = Link {
            endpoint,
            topic: topic(user_id),
            token: access_token,
            out: out_rx,
            events: event_tx,
            stop: stop.clone(),
            wake: Box::new(wake),
            serial: 0,
            state: None,
        };
        thread::spawn(move || link.run());
        Self { tx, rx, stop }
    }

    pub(crate) fn publish(&self, state: Value) {
        let _ = self.tx.send(Outgoing::State(state));
    }

    pub(crate) fn reply(&self, id: &str, ok: bool, message: impl Into<String>) {
        let _ = self.tx.send(Outgoing::Reply {
            id: id.to_owned(),
            ok,
            message: message.into(),
        });
    }

    /// Hands a refreshed access token to the open channel.
    pub(crate) fn update_token(&self, token: &str) {
        let _ = self.tx.send(Outgoing::Token(token.to_owned()));
    }

    pub(crate) fn events(&self) -> Vec<Event> {
        let mut found = Vec::new();
        while let Ok(event) = self.rx.try_recv() {
            found.push(event);
        }
        found
    }
}

pub(crate) fn topic(user_id: &str) -> String {
    format!("realtime:covalos-remote:{user_id}")
}

type Socket = WebSocket<MaybeTlsStream<TcpStream>>;

struct Link {
    endpoint: Endpoint,
    topic: String,
    token: String,
    out: Receiver<Outgoing>,
    events: Sender<Event>,
    stop: Arc<AtomicBool>,
    wake: Box<dyn Fn() + Send>,
    serial: u64,
    /// Last summary, resent after a reconnect.
    state: Option<Value>,
}

impl Link {
    fn run(&mut self) {
        let mut backoff = Duration::from_secs(2);
        while !self.stopped() {
            let started = Instant::now();
            let reason = match self.session() {
                Ok(()) => return,
                Err(reason) => reason,
            };
            self.emit(Event::Disconnected(reason));
            if started.elapsed() > Duration::from_secs(60) {
                backoff = Duration::from_secs(2);
            }
            let until = Instant::now() + backoff;
            while Instant::now() < until {
                if self.stopped() {
                    return;
                }
                thread::sleep(Duration::from_millis(200));
            }
            backoff = (backoff * 2).min(Duration::from_secs(60));
        }
    }

    fn stopped(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }

    fn emit(&self, event: Event) {
        let _ = self.events.send(event);
        (self.wake)();
    }

    fn next_ref(&mut self) -> String {
        self.serial += 1;
        self.serial.to_string()
    }

    fn broadcast(&mut self, socket: &mut Socket, event: &str, payload: Value) -> Result<(), String> {
        let reference = self.next_ref();
        send(
            socket,
            broadcast_message(&self.topic, event, payload, &reference),
        )
    }

    /// One connection, until it drops (`Err`) or the bridge stops (`Ok`).
    fn session(&mut self) -> Result<(), String> {
        let (mut socket, _) = tungstenite::connect(self.endpoint.realtime_url())
            .map_err(|_| "Could not reach the realtime server.".to_owned())?;
        set_read_timeout(&socket, READ_TICK);
        let join_ref = self.next_ref();
        send(
            &mut socket,
            join_message(&self.topic, &self.token, &join_ref),
        )?;
        let mut joined = false;
        let mut last_beat = Instant::now();
        loop {
            if self.stopped() {
                let _ = socket.close(None);
                return Ok(());
            }
            match socket.read() {
                Ok(Message::Text(text)) => match parse_inbound(&text, &self.topic) {
                    Some(Inbound::Joined { reference }) if reference == join_ref => {
                        joined = true;
                        self.emit(Event::Connected);
                        if let Some(state) = self.state.clone() {
                            self.broadcast(&mut socket, "state", state)?;
                        }
                    }
                    Some(Inbound::Refused { reason }) => return Err(reason),
                    Some(Inbound::Broadcast { event, payload }) if event == "command" => {
                        match parse_command(&payload, crate::cloud::now_ms()) {
                            Ok(incoming) => self.emit(Event::Command(incoming)),
                            Err((id, message)) if !id.is_empty() => {
                                let result = json!({"id": id, "ok": false, "message": message});
                                self.broadcast(&mut socket, "result", result)?;
                            }
                            Err(_) => {}
                        }
                    }
                    Some(Inbound::Closed) => return Err("The realtime channel closed.".into()),
                    _ => {}
                },
                Ok(Message::Close(_)) => {
                    return Err("The realtime server closed the link.".into());
                }
                Ok(_) => {}
                Err(tungstenite::Error::Io(e))
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(_) => return Err("The realtime connection dropped.".into()),
            }
            loop {
                let outgoing = match self.out.try_recv() {
                    Ok(outgoing) => outgoing,
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        let _ = socket.close(None);
                        return Ok(());
                    }
                };
                match outgoing {
                    Outgoing::State(state) => {
                        self.state = Some(state.clone());
                        if joined {
                            self.broadcast(&mut socket, "state", state)?;
                        }
                    }
                    Outgoing::Reply { id, ok, message } => {
                        let result = json!({"id": id, "ok": ok, "message": message});
                        self.broadcast(&mut socket, "result", result)?;
                    }
                    Outgoing::Token(token) => {
                        self.token = token;
                        let reference = self.next_ref();
                        send(
                            &mut socket,
                            access_token_message(&self.topic, &self.token, &reference),
                        )?;
                    }
                }
            }
            if last_beat.elapsed() >= HEARTBEAT {
                last_beat = Instant::now();
                let reference = self.next_ref();
                send(&mut socket, heartbeat_message(&reference))?;
            }
        }
    }
}

fn set_read_timeout(socket: &Socket, timeout: Duration) {
    match socket.get_ref() {
        MaybeTlsStream::Plain(stream) => {
            let _ = stream.set_read_timeout(Some(timeout));
        }
        MaybeTlsStream::NativeTls(stream) => {
            let _ = stream.get_ref().set_read_timeout(Some(timeout));
        }
        _ => {}
    }
}

fn send(socket: &mut Socket, message: Value) -> Result<(), String> {
    socket
        .send(Message::Text(message.to_string().into()))
        .map_err(|_| "The realtime connection dropped.".to_owned())
}

fn join_message(topic: &str, token: &str, reference: &str) -> Value {
    json!({
        "topic": topic,
        "event": "phx_join",
        "payload": {
            "config": {
                "broadcast": {"self": false, "ack": false},
                "presence": {"key": ""},
                "private": true
            },
            "access_token": token
        },
        "ref": reference,
        "join_ref": reference
    })
}

fn broadcast_message(topic: &str, event: &str, payload: Value, reference: &str) -> Value {
    json!({
        "topic": topic,
        "event": "broadcast",
        "payload": {"type": "broadcast", "event": event, "payload": payload},
        "ref": reference
    })
}

fn access_token_message(topic: &str, token: &str, reference: &str) -> Value {
    json!({
        "topic": topic,
        "event": "access_token",
        "payload": {"access_token": token},
        "ref": reference
    })
}

fn heartbeat_message(reference: &str) -> Value {
    json!({"topic": "phoenix", "event": "heartbeat", "payload": {}, "ref": reference})
}

#[derive(Debug, PartialEq)]
enum Inbound {
    Joined { reference: String },
    Refused { reason: String },
    Broadcast { event: String, payload: Value },
    Closed,
}

fn parse_inbound(text: &str, topic: &str) -> Option<Inbound> {
    if text.len() > 64 * 1024 {
        return None;
    }
    let value: Value = serde_json::from_str(text).ok()?;
    if value["topic"] != topic {
        return None;
    }
    match value["event"].as_str()? {
        "phx_reply" if value["payload"]["status"] == "ok" => {
            // Replies to pushes other than the join carry no join_ref.
            (value["ref"] == value["join_ref"]).then(|| Inbound::Joined {
                reference: value["ref"].as_str().unwrap_or_default().to_owned(),
            })
        }
        "phx_reply" => Some(Inbound::Refused {
            reason: refusal(&value["payload"]["response"]),
        }),
        "broadcast" => Some(Inbound::Broadcast {
            event: value["payload"]["event"].as_str()?.to_owned(),
            payload: value["payload"]["payload"].clone(),
        }),
        "phx_close" | "phx_error" => Some(Inbound::Closed),
        "system" if value["payload"]["status"] == "error" => Some(Inbound::Refused {
            reason: refusal(&value["payload"]),
        }),
        _ => None,
    }
}

fn refusal(response: &Value) -> String {
    let detail = response["reason"]
        .as_str()
        .or_else(|| response["message"].as_str())
        .unwrap_or("not allowed");
    format!(
        "The realtime server refused the channel: {}",
        crate::agent_gate::redact(detail)
            .chars()
            .take(160)
            .collect::<String>()
    )
}

fn safe_id(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 64
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Shape checks for a phone command. On error, returns the command id (if
/// any) so the phone can be told why.
pub(crate) fn parse_command(payload: &Value, now_ms: u64) -> Result<Incoming, (String, String)> {
    let id = payload["id"]
        .as_str()
        .filter(|id| safe_id(id))
        .unwrap_or_default()
        .to_owned();
    let fail = |message: &str| Err((id.clone(), message.to_owned()));
    if id.is_empty() {
        return fail("Missing command id.");
    }
    let sent = payload["sent_ms"].as_u64().unwrap_or(0);
    if sent == 0 || sent + MAX_COMMAND_AGE_MS < now_ms || sent > now_ms + MAX_COMMAND_AGE_MS {
        return fail("The command is too old or its clock is off.");
    }
    let text = || {
        payload["text"]
            .as_str()
            .map(str::trim)
            .filter(|t| !t.is_empty() && t.chars().count() <= MAX_TEXT && !t.contains('\0'))
            .map(str::to_owned)
    };
    let command = match payload["kind"].as_str().unwrap_or_default() {
        "luma_order" => match text() {
            Some(text) => Command::LumaOrder { text },
            None => return fail("Write an order of up to 8000 characters."),
        },
        "chat_prompt" => {
            let session = payload["session"].as_str().unwrap_or_default();
            if covalos_agents::SessionId::new(session).is_err() {
                return fail("Unknown conversation.");
            }
            match text() {
                Some(text) => Command::ChatPrompt {
                    session: session.to_owned(),
                    text,
                },
                None => return fail("Write a message of up to 8000 characters."),
            }
        }
        "approval" => {
            let ticket = payload["ticket"].as_str().unwrap_or_default();
            let Some(allow) = payload["allow"].as_bool() else {
                return fail("Say whether to allow or deny.");
            };
            if !safe_id(ticket) {
                return fail("Unknown request.");
            }
            Command::Approval {
                ticket: ticket.to_owned(),
                allow,
            }
        }
        "refresh" => Command::Refresh,
        _ => return fail("Unknown command."),
    };
    Ok(Incoming { id, command })
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_800_000_000_000;

    #[test]
    fn phoenix_messages_follow_the_realtime_protocol() {
        let topic = topic("8d0f-11aa");
        assert_eq!(topic, "realtime:covalos-remote:8d0f-11aa");
        let join = join_message(&topic, "token", "1");
        assert_eq!(join["event"], "phx_join");
        assert_eq!(join["payload"]["config"]["private"], true);
        assert_eq!(join["join_ref"], "1");
        let state = broadcast_message(&topic, "state", json!({"sessions": []}), "2");
        assert_eq!(state["payload"]["type"], "broadcast");
        assert_eq!(state["payload"]["event"], "state");
        assert_eq!(heartbeat_message("3")["topic"], "phoenix");
        assert_eq!(
            access_token_message(&topic, "new", "4")["payload"]["access_token"],
            "new"
        );
    }

    #[test]
    fn inbound_frames_are_scoped_to_the_channel() {
        let topic = topic("u1");
        let ok = json!({"topic": topic, "event": "phx_reply",
            "payload": {"status": "ok", "response": {}}, "ref": "1", "join_ref": "1"})
        .to_string();
        assert_eq!(
            parse_inbound(&ok, &topic),
            Some(Inbound::Joined {
                reference: "1".into()
            })
        );
        let refused = json!({"topic": topic, "event": "phx_reply",
            "payload": {"status": "error", "response": {"reason": "Unauthorized"}}, "ref": "1"})
        .to_string();
        assert!(matches!(
            parse_inbound(&refused, &topic),
            Some(Inbound::Refused { reason }) if reason.contains("Unauthorized")
        ));
        let command = json!({"topic": topic, "event": "broadcast",
            "payload": {"type": "broadcast", "event": "command", "payload": {"id": "c1"}}})
        .to_string();
        assert!(matches!(
            parse_inbound(&command, &topic),
            Some(Inbound::Broadcast { event, .. }) if event == "command"
        ));
        assert_eq!(
            parse_inbound(&command, "realtime:covalos-remote:other"),
            None
        );
    }

    #[test]
    fn commands_are_shape_checked_before_the_editor_sees_them() {
        let luma = json!({"id": "c1", "sent_ms": NOW, "kind": "luma_order", "text": " Fix the build "});
        assert_eq!(
            parse_command(&luma, NOW).unwrap().command,
            Command::LumaOrder {
                text: "Fix the build".into()
            }
        );
        let approval =
            json!({"id": "c2", "sent_ms": NOW, "kind": "approval", "ticket": "t-1", "allow": true});
        assert_eq!(
            parse_command(&approval, NOW).unwrap().command,
            Command::Approval {
                ticket: "t-1".into(),
                allow: true
            }
        );
        let stale = json!({"id": "c3", "sent_ms": NOW - 10 * 60_000, "kind": "refresh"});
        assert_eq!(parse_command(&stale, NOW).unwrap_err().0, "c3");
        let traversal =
            json!({"id": "c4", "sent_ms": NOW, "kind": "approval", "ticket": "../x", "allow": true});
        assert!(parse_command(&traversal, NOW).is_err());
        let huge = json!({"id": "c5", "sent_ms": NOW, "kind": "luma_order", "text": "x".repeat(MAX_TEXT + 1)});
        assert!(parse_command(&huge, NOW).is_err());
        let unknown = json!({"id": "c6", "sent_ms": NOW, "kind": "shell", "text": "rm -rf /"});
        assert!(parse_command(&unknown, NOW).is_err());
        let no_id = json!({"sent_ms": NOW, "kind": "refresh"});
        assert_eq!(parse_command(&no_id, NOW).unwrap_err().0, "");
    }
}
