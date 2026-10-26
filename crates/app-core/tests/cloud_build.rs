//! The API-key build route against a local stand-in for the Meta Responses API.

use macroscope_app_core::dto::*;
use macroscope_app_core::{AppCore, CoreConfig, NativeMuse};
use macroscope_vault::MemoryStore;
use serde_json::{json, Value};
use std::path::Path;
use std::sync::Arc;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

const KEY: &str = "mk-test-canary-cloud-0123456789";

fn core(dir: &Path, base: String) -> AppCore {
    AppCore::new(CoreConfig {
        data_dir: dir.join("data"),
        vault: Arc::new(MemoryStore::default()),
        gateway_base_url: base,
        home: dir.join("home"),
        muse: Arc::new(NativeMuse {
            home: dir.join("home"),
        }),
        app_version: "0.1.0".into(),
        platform: "macos".into(),
        now_ms: CoreConfig::system_clock(),
    })
    .unwrap()
}

fn responses_body(mapping: &Value) -> Value {
    json!({"status": "completed",
           "output": [{"type": "message", "content": [{"type": "output_text", "text": mapping.to_string()}]}],
           "usage": {"input_tokens": 3000, "output_tokens": 400,
                     "output_tokens_details": {"reasoning_tokens": 100}}})
}

fn fixture_mapping() -> Value {
    let task: Value =
        serde_json::from_str(include_str!("../../../fixtures/orders/task.json")).unwrap();
    task["mapping"].clone()
}

async fn build_with(mapping: Value) -> (BuildState, Vec<Request>, AppCore, tempfile::TempDir) {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/responses"))
        .and(header("authorization", format!("Bearer {KEY}").as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(responses_body(&mapping)))
        .mount(&server)
        .await;
    let tmp = tempfile::tempdir().unwrap();
    let core = core(tmp.path(), server.uri());
    core.save_api_key(KEY).await.unwrap();
    let preview = core.load_sample_csv().unwrap();
    let draft = core
        .draft_task(&preview.handle.handle_id, "Prepare the import file")
        .unwrap();
    let started = core.start_build(&draft.draft_id, true).await.unwrap();
    assert_eq!(started.route, ModelRoute::ApiKey);
    let mut state = core.build_state(&started.build_id).unwrap();
    for _ in 0..200 {
        state = core.build_state(&started.build_id).unwrap();
        if matches!(state.phase, BuildPhase::Ready | BuildPhase::Failed) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    let requests = server.received_requests().await.unwrap();
    (state, requests, core, tmp)
}

#[tokio::test]
async fn a_correct_model_answer_becomes_ready_with_cost_and_a_synthetic_packet() {
    let (state, requests, core, _tmp) = build_with(fixture_mapping()).await;
    assert_eq!(state.phase, BuildPhase::Ready, "{:?}", state.failure);
    assert!(
        state.spent_micros.unwrap() > 0,
        "usage priced in microdollars"
    );
    assert_eq!(requests.len(), 1);
    let body: Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(body["store"], false);
    assert_eq!(body["text"]["format"]["type"], "json_schema");
    assert!(body.get("response_format").is_none());
    let sent = String::from_utf8_lossy(&requests[0].body);
    for real in ["Ada Example", "ORD-001", "Baker, Taylor", "$1,240.00"] {
        assert!(
            !sent.contains(real),
            "real CSV content reached the provider: {real}"
        );
    }
    let outbound = core.list_outbound().unwrap();
    assert_eq!(outbound.len(), 1);
    assert_eq!(outbound[0].cost_micros, state.spent_micros);
    assert!(outbound[0].synthetic_only);
}

#[tokio::test]
async fn a_schema_valid_but_wrong_answer_is_stopped_by_host_checks() {
    let mut wrong = fixture_mapping();
    wrong["output_columns"][1]["transform"]["from"] = json!("dd_mm_yy");
    let (state, _, _, _tmp) = build_with(wrong).await;
    assert_eq!(state.phase, BuildPhase::Failed);
    let failure = state.failure.unwrap();
    assert_eq!(
        failure.kind,
        BuildFailureKind::Checks,
        "{}",
        failure.message
    );
    assert!(state
        .checks
        .iter()
        .any(|c| c.id == "selected_input" && !c.passed));
}
