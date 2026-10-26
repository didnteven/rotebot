use crate::MuseError;
use serde_json::Value;
use std::collections::BTreeMap;
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildEvent {
    CandidateReceived,
    Completed { terminal: String },
    AuthRequired,
    Quota { reset_at: Option<u64> },
    PermissionProfileUnavailable,
    GenericFallback,
}
#[derive(Debug, Clone, Default)]
pub struct TurnReducer {
    pub session_id: Option<String>,
    pub turn_id: Option<String>,
    pub terminal: Option<String>,
    pub items: BTreeMap<String, Value>,
    pub usage: Option<Value>,
    pub view_cursor: Option<String>,
}
impl TurnReducer {
    pub fn acknowledge_turn(&mut self, session_id: String, turn_id: String) {
        self.session_id = Some(session_id);
        self.turn_id = Some(turn_id);
        self.terminal = None;
        self.items.clear();
        self.usage = None;
    }
    pub fn text(&self) -> String {
        self.items
            .values()
            .filter(|v| v["kind"] == "agentMessage")
            .filter_map(|v| v["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }
    pub fn apply(&mut self, frame: &Value) -> Option<BuildEvent> {
        if frame.get("error").is_some() {
            return Some(match classify_rpc_error(frame) {
                MuseError::PermissionProfileUnavailable => BuildEvent::PermissionProfileUnavailable,
                _ => BuildEvent::GenericFallback,
            });
        }
        let method = frame["method"].as_str()?;
        let p = &frame["params"];
        if p["sessionId"].as_str() != self.session_id.as_deref() {
            return None;
        }
        let tid = p["turnId"]
            .as_str()
            .or_else(|| p["item"]["turnId"].as_str());
        if tid.is_some() && tid != self.turn_id.as_deref() {
            return None;
        }
        if let Some(cursor) = p["viewCursor"].as_str() {
            self.view_cursor = Some(cursor.into());
        }
        match method {
            "item/started" | "item/updated" | "item/completed" => {
                if tid.is_none() || self.terminal.is_some() {
                    return None;
                }
                let item = &p["item"];
                let id = item["itemId"].as_str()?;
                let newer = self.items.get(id).is_none_or(|old| {
                    item["revision"].as_u64().unwrap_or(0) > old["revision"].as_u64().unwrap_or(0)
                });
                if newer {
                    self.items.insert(id.into(), item.clone());
                }
                if !matches!(
                    item["kind"].as_str(),
                    Some(
                        "userMessage" | "agentMessage" | "reasoning" | "toolCall" | "reminderChild"
                    )
                ) {
                    return Some(BuildEvent::GenericFallback);
                }
            }
            "item/delta" => {
                if self.terminal.is_some() {
                    return None;
                }
                let item = self.items.get_mut(p["itemId"].as_str()?)?;
                if item["turnId"].as_str() != self.turn_id.as_deref()
                    || item["status"] != "inProgress"
                {
                    return None;
                }
                let path = p["field"].as_str().unwrap_or("text");
                let mut target = item;
                for field in path.split('.') {
                    target = if let Ok(index) = field.parse::<usize>() {
                        target.get_mut(index)?
                    } else {
                        target.get_mut(if field == "output" {
                            "visibleOutput"
                        } else {
                            field
                        })?
                    };
                }
                if let (Some(old), Some(delta)) = (target.as_str(), p["delta"].as_str()) {
                    *target = Value::String(format!("{old}{delta}"));
                }
            }
            "turn/foregroundCompleted" if tid.is_some() && self.terminal.is_none() => {
                return Some(BuildEvent::CandidateReceived)
            }
            "turn/completed" if tid.is_some() && self.terminal.is_none() => {
                let terminal = p["terminal"].as_str()?.to_string();
                self.terminal = Some(terminal.clone());
                if p["error"]["kind"] == "authRequired" {
                    return Some(BuildEvent::AuthRequired);
                }
                let error = &p["error"];
                if let Some(quota) = error.get("quota").filter(|v| !v.is_null()) {
                    return Some(BuildEvent::Quota {
                        reset_at: quota["resetsAtMs"].as_u64(),
                    });
                }
                return Some(BuildEvent::Completed { terminal });
            }
            "session/tokenUsage" if tid.is_some() => self.usage = Some(p.clone()),
            "turn/started" | "session/statusChanged" => {}
            _ => return Some(BuildEvent::GenericFallback),
        }
        None
    }
}
pub fn classify_rpc_error(frame: &Value) -> MuseError {
    if frame["error"]["data"]["reason"]
        .as_str()
        .is_some_and(|s| s.starts_with("permission_profile_"))
    {
        MuseError::PermissionProfileUnavailable
    } else {
        MuseError::Protocol
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn replay(text: &str, r: &mut TurnReducer, events: &mut Vec<BuildEvent>, allow_ack: bool) {
        let mut pending = None;
        for line in text.lines() {
            let v: Value = serde_json::from_str(line).unwrap();
            if v["dir"] == "c2s" {
                let Ok(f) = serde_json::from_str::<Value>(v["raw"].as_str().unwrap_or("")) else {
                    continue;
                };
                if f["method"] == "turn/start" {
                    pending = Some((
                        f["id"].clone(),
                        f["params"]["sessionId"].as_str().unwrap().to_string(),
                    ));
                }
            }
            if v["dir"] != "s2c" {
                continue;
            }
            let f: Value = macroscope_protocol::read_message(
                &mut format!("{}\n", v["raw"].as_str().unwrap()).as_bytes(),
            )
            .unwrap();
            if allow_ack {
                if let Some((id, sid)) = &pending {
                    if f["id"] == *id && f["result"]["status"] == "accepted" {
                        r.acknowledge_turn(
                            sid.clone(),
                            f["result"]["turnId"].as_str().unwrap().into(),
                        );
                        pending = None;
                    }
                }
            }
            if let Some(e) = r.apply(&f) {
                events.push(e);
            }
        }
    }
    #[test]
    fn transcript_replay_echo_auth_profile_and_negative_framing() {
        let mut r = TurnReducer::default();
        let mut events = vec![];
        replay(
            include_str!("../../../plan/probes/transcripts/01-echo-basic.jsonl"),
            &mut r,
            &mut events,
            true,
        );
        assert_eq!(r.text(), "echo: hello from macroscope");
        assert_eq!(r.terminal.as_deref(), Some("completed"));
        assert!(r.usage.is_some());
        assert!(events.contains(&BuildEvent::CandidateReceived));
        let mut r = TurnReducer::default();
        events.clear();
        replay(
            include_str!("../../../plan/probes/transcripts/02-meta-badkey-serve.jsonl"),
            &mut r,
            &mut events,
            true,
        );
        assert!(events.contains(&BuildEvent::AuthRequired));
        replay(
            include_str!("../../../plan/probes/transcripts/21-goodkey.jsonl"),
            &mut r,
            &mut events,
            true,
        );
        assert!(events.contains(&BuildEvent::PermissionProfileUnavailable));
        replay(
            include_str!("../../../plan/probes/transcripts/15-echo-framing-negative.jsonl"),
            &mut r,
            &mut events,
            true,
        );
        assert!(events.contains(&BuildEvent::GenericFallback));
    }
    #[test]
    fn transcript_replay_orphan_reconciled_and_stale_terminal_ignored() {
        let mut r = TurnReducer::default();
        let mut events = vec![];
        replay(
            include_str!("../../../plan/probes/transcripts/14-echo-crash-midturn.jsonl"),
            &mut r,
            &mut events,
            true,
        );
        assert!(r.terminal.is_none());
        replay(
            include_str!("../../../plan/probes/transcripts/14-echo-crash-midturn-after.jsonl"),
            &mut r,
            &mut events,
            false,
        );
        assert_eq!(r.terminal.as_deref(), Some("cancelled"));
        r.acknowledge_turn(r.session_id.clone().unwrap(), "new-turn".into());
        events.clear();
        replay(
            include_str!("../../../plan/probes/transcripts/14-echo-crash-midturn-after.jsonl"),
            &mut r,
            &mut events,
            false,
        );
        assert!(r.terminal.is_none());
        assert!(r.text().is_empty());
    }
    #[test]
    fn revisions_deltas_unknowns_and_foreground_are_not_terminal() {
        use serde_json::json;
        let mut r = TurnReducer::default();
        r.acknowledge_turn("s".into(), "t".into());
        let item = json!({"itemId":"i","turnId":"t","revision":1,"kind":"agentMessage","text":"a","status":"inProgress"});
        r.apply(&json!({"method":"item/started","params":{"sessionId":"s","item":item}}));
        r.apply(
            &json!({"method":"item/delta","params":{"sessionId":"s","itemId":"i","delta":"b"}}),
        );
        assert_eq!(r.text(), "ab");
        r.apply(&json!({"method":"item/completed","params":{"sessionId":"s","item":{"itemId":"i","turnId":"t","revision":2,"kind":"agentMessage","text":"final","status":"completed"}}}));
        r.apply(&json!({"method":"item/completed","params":{"sessionId":"s","item":item}}));
        assert_eq!(r.text(), "final");
        assert_eq!(r.apply(&json!({"method":"turn/foregroundCompleted","params":{"sessionId":"s","turnId":"t"}})),Some(BuildEvent::CandidateReceived));
        assert!(r.terminal.is_none());
        assert_eq!(
            r.apply(&json!({"method":"new/method","params":{"sessionId":"s"}})),
            Some(BuildEvent::GenericFallback)
        );
        assert_eq!(r.apply(&json!({"method":"turn/completed","params":{"sessionId":"s","turnId":"old","terminal":"completed"}})),None);
    }
}

#[cfg(test)]
mod quota_tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn quota_branches_on_typed_detail_and_preserves_reset_instant() {
        let mut reducer = TurnReducer::default();
        reducer.acknowledge_turn("s".into(), "t".into());
        assert_eq!(reducer.apply(&json!({"method":"turn/completed","params":{"sessionId":"s","turnId":"t","terminal":"failed","error":{"kind":"modelError","message":"opaque","quota":{"limitKind":"daily","resetsAtMs":1792665013000_u64}}}})),Some(BuildEvent::Quota{reset_at:Some(1792665013000)}));
    }
}
