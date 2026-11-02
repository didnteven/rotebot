//! Authenticated local chat-completions stand-in; no owner endpoint or credential is used.
use macroscope_app_core::{dto::*, AppCore, CoreConfig, NativeMuse};
use macroscope_vault::{MemoryStore, SecretRef, SecretStore};
use serde_json::{json, Value};
use std::{
    path::Path,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, Request, Respond, ResponseTemplate,
};
const TOKEN: &str = "task25-canary-id.PRIVATE-secret-8842001";
const MODEL: &str = "meta-models/Muse-Spark-Open-816B-A42B";

#[derive(Clone)]
struct Fake {
    mode: Arc<AtomicUsize>,
    builds: Arc<AtomicUsize>,
}
fn answer(value: Value) -> Value {
    json!({"choices":[{"message":{"content":value.to_string()},"finish_reason":"stop"}],"usage":{"prompt_tokens":123,"completion_tokens":45,"reasoning_tokens":6}})
}
fn mapping() -> Value {
    serde_json::from_str::<Value>(include_str!("../../../fixtures/orders/task.json")).unwrap()
        ["mapping"]
        .clone()
}
impl Respond for Fake {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        assert_eq!(
            request.headers.get("authorization").unwrap(),
            &format!("Bearer {TOKEN}")
        );
        let body: Value = serde_json::from_slice(&request.body).unwrap();
        assert_eq!(body["model"], MODEL);
        assert_eq!(body["response_format"]["type"], "json_schema");
        assert_eq!(body["response_format"]["json_schema"]["strict"], true);
        assert_eq!(body["temperature"], 0.2);
        assert_eq!(body["reasoning_effort"], "medium");
        assert!(!body.to_string().contains(TOKEN));
        let mode = self.mode.load(Ordering::SeqCst);
        if mode == 1 {
            return ResponseTemplate::new(401).set_body_string(TOKEN);
        }
        if mode == 2 {
            return ResponseTemplate::new(400).set_body_string(TOKEN);
        }
        let name = body["response_format"]["json_schema"]["name"]
            .as_str()
            .unwrap();
        let value = if name == "endpoint_test" {
            json!({"ok":true})
        } else if name == "model_step" {
            json!({"summary":"A safe summary"})
        } else if name == "program" {
            let n = self.builds.fetch_add(1, Ordering::SeqCst);
            let code = if mode == 7 {
                "function run(){const a=macroscope.model.ask({note:'invented'}, {type:'object',additionalProperties:false,properties:{summary:{type:'string'}},required:['summary']});macroscope.output('summary.txt',a.summary+'\\n');}"
            } else if mode == 6 && n == 0 {
                "function run(){throw new Error('needs correction');}"
            } else {
                "function run(){macroscope.output('summary.txt','A safe summary\\n');}"
            };
            json!({"summary":"Write summary","output_name":"summary.txt","code":code,"web_sources":[],"connection_sources":[],"model_step":if mode==7 {json!({"purpose":"Write the summary","prompt":"Summarize the note"})} else {Value::Null}})
        } else {
            assert_eq!(
                body["response_format"]["json_schema"]["schema"],
                macroscope_builder::model_schema()
            );
            mapping()
        };
        let mut result = answer(value);
        if name == "endpoint_test" {
            // Reasoning models think before answering; a tiny limit truncates.
            assert!(body["max_tokens"].as_u64().unwrap() >= 1024);
            if mode == 10 {
                result["choices"][0]["finish_reason"] = json!("length");
                result["choices"][0]["message"]["content"] = json!("");
            }
        }
        if name != "endpoint_test" {
            if mode == 3 {
                result["choices"][0]["finish_reason"] = json!("length");
            }
            if mode == 4 {
                result["choices"][0]["message"]["content"] = Value::Null;
            }
            if mode == 5 {
                result["choices"][0]["message"]["content"] = json!("{not json");
            }
            if mode == 8 {
                result["choices"][0]["message"]["content"] = json!(json!({"output_columns":[],"on_error":"stop_and_explain","skip_header_rows":0,"formula_escape":true,"extra":TOKEN}).to_string());
            }
        }
        let response = ResponseTemplate::new(200).set_body_json(result);
        if mode == 9 {
            response.set_delay(std::time::Duration::from_secs(30))
        } else {
            response
        }
    }
}
fn core(root: &Path, vault: Arc<MemoryStore>) -> AppCore {
    AppCore::new(CoreConfig {
        data_dir: root.join("data"),
        vault,
        gateway_base_url: "https://api.meta.ai/v1".into(),
        home: root.join("empty-home"),
        muse: Arc::new(NativeMuse {
            home: root.join("empty-home"),
        }),
        app_version: "test".into(),
        platform: "macos".into(),
        now_ms: CoreConfig::system_clock(),
    })
    .unwrap()
}
struct Fixture {
    _tmp: tempfile::TempDir,
    core: AppCore,
    server: MockServer,
    fake: Fake,
    vault: Arc<MemoryStore>,
}
impl Fixture {
    async fn new() -> Self {
        let server = MockServer::start().await;
        let fake = Fake {
            mode: Arc::new(AtomicUsize::new(0)),
            builds: Arc::new(AtomicUsize::new(0)),
        };
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(fake.clone())
            .mount(&server)
            .await;
        let tmp = tempfile::tempdir().unwrap();
        let vault = Arc::new(MemoryStore::default());
        let core = core(tmp.path(), vault.clone());
        core.set_allow_insecure_web_sources_for_tests(true);
        core.save_custom_endpoint(&format!("{}/v1", server.uri()), MODEL)
            .await
            .unwrap();
        core.save_custom_endpoint_token(TOKEN).await.unwrap();
        Self {
            _tmp: tmp,
            core,
            server,
            fake,
            vault,
        }
    }
    async fn select(&self) {
        let status = self.core.test_custom_endpoint().await.unwrap();
        assert_eq!(status.custom_endpoint.last_test.unwrap().status, "ok");
        self.core
            .select_route(ModelRoute::CustomEndpoint)
            .await
            .unwrap();
    }
    fn draft(&self) -> TaskDraft {
        let csv = self.core.load_sample_csv().unwrap();
        self.core
            .draft_task(&csv.handle.handle_id, "Prepare the import file")
            .unwrap()
    }
    async fn build(&self) -> BuildState {
        let start = self
            .core
            .start_build(&self.draft().draft_id, true)
            .await
            .unwrap();
        wait(&self.core, &start.build_id).await
    }
}
async fn wait(core: &AppCore, id: &str) -> BuildState {
    tokio::time::timeout(std::time::Duration::from_secs(15), async {
        loop {
            let s = core.build_state(id).unwrap();
            if s.phase.is_terminal() {
                return s;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("build completes")
}
#[tokio::test]
async fn mapping_build_ready_strict_schema_usage_and_no_credential_in_host_surfaces() {
    let f = Fixture::new().await;
    f.select().await;
    let d = f.draft();
    let review = f.core.outbound_preview(&d.draft_id).await.unwrap();
    assert_eq!(review.destination, "127.0.0.1");
    assert_eq!(review.model, MODEL);
    assert_eq!(review.estimated_cost_micros, None);
    let start = f.core.start_build(&d.draft_id, true).await.unwrap();
    let mut events = f.core.subscribe_build(&start.build_id).unwrap();
    let state = wait(&f.core, &start.build_id).await;
    assert_eq!(state.phase, BuildPhase::Ready, "{:?}", state.failure);
    assert_eq!(state.spent_micros, None);
    assert!(state.route_label.contains("Muse-Spark-Open-816B-A42B"));
    assert!(state.route_label.contains("127.0.0.1"));
    let status = f.core.model_access().await;
    assert_eq!(status.custom_endpoint.calls_this_month, 2);
    assert_eq!(
        f.vault
            .get(&SecretRef::new("custom-endpoint-token").unwrap())
            .unwrap()
            .unwrap()
            .expose(),
        TOKEN.as_bytes()
    );
    let rows = f.core.list_outbound().unwrap();
    assert_eq!(rows.len(), 2);
    let row = rows.iter().find(|r| r.purpose == "Build").unwrap();
    assert_eq!(row.destination, "127.0.0.1");
    assert_eq!(row.route, ModelRoute::CustomEndpoint);
    assert_eq!(row.cost_micros, None);
    let metadata: Value = serde_json::from_str(row.payload_text.as_ref().unwrap()).unwrap();
    assert!(metadata["requestBytes"].as_u64().unwrap() > 100);
    assert!(metadata["responseBytes"].as_u64().unwrap() > 100);
    assert_eq!(metadata["usage"]["input_tokens"], 123);
    assert_eq!(metadata["usage"]["output_tokens"], 45);
    assert_eq!(metadata["usage"]["reasoning_tokens"], 6);
    for text in [
        json!(state).to_string(),
        json!(status).to_string(),
        json!(review).to_string(),
        json!(rows).to_string(),
        json!(f.core.list_activity().unwrap()).to_string(),
        format!("{:?}", f.core.privacy_overview().await.unwrap()),
    ] {
        assert!(!text.contains(TOKEN));
    }
    while let Ok(event) = events.try_recv() {
        assert!(!json!(event).to_string().contains(TOKEN));
    }
    let key = f
        .vault
        .get(&SecretRef::new("db-key").unwrap())
        .unwrap()
        .unwrap();
    let key: [u8; 32] = key.expose().try_into().unwrap();
    let db =
        macroscope_storage::db::Db::open(&f._tmp.path().join("data/db/state.db"), &key).unwrap();
    assert!(!db
        .setting("custom_endpoint")
        .unwrap()
        .unwrap()
        .to_string()
        .contains(TOKEN));
    assert!(!db
        .build(&start.build_id)
        .unwrap()
        .unwrap()
        .state
        .to_string()
        .contains(TOKEN));
    let requests = f.server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        metadata["requestBytes"].as_u64().unwrap(),
        requests[1].body.len() as u64
    );
}
#[test]
fn captured_process_logs_do_not_contain_token() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "mapping_build_ready_strict_schema_usage_and_no_credential_in_host_surfaces",
            "--nocapture",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains(TOKEN));
    assert!(!String::from_utf8_lossy(&output.stderr).contains(TOKEN));
}
#[tokio::test]
async fn selection_requires_configuration_token_and_successful_current_test() {
    let f = Fixture::new().await;
    assert!(f
        .core
        .select_route(ModelRoute::CustomEndpoint)
        .await
        .is_err());
    f.select().await;
    f.core
        .save_custom_endpoint_token("replacement-token")
        .await
        .unwrap();
    assert!(f
        .core
        .select_route(ModelRoute::CustomEndpoint)
        .await
        .is_err());
    f.core.save_custom_endpoint_token(TOKEN).await.unwrap();
    f.select().await;
    f.core
        .save_custom_endpoint(&format!("{}/v1", f.server.uri()), "changed-model")
        .await
        .unwrap();
    assert!(f
        .core
        .select_route(ModelRoute::CustomEndpoint)
        .await
        .is_err());
    let removed = f.core.remove_custom_endpoint().await.unwrap();
    assert!(!removed.custom_endpoint.token_present);
    assert!(removed.custom_endpoint.base_url.is_none());
    assert!(f
        .vault
        .get(&SecretRef::new("custom-endpoint-token").unwrap())
        .unwrap()
        .is_none());
}
#[tokio::test]
async fn monthly_cap_blocks_before_dispatch_and_survives_restart() {
    let f = Fixture::new().await;
    f.select().await;
    f.core
        .set_custom_endpoint_options(Some("Modal".into()), 1)
        .await
        .unwrap();
    let error = f
        .core
        .start_build(&f.draft().draft_id, true)
        .await
        .unwrap_err();
    assert!(error
        .to_string()
        .contains("monthly custom endpoint call cap reached"));
    assert_eq!(f.server.received_requests().await.unwrap().len(), 1);
    let reopened = core(f._tmp.path(), f.vault.clone());
    reopened.set_allow_insecure_web_sources_for_tests(true);
    assert_eq!(
        reopened
            .model_access()
            .await
            .custom_endpoint
            .calls_this_month,
        1
    );
    assert!(reopened
        .test_custom_endpoint()
        .await
        .unwrap_err()
        .to_string()
        .contains("call cap"));
    f.core.set_custom_endpoint_options(None, 2).await.unwrap();
    assert_eq!(f.build().await.phase, BuildPhase::Ready);
}
#[tokio::test]
async fn local_only_blocks_endpoint_testing_and_selected_custom_builds() {
    let f = Fixture::new().await;
    f.select().await;
    f.core.set_local_only(true).await.unwrap();
    assert!(f.core.test_custom_endpoint().await.is_err());
    assert!(f.core.start_build(&f.draft().draft_id, true).await.is_err());
    assert_eq!(f.server.received_requests().await.unwrap().len(), 1);
}
#[tokio::test]
async fn endpoint_test_reports_rejected_unsupported_and_unreachable_with_latency() {
    let f = Fixture::new().await;
    for (mode, expected) in [
        (1, "token rejected"),
        (2, "request rejected: check the URL and model name"),
        (10, "reply did not match the JSON schema or was cut off"),
        (0, "ok"),
    ] {
        f.fake.mode.store(mode, Ordering::SeqCst);
        let status = f.core.test_custom_endpoint().await.unwrap();
        let test = status.custom_endpoint.last_test.unwrap();
        assert_eq!(test.status, expected);
        assert!(test.latency_ms < 120000);
        if mode != 0 {
            assert!(f
                .core
                .select_route(ModelRoute::CustomEndpoint)
                .await
                .is_err());
        }
    }
    f.core
        .save_custom_endpoint("http://127.0.0.1:1/v1", MODEL)
        .await
        .unwrap();
    let test = f
        .core
        .test_custom_endpoint()
        .await
        .unwrap()
        .custom_endpoint
        .last_test
        .unwrap();
    assert_eq!(test.status, "unreachable");
}
#[tokio::test]
async fn truncated_missing_invalid_and_secret_echo_answers_are_malformed() {
    let f = Fixture::new().await;
    f.select().await;
    for mode in [3, 4, 5, 8] {
        f.fake.mode.store(mode, Ordering::SeqCst);
        let state = f.build().await;
        assert_eq!(state.phase, BuildPhase::Failed);
        assert_eq!(state.failure.unwrap().kind, BuildFailureKind::Malformed);
        assert!(!json!(f.core.list_outbound().unwrap())
            .to_string()
            .contains(TOKEN));
    }
    // An answer that is not schema JSON gets one correction round; a cut-off answer or one echoing
    // the key does not.
    assert_eq!(f.server.received_requests().await.unwrap().len(), 11);
}
#[tokio::test]
async fn rejects_insecure_query_userinfo_fragment_and_empty_model() {
    let tmp = tempfile::tempdir().unwrap();
    let c = core(tmp.path(), Arc::new(MemoryStore::default()));
    for url in [
        "http://example.com/v1",
        "http://127.0.0.1:8080/v1",
        "https://user:secret@example.com/v1",
        "https://@example.com/v1",
        "https://example.com/v1?q=secret",
        "https://example.com/v1?",
        "https://example.com/v1#fragment",
    ] {
        assert!(c.save_custom_endpoint(url, MODEL).await.is_err(), "{url}");
    }
    assert!(c
        .save_custom_endpoint("https://example.com/v1", "")
        .await
        .is_err());
    c.set_allow_insecure_web_sources_for_tests(true);
    assert!(c
        .save_custom_endpoint("http://localhost/v1", MODEL)
        .await
        .is_err());
    c.save_custom_endpoint("http://127.0.0.1:8080/v1", MODEL)
        .await
        .unwrap();
}
#[tokio::test]
async fn emergency_stop_cancels_an_inflight_endpoint_test_immediately() {
    let f = Fixture::new().await;
    f.fake.mode.store(9, Ordering::SeqCst);
    let core = f.core.clone();
    let task = tokio::spawn(async move { core.test_custom_endpoint().await });
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while f.server.received_requests().await.unwrap().is_empty() {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(1), f.core.emergency_stop())
        .await
        .unwrap()
        .unwrap();
    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(1), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap_err()
            .to_string()
            .contains("cancelled")
    );
}

#[cfg(feature = "synthetic-runtime-tests")]
mod runtime {
    use super::*;
    fn enable(f: &Fixture) {
        let path = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("macroscope-runner");
        f.core.use_synthetic_runtime_for_tests(
            &path,
            Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../runner/profile.sb")),
        );
    }
    fn draft(f: &Fixture) -> TaskDraft {
        let d = f.core.draft_task_no_input("Write a summary").unwrap();
        f.core
            .update_task(&d.draft_id, TaskKind::Program, "summary.txt", None)
            .unwrap()
    }
    #[tokio::test]
    async fn program_build_and_host_check_correction_use_the_custom_route() {
        let f = Fixture::new().await;
        enable(&f);
        f.select().await;
        f.fake.mode.store(6, Ordering::SeqCst);
        let start = f.core.start_build(&draft(&f).draft_id, true).await.unwrap();
        let state = wait(&f.core, &start.build_id).await;
        assert_eq!(state.phase, BuildPhase::Ready, "{:?}", state.failure);
        assert_eq!(f.fake.builds.load(Ordering::SeqCst), 2);
        assert!(state
            .events
            .iter()
            .any(|e| e.message.contains("Corrected candidate")));
        let requests = f.server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 3);
        let body: Value = serde_json::from_slice(&requests[2].body).unwrap();
        assert_eq!(
            body["response_format"]["json_schema"]["schema"],
            macroscope_builder::program_schema()
        );
        assert!(body["max_tokens"].as_u64().unwrap() >= 16384);
        assert_eq!(
            f.core.model_access().await.custom_endpoint.calls_this_month,
            3
        );
    }
    #[tokio::test]
    async fn model_steps_use_custom_route_and_both_caps_apply_before_dispatch() {
        let f = Fixture::new().await;
        enable(&f);
        f.select().await;
        f.fake.mode.store(7, Ordering::SeqCst);
        let start = f.core.start_build(&draft(&f).draft_id, true).await.unwrap();
        let candidate = tokio::time::timeout(std::time::Duration::from_secs(3), async {
            loop {
                let s = f.core.build_state(&start.build_id).unwrap();
                if s.program.is_some() {
                    break s;
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        f.core
            .approve_build_sources(&start.build_id, candidate.program.as_ref().unwrap())
            .unwrap();
        let state = wait(&f.core, &start.build_id).await;
        assert_eq!(state.phase, BuildPhase::Ready, "{:?}", state.failure);
        let tool = f.core.save_tool(&state.build_id, "Summary").await.unwrap();
        let folder = f.core.register_picked_dir(f._tmp.path()).unwrap();
        let before = f.server.received_requests().await.unwrap().len();
        let staged = f
            .core
            .stage_run_no_input(&tool.summary_row.id, &folder.handle_id, "summary.txt")
            .await
            .unwrap();
        let StageResult::Staged(staged) = staged else {
            panic!("expected staged model output")
        };
        f.core.discard_run(&staged.run_id).unwrap();
        assert_eq!(
            f.server.received_requests().await.unwrap().len(),
            before + 1
        );
        assert!(f
            .core
            .list_outbound()
            .unwrap()
            .iter()
            .any(|r| r.purpose == "Model step" && r.destination == "127.0.0.1"));
        f.core.set_model_step_cap(&tool.summary_row.id, 0).unwrap();
        assert!(f
            .core
            .stage_run_no_input(&tool.summary_row.id, &folder.handle_id, "blocked.txt")
            .await
            .unwrap_err()
            .to_string()
            .contains("model-step cap"));
        f.core.set_model_step_cap(&tool.summary_row.id, 60).unwrap();
        f.core.set_custom_endpoint_options(None, 0).await.unwrap();
        assert!(f
            .core
            .stage_run_no_input(&tool.summary_row.id, &folder.handle_id, "blocked.txt")
            .await
            .unwrap_err()
            .to_string()
            .contains("custom endpoint call cap"));
        assert_eq!(
            f.server.received_requests().await.unwrap().len(),
            before + 1
        );
    }
    #[tokio::test]
    async fn stop_cancels_an_inflight_build_without_waiting_for_timeout() {
        let f = Fixture::new().await;
        enable(&f);
        f.select().await;
        f.fake.mode.store(9, Ordering::SeqCst);
        let start = f.core.start_build(&draft(&f).draft_id, true).await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while f.server.received_requests().await.unwrap().len() < 2 {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        let state = f.core.stop_build(&start.build_id).unwrap();
        assert_eq!(state.phase, BuildPhase::Stopped);
        f.fake.mode.store(0, Ordering::SeqCst);
        let next = tokio::time::timeout(std::time::Duration::from_secs(1), async {
            loop {
                if let Ok(s) = f.core.start_build(&draft(&f).draft_id, true).await {
                    break s;
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("cancelled transport releases the worker immediately");
        assert_eq!(wait(&f.core, &next.build_id).await.phase, BuildPhase::Ready);
        assert_eq!(
            f.core.model_access().await.custom_endpoint.calls_this_month,
            3
        );
    }
}
#[tokio::test]
async fn modal_token_id_and_secret_are_stored_as_one_bearer() {
    let f = Fixture::new().await;
    let status = f
        .core
        .save_custom_endpoint_modal_token(" wk-abc123 ", "ws-def456\n")
        .await
        .unwrap();
    assert!(status.custom_endpoint.token_present);
    assert_eq!(
        f.vault
            .get(&SecretRef::new("custom-endpoint-token").unwrap())
            .unwrap()
            .unwrap()
            .expose(),
        b"wk-abc123.ws-def456"
    );
    for (id, secret) in [
        ("", "ws-x"),
        ("wk-x", ""),
        ("wk x", "ws-x"),
        ("wk-x", "ws.x"),
    ] {
        assert!(f
            .core
            .save_custom_endpoint_modal_token(id, secret)
            .await
            .is_err());
    }
}
