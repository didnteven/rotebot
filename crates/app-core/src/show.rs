//! Show: one user-started, metadata-only recording (plan §5; ADR 0001 §2).
//!
//! The controller spawns the native observer for a single session, sends it the
//! connector signatures, and turns its events into a Show state and connection
//! suggestions. The observer matches window titles in its own process and emits only
//! connector ids, so no title or screen content reaches this process. Closing the
//! observer's stdin stops it (control EOF), and the controller also enforces the
//! five-minute cap itself.

use crate::dto::{ConnectionSuggestion, ShowState};
use crate::AppCore;
use macroscope_connections::catalog::{Catalog, TitleMatch};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::{broadcast, mpsc};

pub const MAX_SECONDS: u32 = 300;
const LINE_LIMIT: usize = 64 * 1024;

/// Browsers whose window titles may identify a web app such as Gmail.
const BROWSERS: &[&str] = &[
    "com.google.Chrome",
    "com.apple.Safari",
    "company.thebrowser.Browser",
    "org.mozilla.firefox",
    "com.microsoft.edgemac",
    "com.brave.Browser",
];

struct Session {
    id: String,
    started_ms: i64,
    paused: bool,
    control: mpsc::Sender<String>,
    apps: BTreeSet<String>,
    /// connector id -> (signal kind, count) seen in this session.
    matched: BTreeMap<String, (String, i64)>,
}

struct Inner {
    state: ShowState,
    session: Option<Session>,
    accessibility: String,
}

#[derive(Clone)]
pub struct ShowController {
    observer: Option<PathBuf>,
    core: AppCore,
    inner: Arc<Mutex<Inner>>,
    tx: broadcast::Sender<ShowState>,
}

fn lock(m: &Mutex<Inner>) -> std::sync::MutexGuard<'_, Inner> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

/// The observer's `start` message: connector signatures from the catalog plus the
/// plan's default exclusions (the observer adds its own built-in list too).
pub fn start_message(session_id: &str, catalog: &Catalog, max_seconds: u32) -> Value {
    let connectors: Vec<Value> = catalog
        .connectors
        .iter()
        .filter(|c| !c.signatures.bundle_ids.is_empty() || !c.signatures.window_titles.is_empty())
        .map(|c| {
            let titles = |kind: fn(&TitleMatch) -> bool| -> Vec<String> {
                c.signatures
                    .window_titles
                    .iter()
                    .filter(|t| kind(&t.kind))
                    .map(|t| t.value.clone())
                    .collect()
            };
            json!({
                "id": c.id,
                "bundleIds": c.signatures.bundle_ids,
                "titleSuffixes": titles(|k| matches!(k, TitleMatch::Suffix)),
                "titleContains": titles(|k| matches!(k, TitleMatch::Contains)),
                "browserBundleIds": BROWSERS,
            })
        })
        .collect();
    json!({
        "type": "start",
        "sessionId": session_id,
        "maxSeconds": max_seconds.min(MAX_SECONDS),
        "rules": {"connectors": connectors},
        "exclusions": {"bundleIds": []},
        "policyEpoch": 1,
    })
}

fn is_connected(state: &Value) -> bool {
    ["/state/kind", "/kind", "/state"]
        .iter()
        .filter_map(|p| state.pointer(p))
        .any(|v| {
            v.as_str()
                .map(|s| s.eq_ignore_ascii_case("connected"))
                .unwrap_or(false)
                || v.get("Connected").is_some()
        })
}

impl ShowController {
    pub fn new(observer: Option<PathBuf>, core: AppCore) -> ShowController {
        let state = if !cfg!(target_os = "macos") {
            ShowState::Unavailable {
                reason: "Show is available on macOS only for now.".into(),
            }
        } else if observer.as_ref().is_none_or(|p| !p.is_file()) {
            ShowState::Unavailable {
                reason:
                    "Show needs the macOS observer helper, which is not included in this build."
                        .into(),
            }
        } else {
            ShowState::Idle {
                accessibility: "unknown".into(),
            }
        };
        let (tx, _) = broadcast::channel(64);
        ShowController {
            observer,
            core,
            inner: Arc::new(Mutex::new(Inner {
                state,
                session: None,
                accessibility: "unknown".into(),
            })),
            tx,
        }
    }

    pub fn state(&self) -> ShowState {
        lock(&self.inner).state.clone()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ShowState> {
        self.tx.subscribe()
    }

    fn publish(&self, inner: &mut Inner) {
        if let Some(s) = &inner.session {
            inner.state = ShowState::Recording {
                session_id: s.id.clone(),
                started_at_ms: s.started_ms,
                max_seconds: MAX_SECONDS,
                paused: s.paused,
                apps_seen: s.apps.len() as u32,
                matches: s.matched.values().map(|(_, n)| *n as u32).sum(),
            };
        }
        let _ = self.tx.send(inner.state.clone());
    }

    /// Starts one recording. Returns the current state if one is already running.
    pub async fn start(&self) -> ShowState {
        let path = match (&self.observer, self.state()) {
            (_, ShowState::Unavailable { .. }) => return self.state(),
            (_, s @ ShowState::Recording { .. }) => return s,
            (Some(p), _) => p.clone(),
            (None, _) => return self.state(),
        };
        let mut child = match Command::new(&path)
            .env_clear()
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
        {
            Ok(c) => c,
            Err(_) => {
                let mut inner = lock(&self.inner);
                inner.state = ShowState::Unavailable {
                    reason: "The observer helper could not start.".into(),
                };
                self.publish(&mut inner);
                return inner.state.clone();
            }
        };
        let (Some(mut stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            return self.state();
        };
        let session_id = format!("show_{}", uuid::Uuid::new_v4().simple());
        let (control, mut control_rx) = mpsc::channel::<String>(8);
        let first = start_message(&session_id, &Catalog::embedded(), MAX_SECONDS).to_string();
        let _ = control.send(first).await;
        // Writer: owns stdin. When every sender is dropped, stdin closes (observer EOF stop).
        tokio::spawn(async move {
            while let Some(line) = control_rx.recv().await {
                if stdin
                    .write_all(format!("{line}\n").as_bytes())
                    .await
                    .is_err()
                {
                    break;
                }
            }
            let _ = stdin.shutdown().await;
        });
        {
            let mut inner = lock(&self.inner);
            inner.session = Some(Session {
                id: session_id.clone(),
                started_ms: self.core.now_ms(),
                paused: false,
                control,
                apps: BTreeSet::new(),
                matched: BTreeMap::new(),
            });
            self.publish(&mut inner);
        }
        self.core.log_activity(
            "show",
            "Show recording started",
            Some("Metadata only; up to 5 minutes".into()),
        );
        // Reader: observer events until it stops or exits.
        let reader = self.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if line.len() > LINE_LIMIT {
                    continue;
                }
                let Ok(event) = serde_json::from_str::<Value>(&line) else {
                    continue;
                };
                if reader.handle_event(&event) {
                    break;
                }
            }
            reader.finish();
            let _ = child.wait().await;
        });
        // Hard cap enforced here as well as in the observer.
        let timer = self.clone();
        let id = session_id.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(MAX_SECONDS as u64 + 5)).await;
            if timer.current_session_id().as_deref() == Some(id.as_str()) {
                timer.stop().await;
            }
        });
        self.state()
    }

    fn current_session_id(&self) -> Option<String> {
        lock(&self.inner).session.as_ref().map(|s| s.id.clone())
    }

    /// Applies one observer event; returns true when the session has ended.
    fn handle_event(&self, event: &Value) -> bool {
        let mut inner = lock(&self.inner);
        let Some(session) = inner.session.as_mut() else {
            return true;
        };
        if event.get("sessionId").and_then(Value::as_str) != Some(session.id.as_str()) {
            return false; // never accept another session's events
        }
        match event.get("type").and_then(Value::as_str) {
            Some("ready") => {
                let ax = event
                    .pointer("/permissions/accessibility")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                inner.accessibility = ax.to_string();
            }
            Some("app_focus") => {
                if let Some(name) = event.get("appName").and_then(Value::as_str) {
                    session.apps.insert(name.chars().take(80).collect());
                }
            }
            Some("connector_match") => {
                let (Some(id), Some(kind)) = (
                    event.get("connectorId").and_then(Value::as_str),
                    event.get("signalKind").and_then(Value::as_str),
                ) else {
                    return false;
                };
                if Catalog::embedded().get(id).is_none()
                    || !matches!(kind, "foreground_app" | "window_title")
                {
                    return false;
                }
                let entry = session
                    .matched
                    .entry(id.to_string())
                    .or_insert((kind.to_string(), 0));
                entry.1 += 1;
                let _ = self
                    .core
                    .database()
                    .record_detection(id, kind, 1, self.core.now_ms());
            }
            Some("stopped") => return true,
            _ => {}
        }
        self.publish(&mut inner);
        false
    }

    fn finish(&self) {
        let mut inner = lock(&self.inner);
        let Some(session) = inner.session.take() else {
            return;
        };
        let now = self.core.now_ms();
        let catalog = Catalog::embedded();
        let hidden = self
            .core
            .database()
            .hidden_suggestions(now)
            .unwrap_or_default();
        let connected: Vec<String> = self
            .core
            .database()
            .connector_states()
            .unwrap_or_default()
            .into_iter()
            .filter(|(_, v)| is_connected(v))
            .map(|(id, _)| id)
            .collect();
        let suggestions: Vec<ConnectionSuggestion> = session
            .matched
            .iter()
            .filter(|(id, _)| !hidden.contains(id) && !connected.contains(id))
            .filter_map(|(id, (kind, count))| {
                let name = catalog.get(id)?.name.clone();
                Some(ConnectionSuggestion {
                    connector_id: id.clone(),
                    message: format!("You used {name} while recording. Connect {name} so tools can work with it."),
                    connector_name: name,
                    signal_kind: kind.clone(),
                    count: *count,
                })
            })
            .collect();
        let duration = ((now - session.started_ms).max(0) / 1000) as u32;
        inner.state = ShowState::Finished {
            session_id: session.id.clone(),
            duration_seconds: duration.min(MAX_SECONDS + 10),
            apps_seen: session.apps.iter().cloned().collect(),
            suggestions: suggestions.clone(),
        };
        let _ = self.tx.send(inner.state.clone());
        drop(inner);
        let detail = if suggestions.is_empty() {
            format!("{} apps seen", session.apps.len())
        } else {
            format!(
                "{} apps seen; suggested connecting {}",
                session.apps.len(),
                suggestions
                    .iter()
                    .map(|s| s.connector_name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        self.core
            .log_activity("show", "Show recording finished", Some(detail));
    }

    pub async fn pause(&self, paused: bool) -> ShowState {
        let control = {
            let mut inner = lock(&self.inner);
            let Some(s) = inner.session.as_mut() else {
                return inner.state.clone();
            };
            s.paused = paused;
            let c = s.control.clone();
            self.publish(&mut inner);
            c
        };
        let _ = control
            .send(json!({"type": if paused { "pause" } else { "resume" }}).to_string())
            .await;
        self.state()
    }

    /// Asks the observer to stop, then closes its stdin; the reader finishes the session.
    pub async fn stop(&self) -> ShowState {
        let control = {
            let inner = lock(&self.inner);
            let Some(s) = inner.session.as_ref() else {
                return inner.state.clone();
            };
            s.control.clone()
        };
        let _ = control.send(json!({"type": "stop"}).to_string()).await;
        // Replace the session's sender so stdin closes once the writer drains.
        {
            let mut inner = lock(&self.inner);
            if let Some(s) = inner.session.as_mut() {
                let (dead, _) = mpsc::channel(1);
                s.control = dead;
            }
        }
        drop(control);
        for _ in 0..40 {
            if lock(&self.inner).session.is_none() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        if lock(&self.inner).session.is_some() {
            self.finish(); // observer did not exit in 2 s; kill_on_drop reaps it with the reader
        }
        self.state()
    }

    /// Accessibility as last reported by the observer ("unknown" before the first session).
    pub fn accessibility(&self) -> String {
        lock(&self.inner).accessibility.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_message_carries_catalog_signatures_and_the_cap() {
        let msg = start_message("s1", &Catalog::embedded(), 9999);
        assert_eq!(msg["type"], "start");
        assert_eq!(msg["maxSeconds"], MAX_SECONDS);
        let connectors = msg["rules"]["connectors"].as_array().unwrap();
        let gmail = connectors.iter().find(|c| c["id"] == "gmail").unwrap();
        assert_eq!(gmail["titleSuffixes"][0], " - Gmail");
        assert!(gmail["browserBundleIds"]
            .as_array()
            .unwrap()
            .iter()
            .any(|b| b == "com.google.Chrome"));
        let slack = connectors.iter().find(|c| c["id"] == "slack").unwrap();
        assert!(!slack["bundleIds"].as_array().unwrap().is_empty());
        for c in connectors {
            for key in [
                "id",
                "bundleIds",
                "titleSuffixes",
                "titleContains",
                "browserBundleIds",
            ] {
                assert!(c.get(key).is_some(), "observer requires {key}");
            }
        }
    }

    #[test]
    fn connected_state_detection_accepts_common_shapes() {
        assert!(is_connected(&json!({"state": {"kind": "connected"}})));
        assert!(is_connected(&json!({"kind": "connected"})));
        assert!(is_connected(
            &json!({"state": {"Connected": {"since_ms": 1}}})
        ));
        assert!(!is_connected(&json!({"state": {"kind": "not_connected"}})));
        // The exact format the connections service persists.
        let status = macroscope_connections::state::ConnectionStatus {
            connector_id: "gmail".into(),
            state: macroscope_connections::state::ConnectionState::Connected {
                since_ms: 1,
                tool_count: 2,
            },
            mcp_url: "https://x".into(),
            last_checked_ms: None,
        };
        assert!(is_connected(&serde_json::to_value(&status).unwrap()));
        let mut off = status;
        off.state = macroscope_connections::state::ConnectionState::NotConnected;
        assert!(!is_connected(&serde_json::to_value(&off).unwrap()));
    }
}
