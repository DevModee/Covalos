//! Normalized agent events and decoders for provider output streams.
//!
//! The decoders cover the JSON line formats COVALOS already consumes in
//! production: `claude --print --output-format stream-json` and
//! `codex exec --json`. Unknown lines are ignored rather than guessed at.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "type")]
pub enum AgentEvent {
    Started {
        provider_session: String,
        model: Option<String>,
    },
    TextDelta {
        text: String,
    },
    Message {
        text: String,
    },
    ToolStarted {
        name: String,
    },
    ApprovalRequested {
        ticket: String,
        summary: String,
    },
    ApprovalResolved {
        ticket: String,
        allowed: bool,
    },
    Usage(Usage),
    /// Cost as reported by the provider itself, in millionths of a US dollar.
    Cost {
        usd_micros: u64,
    },
    TurnCompleted,
    Failed {
        message: String,
    },
    Cancelled,
    Closed,
}

pub trait StreamDecoder {
    fn decode(&mut self, line: &str) -> Vec<AgentEvent>;
    /// Called once the process exits; reports failures the stream implied.
    fn finish(&mut self, exit_ok: bool) -> Vec<AgentEvent>;
}

fn parse(line: &str) -> Option<Value> {
    serde_json::from_str(line.trim()).ok()
}

fn usage(value: &Value) -> Option<AgentEvent> {
    Some(AgentEvent::Usage(Usage {
        input_tokens: value["input_tokens"].as_u64()?,
        output_tokens: value["output_tokens"].as_u64()?,
    }))
}

fn exit_events(failure: Option<String>, completed: bool, exit_ok: bool) -> Vec<AgentEvent> {
    match failure {
        Some(message) => vec![AgentEvent::Failed { message }],
        None if completed => Vec::new(),
        None if exit_ok => vec![AgentEvent::TurnCompleted],
        None => vec![AgentEvent::Failed {
            message: "provider exited without a result".into(),
        }],
    }
}

#[derive(Default)]
pub struct ClaudeDecoder {
    failure: Option<String>,
    completed: bool,
}

impl StreamDecoder for ClaudeDecoder {
    fn decode(&mut self, line: &str) -> Vec<AgentEvent> {
        let Some(json) = parse(line) else {
            return Vec::new();
        };
        match json["type"].as_str() {
            Some("system") if json["subtype"] == "init" => json["session_id"]
                .as_str()
                .map(|id| AgentEvent::Started {
                    provider_session: id.to_owned(),
                    model: json["model"].as_str().map(str::to_owned),
                })
                .into_iter()
                .collect(),
            Some("stream_event") if json["event"]["type"] == "content_block_delta" => json["event"]
                ["delta"]["text"]
                .as_str()
                .map(|text| AgentEvent::TextDelta {
                    text: text.to_owned(),
                })
                .into_iter()
                .collect(),
            Some("assistant") => json["message"]["content"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|block| block["type"] == "tool_use")
                .filter_map(|block| block["name"].as_str())
                .map(|name| AgentEvent::ToolStarted {
                    name: name.to_owned(),
                })
                .collect(),
            Some("result") if json["is_error"] == true => {
                self.failure = Some(
                    json["result"]
                        .as_str()
                        .unwrap_or("Claude Code reported an error")
                        .to_owned(),
                );
                Vec::new()
            }
            Some("result") => {
                self.completed = true;
                let mut events = Vec::new();
                if let Some(text) = json["result"].as_str() {
                    events.push(AgentEvent::Message {
                        text: text.to_owned(),
                    });
                }
                events.extend(usage(&json["usage"]));
                if let Some(usd) = json["total_cost_usd"]
                    .as_f64()
                    .filter(|c| c.is_finite() && *c >= 0.0)
                {
                    events.push(AgentEvent::Cost {
                        usd_micros: (usd * 1_000_000.0).round() as u64,
                    });
                }
                events.push(AgentEvent::TurnCompleted);
                events
            }
            _ => Vec::new(),
        }
    }

    fn finish(&mut self, exit_ok: bool) -> Vec<AgentEvent> {
        exit_events(self.failure.take(), self.completed, exit_ok)
    }
}

#[derive(Default)]
pub struct CodexDecoder {
    failure: Option<String>,
    completed: bool,
}

impl StreamDecoder for CodexDecoder {
    fn decode(&mut self, line: &str) -> Vec<AgentEvent> {
        let Some(json) = parse(line) else {
            return Vec::new();
        };
        let item = &json["item"];
        match json["type"].as_str() {
            Some("thread.started") => json["thread_id"]
                .as_str()
                .map(|id| AgentEvent::Started {
                    provider_session: id.to_owned(),
                    model: None,
                })
                .into_iter()
                .collect(),
            Some("item.started") if item["type"] == "command_execution" => {
                vec![AgentEvent::ToolStarted {
                    name: "shell".into(),
                }]
            }
            Some("item.completed") if item["type"] == "agent_message" => item["text"]
                .as_str()
                .map(|text| AgentEvent::Message {
                    text: text.to_owned(),
                })
                .into_iter()
                .collect(),
            Some("turn.completed") => {
                self.completed = true;
                usage(&json["usage"])
                    .into_iter()
                    .chain([AgentEvent::TurnCompleted])
                    .collect()
            }
            Some("error" | "turn.failed") => {
                self.failure = Some(
                    json["message"]
                        .as_str()
                        .or_else(|| json["error"]["message"].as_str())
                        .unwrap_or("Codex reported an error")
                        .to_owned(),
                );
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn finish(&mut self, exit_ok: bool) -> Vec<AgentEvent> {
        exit_events(self.failure.take(), self.completed, exit_ok)
    }
}

/// Decodes Agent Client Protocol (ACP) JSON-RPC 2.0 lines (`gemini --acp`,
/// `opencode acp`, `antigravity-acp`, etc.) into normalized `AgentEvent`s.
#[derive(Default)]
pub struct AcpDecoder {
    failure: Option<String>,
    completed: bool,
    accumulated: String,
}

impl StreamDecoder for AcpDecoder {
    fn decode(&mut self, line: &str) -> Vec<AgentEvent> {
        let Some(json) = parse(line) else {
            return Vec::new();
        };
        if let Some(err) = json.get("error").filter(|v| !v.is_null()) {
            let msg = err["message"]
                .as_str()
                .unwrap_or("ACP provider reported an error")
                .to_owned();
            self.failure = Some(msg);
            return Vec::new();
        }
        // JSON-RPC responses: id=2 is session/new (returns sessionId), id=3 is session/prompt (returns stopReason).
        if let Some(result) = json.get("result").filter(|v| !v.is_null()) {
            let mut out = Vec::new();
            if let Some(session_id) = result["sessionId"].as_str() {
                out.push(AgentEvent::Started {
                    provider_session: session_id.to_owned(),
                    // The session opens before the client applies its model selection.
                    model: None,
                });
            }
            if result["stopReason"].is_string() {
                if !self.accumulated.is_empty() {
                    out.push(AgentEvent::Message {
                        text: std::mem::take(&mut self.accumulated),
                    });
                }
                if let Some(u) = usage(&result["usage"]) {
                    out.push(u);
                }
                self.completed = true;
                out.push(AgentEvent::TurnCompleted);
            }
            return out;
        }
        // JSON-RPC notifications: `session/update`
        if json["method"].as_str() == Some("session/update") {
            let update = &json["params"]["update"];
            let kind = update["sessionUpdate"].as_str().unwrap_or("");
            match kind {
                "agent_message_chunk" => {
                    if let Some(text) = update["content"]["text"].as_str()
                        && !text.is_empty()
                    {
                        self.accumulated.push_str(text);
                        return vec![AgentEvent::TextDelta {
                            text: text.to_owned(),
                        }];
                    }
                }
                "tool_call" => {
                    let name = update["title"]
                        .as_str()
                        .or_else(|| update["kind"].as_str())
                        .unwrap_or("Tool")
                        .to_owned();
                    return vec![AgentEvent::ToolStarted { name }];
                }
                _ => {}
            }
        }
        Vec::new()
    }

    fn finish(&mut self, exit_ok: bool) -> Vec<AgentEvent> {
        exit_events(self.failure.take(), self.completed, exit_ok)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn feed(decoder: &mut dyn StreamDecoder, lines: &[Value]) -> Vec<AgentEvent> {
        lines
            .iter()
            .flat_map(|line| decoder.decode(&line.to_string()))
            .collect()
    }

    #[test]
    fn claude_stream_is_normalized() {
        let mut decoder = ClaudeDecoder::default();
        let events = feed(
            &mut decoder,
            &[
                json!({"type":"system","subtype":"init","session_id":"s1","model":"m"}),
                json!({"type":"stream_event","event":{"type":"content_block_delta","delta":{"text":"he"}}}),
                json!({"type":"assistant","message":{"content":[{"type":"text","text":"x"},{"type":"tool_use","name":"Read","input":{}}]}}),
                json!({"type":"result","result":"done","usage":{"input_tokens":3,"output_tokens":4},"total_cost_usd":0.0125}),
            ],
        );
        assert_eq!(
            events,
            vec![
                AgentEvent::Started {
                    provider_session: "s1".into(),
                    model: Some("m".into())
                },
                AgentEvent::TextDelta { text: "he".into() },
                AgentEvent::ToolStarted {
                    name: "Read".into()
                },
                AgentEvent::Message {
                    text: "done".into()
                },
                AgentEvent::Usage(Usage {
                    input_tokens: 3,
                    output_tokens: 4
                }),
                AgentEvent::Cost { usd_micros: 12_500 },
                AgentEvent::TurnCompleted,
            ]
        );
        assert!(decoder.finish(true).is_empty());
    }

    #[test]
    fn claude_errors_and_silent_exits_fail() {
        let mut decoder = ClaudeDecoder::default();
        feed(
            &mut decoder,
            &[json!({"type":"result","is_error":true,"result":"expired"})],
        );
        assert_eq!(
            decoder.finish(true),
            vec![AgentEvent::Failed {
                message: "expired".into()
            }]
        );
        let mut silent = ClaudeDecoder::default();
        assert!(matches!(
            silent.finish(false)[..],
            [AgentEvent::Failed { .. }]
        ));
        assert_eq!(silent.decode("not json"), vec![]);
    }

    #[test]
    fn codex_stream_is_normalized() {
        let mut decoder = CodexDecoder::default();
        let events = feed(
            &mut decoder,
            &[
                json!({"type":"thread.started","thread_id":"t1"}),
                json!({"type":"item.started","item":{"type":"command_execution"}}),
                json!({"type":"item.completed","item":{"type":"agent_message","text":"ok"}}),
                json!({"type":"turn.completed","usage":{"input_tokens":1,"output_tokens":2}}),
            ],
        );
        assert_eq!(events.len(), 5);
        assert_eq!(
            events[0],
            AgentEvent::Started {
                provider_session: "t1".into(),
                model: None
            }
        );
        assert_eq!(events[4], AgentEvent::TurnCompleted);
        let mut failing = CodexDecoder::default();
        feed(
            &mut failing,
            &[json!({"type":"turn.failed","error":{"message":"quota"}})],
        );
        assert_eq!(
            failing.finish(false),
            vec![AgentEvent::Failed {
                message: "quota".into()
            }]
        );
    }

    #[test]
    fn acp_stream_is_normalized() {
        let mut decoder = AcpDecoder::default();
        let events = feed(
            &mut decoder,
            &[
                json!({"jsonrpc":"2.0","id":2,"result":{"sessionId":"acp-1","models":{"currentModelId":"gemini-2.5-pro"}}}),
                json!({"jsonrpc":"2.0","method":"session/update","params":{"update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"Hi "}}}}),
                json!({"jsonrpc":"2.0","method":"session/update","params":{"update":{"sessionUpdate":"tool_call","title":"read_file"}}}),
                json!({"jsonrpc":"2.0","method":"session/update","params":{"update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"there"}}}}),
                json!({"jsonrpc":"2.0","id":3,"result":{"stopReason":"end_turn","usage":{"input_tokens":10,"output_tokens":5}}}),
            ],
        );
        assert_eq!(
            events,
            vec![
                AgentEvent::Started {
                    provider_session: "acp-1".into(),
                    model: None,
                },
                AgentEvent::TextDelta { text: "Hi ".into() },
                AgentEvent::ToolStarted {
                    name: "read_file".into(),
                },
                AgentEvent::TextDelta {
                    text: "there".into(),
                },
                AgentEvent::Message {
                    text: "Hi there".into(),
                },
                AgentEvent::Usage(Usage {
                    input_tokens: 10,
                    output_tokens: 5,
                }),
                AgentEvent::TurnCompleted,
            ]
        );
        assert!(decoder.finish(true).is_empty());
    }
}
