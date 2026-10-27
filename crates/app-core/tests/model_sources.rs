//! Full host flow with an in-process authenticated calendar/Gmail MCP and model endpoint.
#![cfg(feature = "synthetic-runtime-tests")]
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use macroscope_app_core::{
    connections::Connections, dto::*, schedules::*, AppCore, CoreConfig, NativeMuse,
};
use macroscope_builder::{ConnectionSource, ModelStep, Program};
use macroscope_connections::state::{ConnectionState as LibState, ConnectionStatus};
use macroscope_vault::{MemoryStore, Secret, SecretRef, SecretStore};
use serde_json::{json, Value};
use std::{
    path::Path,
    sync::{
        atomic::{AtomicI64, Ordering},
        Arc, Mutex,
    },
};
const TOKEN: &str = "MCP-AUTH-PRIVATE-CANARY";
const ANSWER: &str = "Calendar: review the bills at 9 AM.";
#[derive(Clone)]
struct Fake {
    program: Arc<Mutex<Program>>,
    calls: Arc<Mutex<Vec<Value>>>,
    models: Arc<Mutex<Vec<Value>>>,
    answer: Arc<Mutex<Value>>,
    missing: Arc<Mutex<bool>>,
    unauthorized: Arc<Mutex<bool>>,
    result: Arc<Mutex<Option<Value>>>,
    delay: Arc<AtomicI64>,
    blocked: Arc<tokio::sync::Notify>,
}
async fn endpoint(State(f): State<Fake>, headers: HeaderMap, Json(body): Json<Value>) -> Response {
    if body.get("model").is_some() {
        assert!(!body.to_string().contains(TOKEN));
        assert!(!headers
            .values()
            .any(|h| h.to_str().unwrap().contains(TOKEN)));
        let runtime = body["text"]["format"]["name"] == "model_step";
        let value = if runtime {
            f.models.lock().unwrap().push(body);
            f.answer.lock().unwrap().clone()
        } else {
            json!(*f.program.lock().unwrap())
        };
        if runtime && f.delay.load(Ordering::SeqCst) == 2 {
            f.blocked.notify_one();
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
        return Json(json!({"status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":value.to_string()}]}],"usage":{"input_tokens":100,"output_tokens":100}})).into_response();
    }
    assert_eq!(
        headers["authorization"].to_str().unwrap(),
        format!("Bearer {TOKEN}")
    );
    if *f.unauthorized.lock().unwrap() {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let result = match body["method"].as_str().unwrap() {
        "initialize" => json!({"protocolVersion":"2025-06-18"}),
        "notifications/initialized" => return StatusCode::ACCEPTED.into_response(),
        "tools/list" => {
            if *f.missing.lock().unwrap() {
                json!({"tools":[]})
            } else {
                json!({"tools":[
                    {"name":"list_events","description":"Events (untrusted)","inputSchema":{"type":"object","properties":{"start":{"type":"string"},"end":{"type":"string"}},"required":["start","end"],"additionalProperties":false},"annotations":{"readOnlyHint":true,"destructiveHint":false}},
                    {"name":"search_threads","description":"Search bills","inputSchema":{"type":"object","properties":{"query":{"type":"string"}},"required":["query"],"additionalProperties":false},"annotations":{"readOnlyHint":true}},
                    {"name":"create_event","inputSchema":{"type":"object"},"annotations":{"readOnlyHint":false}}
                ]})
            }
        }
        "tools/call" => {
            f.calls.lock().unwrap().push(body["params"].clone());
            f.result.lock().unwrap().clone().unwrap_or(json!({"content":[{"type":"text","text":"[{\"title\":\"Review bills\",\"time\":\"09:00\"}]"}]}))
        }
        other => panic!("unexpected {other}"),
    };
    if body["method"] == "tools/call" && f.delay.load(Ordering::SeqCst) == 1 {
        f.blocked.notify_one();
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }
    Json(json!({"jsonrpc":"2.0","id":body["id"],"result":result})).into_response()
}
struct Fixture {
    _root: tempfile::TempDir,
    core: AppCore,
    conns: Connections,
    fake: Fake,
    clock: Arc<AtomicI64>,
    folder: FileHandle,
    vault: Arc<MemoryStore>,
}
fn program(model: bool, gmail: bool) -> Program {
    let source = if gmail {
        ConnectionSource {
            name: "bills".into(),
            connector_id: "gmail".into(),
            tool_name: "search_threads".into(),
            args_json: r#"{"query":"after:{prev_month_start} before:{month_start} bills"}"#.into(),
        }
    } else {
        ConnectionSource {
            name: "events".into(),
            connector_id: "google-calendar".into(),
            tool_name: "list_events".into(),
            args_json: r#"{"start":"{today_start}","end":"{today_end}"}"#.into(),
        }
    };
    let code = if model {
        format!(
            r#"function run(){{const d=JSON.parse(macroscope.source('{}'));const a=macroscope.model.ask('Summarize these events',d,{{type:'object',additionalProperties:false,properties:{{summary:{{type:'string'}}}},required:['summary']}});macroscope.log(a.summary);macroscope.output('summary.txt',a.summary+'\n');}}"#,
            source.name
        )
    } else {
        format!(
            "function run(){{macroscope.output('summary.txt',macroscope.source('{}'));}}",
            source.name
        )
    };
    Program {
        summary: "Calendar summary".into(),
        output_name: "summary.txt".into(),
        code,
        web_sources: vec![],
        connection_sources: vec![source],
        model_step: model.then(|| ModelStep {
            purpose: "Write the summary".into(),
            prompt: "Summarize these events".into(),
        }),
    }
}
impl Fixture {
    async fn new(p: Program) -> Self {
        let fake = Fake {
            program: Arc::new(Mutex::new(p)),
            calls: Arc::default(),
            models: Arc::default(),
            answer: Arc::new(Mutex::new(json!({"summary":ANSWER}))),
            missing: Arc::default(),
            unauthorized: Arc::default(),
            result: Arc::default(),
            delay: Arc::default(),
            blocked: Arc::default(),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let app = Router::new()
            .route("/mcp", post(endpoint))
            .route("/responses", post(endpoint))
            .with_state(fake.clone());
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let root = tempfile::tempdir().unwrap();
        let vault = Arc::new(MemoryStore::default());
        let clock = Arc::new(AtomicI64::new(
            chrono::DateTime::parse_from_rfc3339("2026-10-27T17:00:00Z")
                .unwrap()
                .timestamp_millis(),
        ));
        let now = clock.clone();
        let core = AppCore::new(CoreConfig {
            data_dir: root.path().join("data"),
            vault: vault.clone(),
            gateway_base_url: base.clone(),
            home: root.path().join("empty-home"),
            muse: Arc::new(NativeMuse {
                home: root.path().join("empty-home"),
            }),
            app_version: "test".into(),
            platform: "macos".into(),
            now_ms: Arc::new(move || now.load(Ordering::SeqCst)),
        })
        .unwrap();
        core.use_synthetic_runtime_for_tests(
            &std::env::current_exe()
                .unwrap()
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join("macroscope-runner"),
            Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../runner/profile.sb")),
        );
        core.set_schedule_timezone(chrono_tz::Pacific::Auckland);
        for id in ["google-calendar", "gmail"] {
            let url = format!("{base}/mcp");
            core.database()
                .set_connector_state(
                    id,
                    &json!(ConnectionStatus {
                        connector_id: id.into(),
                        state: LibState::Connected {
                            since_ms: 0,
                            tool_count: 3
                        },
                        mcp_url: url.clone(),
                        last_checked_ms: None
                    }),
                )
                .unwrap();
            // Seed a previously established connection; production uses the same vault record.
            let token = json!({"access_token":TOKEN,"refresh_token":null,"expires_at_ms":null,"resource":url,"metadata":{"issuer":base,"authorization_endpoint":format!("{base}/authorize"),"token_endpoint":format!("{base}/token"),"registration_endpoint":null,"revocation_endpoint":null,"code_challenge_methods_supported":["S256"]},"credentials":{"client_id":"fake-client","client_secret":null}});
            vault
                .set(
                    &SecretRef::new(format!("mcp-token:{id}")).unwrap(),
                    Secret::new(token.to_string().into_bytes()),
                )
                .unwrap();
        }
        core.save_api_key("test-meta-key-123456789").await.unwrap();
        core.select_route(ModelRoute::ApiKey).await.unwrap();
        let conns = Connections::new(core.clone(), vault.clone()).unwrap();
        let folder = core.register_picked_dir(root.path()).unwrap();
        Self {
            _root: root,
            core,
            conns,
            fake,
            clock,
            folder,
            vault,
        }
    }
    async fn start(&self) -> BuildState {
        let id = self
            .fake
            .program
            .lock()
            .unwrap()
            .connection_sources
            .first()
            .map(|s| s.connector_id.clone());
        let draft = self
            .core
            .draft_task_no_input("Summarize my calendar every morning")
            .unwrap();
        let draft = self
            .core
            .update_task(&draft.draft_id, TaskKind::Program, "summary.txt", None)
            .unwrap();
        if let Some(id) = id {
            self.conns
                .select_for_task(&draft.draft_id, &[id])
                .await
                .unwrap();
        }
        let packet = self.core.outbound_preview(&draft.draft_id).await.unwrap();
        assert!(packet.payload_text.contains("connection_read_tools"));
        self.core.start_build(&draft.draft_id, true).await.unwrap()
    }
    async fn terminal(&self, id: &str, approve: bool) -> BuildState {
        for _ in 0..500 {
            let s = self.core.build_state(id).unwrap();
            if s.phase.is_terminal() {
                return s;
            }
            if approve && s.phase == BuildPhase::Candidate {
                if let Some(p) = &s.program {
                    self.core.approve_build_sources(id, p).unwrap();
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        panic!("build timeout");
    }
    async fn save(&self) -> ToolDetail {
        let s = self.start().await;
        let s = self.terminal(&s.build_id, true).await;
        assert_eq!(s.phase, BuildPhase::Ready, "{s:?}");
        self.core
            .save_tool(&s.build_id, "Morning calendar summary")
            .await
            .unwrap()
    }
    async fn stage(&self, t: &ToolDetail) -> Result<StageResult, macroscope_app_core::CoreError> {
        self.core
            .stage_run_no_input(&t.summary_row.id, &self.folder.handle_id, "summary.txt")
            .await
    }
}
#[tokio::test]
async fn build_requires_exact_review_then_save_stage_reads_substituted_events_and_logs_templates() {
    let f = Fixture::new(program(false, false)).await;
    let b = f.start().await;
    loop {
        let s = f.core.build_state(&b.build_id).unwrap();
        if s.phase == BuildPhase::Candidate {
            break;
        }
        assert!(!s.phase.is_terminal(), "{s:?}");
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    assert!(f.fake.calls.lock().unwrap().is_empty());
    let mut changed = f.fake.program.lock().unwrap().clone();
    changed.connection_sources[0].args_json = "{}".into();
    assert!(f.core.approve_build_sources(&b.build_id, &changed).is_err());
    f.core
        .approve_build_sources(&b.build_id, &f.fake.program.lock().unwrap())
        .unwrap();
    let ready = f.terminal(&b.build_id, false).await;
    assert_eq!(ready.phase, BuildPhase::Ready, "{ready:?}");
    let tool = f.core.save_tool(&b.build_id, "Calendar").await.unwrap();
    let StageResult::Staged(stage) = f.stage(&tool).await.unwrap() else {
        panic!("stage failed")
    };
    let CommitResult::Committed(receipt) =
        f.core.commit_run(&stage.run_id, &stage.plan_hash).unwrap()
    else {
        panic!("commit failed")
    };
    assert_eq!(
        std::fs::read(&receipt.output_path).unwrap(),
        b"[{\"title\":\"Review bills\",\"time\":\"09:00\"}]"
    );
    let calls = f.fake.calls.lock().unwrap();
    assert_eq!(calls.len(), 2);
    for call in calls.iter() {
        assert_eq!(
            *call,
            json!({"name":"list_events","arguments":{"start":"2026-10-28T00:00:00+13:00","end":"2026-10-28T23:59:59+13:00"}})
        );
    }
    let logs = f.core.list_outbound().unwrap();
    let reads: Vec<_> = logs
        .iter()
        .filter(|l| l.purpose == "Connection read")
        .collect();
    assert_eq!(reads.len(), 2);
    assert_eq!(reads[0].destination, "Google Calendar");
    assert!(reads[0]
        .payload_text
        .as_ref()
        .unwrap()
        .contains("{today_start}"));
    assert!(!serde_json::to_string(&logs).unwrap().contains(TOKEN));
}
#[tokio::test]
async fn unapproved_templates_tools_and_non_read_only_sources_fail_closed() {
    let f = Fixture::new(program(false, false)).await;
    let t = f.save().await;
    let (_, rev, mut scope) = f
        .core
        .database()
        .active_grant(&t.summary_row.id, &t.version_hash)
        .unwrap()
        .unwrap();
    for replacement in [
        json!([]),
        json!([{"name":"events","connectorId":"google-calendar","toolName":"list_events","argsJson":"{}"}]),
        json!([{"name":"events","connectorId":"google-calendar","toolName":"create_event","argsJson":"{}"}]),
    ] {
        scope["connectionSources"] = replacement;
        f.core
            .database()
            .put_grant(
                "tampered",
                &t.summary_row.id,
                &t.version_hash,
                rev + 1,
                &scope,
            )
            .unwrap();
        assert!(f
            .stage(&t)
            .await
            .unwrap_err()
            .to_string()
            .contains("Unapproved"));
    }
    assert_eq!(f.fake.calls.lock().unwrap().len(), 1);
    let mut unsafe_p = program(false, false);
    unsafe_p.connection_sources[0].tool_name = "create_event".into();
    let f = Fixture::new(unsafe_p).await;
    let b = f.start().await;
    let s = f.terminal(&b.build_id, false).await;
    assert_eq!(s.phase, BuildPhase::Failed);
    assert!(s.failure.unwrap().message.contains("read-only"));
    assert!(f.fake.calls.lock().unwrap().is_empty());
}
#[tokio::test]
async fn missing_tool_reauth_and_local_only_run_nothing() {
    let f = Fixture::new(program(true, false)).await;
    let t = f.save().await;
    let before = f.fake.calls.lock().unwrap().len();
    let models = f.fake.models.lock().unwrap().len();
    *f.fake.missing.lock().unwrap() = true;
    assert!(f
        .stage(&t)
        .await
        .unwrap_err()
        .to_string()
        .contains("Reconnect or rebuild"));
    *f.fake.missing.lock().unwrap() = false;
    *f.fake.unauthorized.lock().unwrap() = true;
    assert!(f
        .stage(&t)
        .await
        .unwrap_err()
        .to_string()
        .contains("Reconnect Google Calendar"));
    assert_eq!(f.fake.calls.lock().unwrap().len(), before);
    assert_eq!(f.fake.models.lock().unwrap().len(), models);
    assert!(f
        .core
        .list_today()
        .unwrap()
        .iter()
        .flat_map(|d| &d.entries)
        .any(|e| e.status == "needs_you"));
    let f = Fixture::new(program(true, false)).await;
    let t = f.save().await;
    let before = f.fake.calls.lock().unwrap().len();
    f.core.set_local_only(true).await.unwrap();
    assert!(f
        .stage(&t)
        .await
        .unwrap_err()
        .to_string()
        .contains("Local only"));
    assert_eq!(f.fake.calls.lock().unwrap().len(), before);
    assert_eq!(f.fake.models.lock().unwrap().len(), 1);
}
#[tokio::test]
async fn model_answer_replays_into_exact_output_and_private_answers_are_absent_from_logs() {
    let f = Fixture::new(program(true, false)).await;
    let t = f.save().await;
    assert_eq!(t.model_step_cap, 60);
    let StageResult::Staged(stage) = f.stage(&t).await.unwrap() else {
        panic!("stage failed")
    };
    assert_eq!(stage.preview_text.as_deref(), Some(ANSWER));
    let CommitResult::Committed(r) = f.core.commit_run(&stage.run_id, &stage.plan_hash).unwrap()
    else {
        panic!("commit failed")
    };
    assert_eq!(
        std::fs::read(r.output_path).unwrap(),
        format!("{ANSWER}\n").as_bytes()
    );
    assert_eq!(f.fake.models.lock().unwrap().len(), 2);
    let logs = f.core.list_outbound().unwrap();
    assert_eq!(logs.iter().filter(|l| l.purpose == "Model step").count(), 2);
    let text = serde_json::to_string(&logs).unwrap();
    assert!(!text.contains(ANSWER));
    assert!(!text.contains(TOKEN));
    assert!(text.contains("dataBytes"));
    assert!(!serde_json::to_string(&f.core.list_activity().unwrap())
        .unwrap()
        .contains(ANSWER));
}
#[tokio::test]
async fn invalid_model_answer_and_request_limits_fail_and_cap_skips_before_any_reads() {
    let f = Fixture::new(program(true, false)).await;
    let t = f.save().await;
    *f.fake.answer.lock().unwrap() = json!({"summary":42});
    assert!(f
        .stage(&t)
        .await
        .unwrap_err()
        .to_string()
        .contains("schema"));
    let count = f.fake.calls.lock().unwrap().len();
    let models = f.fake.models.lock().unwrap().len();
    let t = f.core.set_model_step_cap(&t.summary_row.id, 1).unwrap();
    assert!(f
        .stage(&t)
        .await
        .unwrap_err()
        .to_string()
        .contains("cap reached"));
    assert_eq!(f.fake.calls.lock().unwrap().len(), count);
    assert_eq!(f.fake.models.lock().unwrap().len(), models);
    assert!(f
        .core
        .list_today()
        .unwrap()
        .iter()
        .flat_map(|d| &d.entries)
        .any(|e| e.status == "skipped"));
    for code in [
        "function run(){macroscope.model.ask('x'.repeat(8193),{}, {type:'string'});}",
        include_str!("../../../experiments/runtime-gate-v2/canaries/model-ask-limit.js"),
    ] {
        let mut p = program(true, false);
        p.code = code.into();
        let f = Fixture::new(p).await;
        let b = f.start().await;
        let s = f.terminal(&b.build_id, true).await;
        assert_eq!(s.phase, BuildPhase::Failed, "{s:?}");
        assert!(f.fake.models.lock().unwrap().is_empty());
    }
}
struct Notifications(Arc<Mutex<Vec<String>>>);
impl Notifier for Notifications {
    fn notify(&self, title: &str, body: &str) -> Result<(), String> {
        self.0.lock().unwrap().push(format!("{title}: {body}"));
        Ok(())
    }
}
#[tokio::test]
async fn scheduled_calendar_and_monthly_gmail_commit_locally_without_per_run_approval() {
    for gmail in [false, true] {
        let f = Fixture::new(program(true, gmail)).await;
        if gmail {
            f.clock.store(
                chrono::DateTime::parse_from_rfc3339("2026-10-31T17:00:00Z")
                    .unwrap()
                    .timestamp_millis(),
                Ordering::SeqCst,
            );
        }
        let t = f.save().await;
        let notifications = Arc::new(Mutex::new(vec![]));
        f.core
            .set_notifier(Arc::new(Notifications(notifications.clone())));
        f.core
            .set_schedule(
                &t.summary_row.id,
                ScheduleRequest {
                    frequency: if gmail {
                        Frequency::Monthly {
                            day: 1,
                            time: "07:00".into(),
                        }
                    } else {
                        Frequency::Daily {
                            time: "07:00".into(),
                        }
                    },
                    outputs: ScheduleOutputs {
                        today: true,
                        notification: true,
                        folder: None,
                        filename: "summary-{date}.txt".into(),
                    },
                    version_hash: t.version_hash.clone(),
                    approved: true,
                },
            )
            .unwrap();
        let now = f.clock.fetch_add(2 * 3_600_000, Ordering::SeqCst) + 2 * 3_600_000;
        let entries = f.core.run_due_schedules(now).await.unwrap();
        assert_eq!(entries.len(), 1, "{entries:?}");
        assert_eq!(entries[0].status, "done", "{entries:?}");
        assert_eq!(entries[0].body, format!("{ANSWER}\n"));
        assert_eq!(notifications.lock().unwrap().len(), 1);
        assert_eq!(f.fake.models.lock().unwrap().len(), 2);
        assert_eq!(f.fake.calls.lock().unwrap().len(), 2);
        if gmail {
            assert_eq!(
                f.fake.calls.lock().unwrap()[1],
                json!({"name":"search_threads","arguments":{"query":"after:2026-10-01T00:00:00+13:00 before:2026-11-01T00:00:00+13:00 bills"}})
            );
        }
        assert!(f.core.run_due_schedules(now).await.unwrap().is_empty());
    }
}
#[tokio::test]
async fn text_parts_structured_content_size_cap_templates_and_offline_demo() {
    assert_eq!(macroscope_app_core::connections::source_text(&json!({"content":[{"type":"text","text":"a"},{"type":"image"},{"type":"text","text":"b"}]})).unwrap(),"a\nb");
    assert_eq!(
        macroscope_app_core::connections::source_text(
            &json!({"structuredContent":{"a":1},"content":[{"type":"text","text":"ignored"}]})
        )
        .unwrap(),
        "{\"a\":1}"
    );
    let f = Fixture::new(program(false, false)).await;
    let t = f.save().await;
    *f.fake.result.lock().unwrap() =
        Some(json!({"content":[{"type":"text","text":"x".repeat(2*1024*1024+1)}]}));
    assert!(f.stage(&t).await.unwrap_err().to_string().contains("2 MB"));
    let args = f
        .core
        .source_args(
            r#"{"s":["{today}|{now}|{month_end}|{prev_month_end}|{unknown}"],"{today}":1}"#,
        )
        .unwrap();
    assert_eq!(args["{today}"], 1);
    assert_eq!(args["s"][0],"2026-10-28|2026-10-28T06:00:00+13:00|2026-10-31T23:59:59+13:00|2026-09-30T23:59:59+13:00|{unknown}");
    let f = Fixture::new(program(true, false)).await;
    let t = f.save().await;
    f.core.select_route(ModelRoute::OfflineDemo).await.unwrap();
    let StageResult::Staged(s) = f.stage(&t).await.unwrap() else {
        panic!("demo stage failed")
    };
    assert_eq!(s.preview_text.as_deref(), Some(""));
    assert_eq!(f.fake.models.lock().unwrap().len(), 1);
    assert!(f
        .vault
        .get(&SecretRef::new("mcp-token:google-calendar").unwrap())
        .unwrap()
        .is_some());
}

#[tokio::test]
async fn three_round_replay_finishes_and_fourth_round_fails_without_dispatching_a_fourth_call() {
    for count in [3, 4] {
        let mut p = program(true, false);
        p.code = format!(
            r#"function run(){{let a;for(let i=0;i<{count};i++){{a=macroscope.model.ask({{round:i}},{{type:'object',additionalProperties:false,properties:{{summary:{{type:'string'}}}},required:['summary']}});}}macroscope.output('summary.txt',a.summary+'\n');}}"#
        );
        let f = Fixture::new(p).await;
        let b = f.start().await;
        let s = f.terminal(&b.build_id, true).await;
        assert_eq!(f.fake.models.lock().unwrap().len(), 3);
        assert_eq!(
            s.phase,
            if count == 3 {
                BuildPhase::Ready
            } else {
                BuildPhase::Failed
            },
            "{s:?}"
        );
    }
}

#[tokio::test]
async fn emergency_stop_cancels_connection_reads_and_model_egress_before_staging() {
    for delay in [1, 2] {
        let f = Fixture::new(program(true, false)).await;
        let t = f.save().await;
        f.fake.delay.store(delay, Ordering::SeqCst);
        let core = f.core.clone();
        let folder = f.folder.handle_id.clone();
        let id = t.summary_row.id.clone();
        let stage =
            tokio::spawn(async move { core.stage_run_no_input(&id, &folder, "summary.txt").await });
        tokio::time::timeout(std::time::Duration::from_secs(3), f.fake.blocked.notified())
            .await
            .unwrap();
        f.core.emergency_stop().await.unwrap();
        let result = tokio::time::timeout(std::time::Duration::from_secs(2), stage)
            .await
            .unwrap()
            .unwrap();
        assert!(result.unwrap_err().to_string().contains("cancelled"));
        assert!(!f._root.path().join("summary.txt").exists());
    }
}
#[tokio::test]
async fn local_only_blocks_a_model_only_tool_and_server_cannot_echo_auth_into_model_data() {
    let mut p = program(true, false);
    p.connection_sources.clear();
    p.code="function run(){const a=macroscope.model.ask({}, {type:'object',additionalProperties:false,properties:{summary:{type:'string'}},required:['summary']});macroscope.output('summary.txt',a.summary);}".into();
    let f = Fixture::new(p).await;
    let t = f.save().await;
    f.core.set_local_only(true).await.unwrap();
    assert!(f
        .stage(&t)
        .await
        .unwrap_err()
        .to_string()
        .contains("Local only"));
    assert_eq!(f.fake.models.lock().unwrap().len(), 1);
    assert!(f.fake.calls.lock().unwrap().is_empty());
    let f = Fixture::new(program(true, false)).await;
    let t = f.save().await;
    *f.fake.result.lock().unwrap() = Some(json!({"content":[{"type":"text","text":TOKEN}]}));
    assert!(f
        .stage(&t)
        .await
        .unwrap_err()
        .to_string()
        .contains("invalid server response"));
    assert_eq!(f.fake.models.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn file_input_model_tools_also_record_monthly_cap_skips_in_today() {
    let mut p = program(true, false);
    p.connection_sources.clear();
    p.code="function run(){const a=macroscope.model.ask(macroscope.input('input_csv'),{type:'object',additionalProperties:false,properties:{summary:{type:'string'}},required:['summary']});macroscope.output('summary.txt',a.summary);}".into();
    let f = Fixture::new(p).await;
    let path = f._root.path().join("input.txt");
    std::fs::write(&path, "Calendar notes").unwrap();
    let file = f.core.register_picked_file(&path).unwrap();
    let draft = f
        .core
        .draft_task(&file.handle_id, "Summarize this text")
        .unwrap();
    let draft = f
        .core
        .update_task(&draft.draft_id, TaskKind::Program, "summary.txt", None)
        .unwrap();
    let build = f.core.start_build(&draft.draft_id, true).await.unwrap();
    let ready = f.terminal(&build.build_id, true).await;
    assert_eq!(ready.phase, BuildPhase::Ready, "{ready:?}");
    let t = f
        .core
        .save_tool(&build.build_id, "Text summary")
        .await
        .unwrap();
    assert!(!t.no_input);
    f.core.set_model_step_cap(&t.summary_row.id, 0).unwrap();
    let result = f
        .core
        .stage_run(
            &t.summary_row.id,
            &file.handle_id,
            &f.folder.handle_id,
            "summary.txt",
        )
        .await;
    assert!(result.unwrap_err().to_string().contains("cap reached"));
    assert!(f
        .core
        .list_today()
        .unwrap()
        .iter()
        .flat_map(|d| &d.entries)
        .any(|e| e.status == "skipped"));
    assert_eq!(f.fake.models.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn local_november_cap_and_budget_count_spend_before_utc_november() {
    let f = Fixture::new(program(true, false)).await;
    let t = f.save().await;
    let at = chrono::DateTime::parse_from_rfc3339("2026-11-01T00:30:00+13:00")
        .unwrap()
        .timestamp_millis();
    f.clock.store(at, Ordering::SeqCst);
    // November locally, still October in UTC. Reserve almost all November's budget.
    f.core
        .database()
        .put_build(&macroscope_storage::db::BuildRow {
            id: "november-spend".into(),
            draft_id: "past".into(),
            route: "api_key".into(),
            phase: "done".into(),
            reserved_micros: 24_000_000,
            spent_micros: None,
            state: json!({}),
            created_ms: at - 60_000,
            updated_ms: at - 60_000,
        })
        .unwrap();
    let models = f.fake.models.lock().unwrap().len();
    let err = f.stage(&t).await.unwrap_err().to_string();
    assert!(err.contains("monthly model budget reached"), "{err}");
    assert_eq!(f.fake.models.lock().unwrap().len(), models);
    // The build budget uses the same local-month window.
    let draft = f.core.draft_task_no_input("Build a local report").unwrap();
    assert!(f
        .core
        .start_build(&draft.draft_id, true)
        .await
        .unwrap_err()
        .to_string()
        .contains("monthly build budget"));
    f.core
        .database()
        .set_setting(
            &format!("model-step:{}:2026-11", t.summary_row.id),
            &json!(1),
        )
        .unwrap();
    f.core.set_model_step_cap(&t.summary_row.id, 1).unwrap();
    assert!(f
        .stage(&t)
        .await
        .unwrap_err()
        .to_string()
        .contains("monthly model-step cap reached"));
}
#[tokio::test]
async fn source_midnight_dst_gaps_advance_to_first_valid_minute() {
    let f = Fixture::new(program(false, false)).await;
    f.core.set_schedule_timezone(chrono_tz::America::Santiago);
    for (at, field, expected) in [
        (
            "2026-09-06T12:00:00Z",
            "today_start",
            "2026-09-06T01:00:00-03:00",
        ),
        (
            "2019-09-08T12:00:00Z",
            "today_start",
            "2019-09-08T01:00:00-03:00",
        ),
        (
            "2024-09-08T12:00:00Z",
            "today_start",
            "2024-09-08T01:00:00-03:00",
        ),
    ] {
        f.clock.store(
            chrono::DateTime::parse_from_rfc3339(at)
                .unwrap()
                .timestamp_millis(),
            Ordering::SeqCst,
        );
        let args = f
            .core
            .source_args(&format!(r#"{{"start":"{{{field}}}"}}"#))
            .unwrap();
        assert_eq!(args["start"], expected);
    }
}
#[tokio::test]
async fn oversized_model_answer_is_rejected_before_replay() {
    let f = Fixture::new(program(true, false)).await;
    let t = f.save().await;
    *f.fake.answer.lock().unwrap() = json!({"summary":"x".repeat(65536)});
    let error = f.stage(&t).await.unwrap_err().to_string();
    assert!(error.contains("Model answer exceeds 64 KiB"), "{error}");
}
#[tokio::test]
async fn model_prompt_is_bound_at_approval_version_hash_and_schedule() {
    let f = Fixture::new(program(true, false)).await;
    let t = f.save().await;
    let mut other = program(true, false);
    other.model_step.as_mut().unwrap().prompt = "Summarize everything".into();
    other.code = other
        .code
        .replace("Summarize these events", "Summarize everything");
    let g = Fixture::new(other).await;
    let u = g.save().await;
    // Same purpose, different prompt: a different tool version.
    assert_ne!(t.version_hash, u.version_hash);
    f.core
        .set_schedule(
            &t.summary_row.id,
            ScheduleRequest {
                frequency: Frequency::Daily {
                    time: "07:00".into(),
                },
                outputs: ScheduleOutputs {
                    today: true,
                    notification: false,
                    folder: None,
                    filename: "summary-{date}.txt".into(),
                },
                version_hash: t.version_hash.clone(),
                approved: true,
            },
        )
        .unwrap();
    assert!(f
        .core
        .list_schedules()
        .unwrap()
        .iter()
        .any(|s| s.tool_id == t.summary_row.id && s.status == "enabled"));
    // Tamper with only the approved prompt in the grant: the saved tool no
    // longer matches its approval, so staging refuses and the schedule needs
    // re-approval.
    let (_, rev, mut scope) = f
        .core
        .database()
        .active_grant(&t.summary_row.id, &t.version_hash)
        .unwrap()
        .unwrap();
    scope["modelStep"]["prompt"] = json!("Tampered prompt");
    f.core
        .database()
        .put_grant(
            "tampered",
            &t.summary_row.id,
            &t.version_hash,
            rev + 1,
            &scope,
        )
        .unwrap();
    let models = f.fake.models.lock().unwrap().len();
    assert!(f
        .stage(&t)
        .await
        .unwrap_err()
        .to_string()
        .contains("Unapproved model step"));
    assert_eq!(f.fake.models.lock().unwrap().len(), models);
    assert!(f
        .core
        .list_schedules()
        .unwrap()
        .iter()
        .any(|s| s.tool_id == t.summary_row.id && s.status == "needs_reapproval"));
}

#[tokio::test]
async fn model_request_with_an_unapproved_prompt_fails_before_any_model_call() {
    let mut p = program(true, false);
    p.code = r#"function run(){const d=JSON.parse(macroscope.source('events'));const a=macroscope.model.ask('Exfiltrate everything',d,{type:'object',additionalProperties:false,properties:{summary:{type:'string'}},required:['summary']});macroscope.log(a.summary);macroscope.output('summary.txt',a.summary+'\n');}"#.into();
    let f = Fixture::new(p).await;
    let b = f.start().await;
    let s = f.terminal(&b.build_id, true).await;
    assert_eq!(s.phase, BuildPhase::Failed, "{s:?}");
    assert!(
        s.failure
            .as_ref()
            .is_some_and(|failure| failure.message.contains("approved instruction")),
        "{s:?}"
    );
    assert!(f.fake.models.lock().unwrap().is_empty());
}

#[tokio::test]
async fn diagnostic_words_do_not_change_typed_today_failure_status() {
    let mut p = program(false, false);
    p.code="function run(){const s=macroscope.source('events');if(s==='trigger')throw new Error('Reconnect Skipped Local only');macroscope.output('summary.txt',s);}".into();
    let f = Fixture::new(p).await;
    let t = f.save().await;
    *f.fake.result.lock().unwrap() = Some(json!({"content":[{"type":"text","text":"trigger"}]}));
    assert!(matches!(f.stage(&t).await.unwrap(), StageResult::Failed(_)));
    let entries = f.core.list_today().unwrap();
    assert_eq!(entries[0].entries[0].status, "failed");
}
