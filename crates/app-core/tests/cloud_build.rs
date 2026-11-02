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
    for _ in 0..1200 {
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
    // The logged payload is exactly what was sent: same message, same instructions.
    let user_msg = body["input"][1]["content"].as_str().unwrap();
    let logged = outbound[0].payload_text.as_deref().unwrap();
    let logged_json: Value = serde_json::from_str(
        logged
            .split_once("Message (synthetic rows only):\n")
            .unwrap()
            .1,
    )
    .unwrap();
    assert_eq!(
        logged_json,
        serde_json::from_str::<Value>(user_msg).unwrap()
    );
    assert!(logged.contains(body["input"][0]["content"].as_str().unwrap()));
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

#[tokio::test]
async fn local_only_during_a_request_cancels_and_acknowledges_the_barrier() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/responses"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(std::time::Duration::from_secs(30))
                .set_body_json(responses_body(&fixture_mapping())),
        )
        .mount(&server)
        .await;
    let tmp = tempfile::tempdir().unwrap();
    let core = core(tmp.path(), server.uri());
    core.save_api_key(KEY).await.unwrap();
    let preview = core.load_sample_csv().unwrap();
    let draft = core.draft_task(&preview.handle.handle_id, "").unwrap();
    let b = core.start_build(&draft.draft_id, true).await.unwrap();
    for _ in 0..1000 {
        if !server.received_requests().await.unwrap().is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
    let access = tokio::time::timeout(std::time::Duration::from_secs(5), core.set_local_only(true))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(access.egress, EgressState::LocalOnly);
    assert_eq!(
        core.build_state(&b.build_id).unwrap().phase,
        BuildPhase::Stopped
    );
    assert_eq!(
        core.set_local_only(false).await.unwrap().egress,
        EgressState::CloudAllowed
    );
}

#[tokio::test]
async fn local_only_refuses_api_key_network_checks() {
    let server = MockServer::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let core = core(tmp.path(), server.uri());
    core.save_api_key(KEY).await.unwrap();
    core.set_local_only(true).await.unwrap();
    assert!(core.test_api_key().await.is_err());
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn unknown_charges_keep_the_monthly_build_reservation_and_block_admission() {
    let tmp = tempfile::tempdir().unwrap();
    let core = core(tmp.path(), "http://127.0.0.1:9".into());
    core.save_api_key(KEY).await.unwrap();
    let input = core.load_sample_csv().unwrap();
    let draft = core.draft_task(&input.handle.handle_id, "").unwrap();
    core.database()
        .put_build(&macroscope_storage::db::BuildRow {
            id: "previous".into(),
            draft_id: draft.draft_id.clone(),
            route: "api_key".into(),
            phase: "failed".into(),
            reserved_micros: 24_000_000,
            spent_micros: None,
            state: json!({}),
            created_ms: core.now_ms(),
            updated_ms: core.now_ms(),
        })
        .unwrap();
    assert!(core.start_build(&draft.draft_id, true).await.is_err());
    assert!(core.list_outbound().unwrap().is_empty());
}

#[tokio::test]
async fn deletion_during_dispatch_awaits_workers_and_cannot_resurrect_history() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/responses"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(std::time::Duration::from_secs(30))
                .set_body_json(responses_body(&fixture_mapping())),
        )
        .mount(&server)
        .await;
    let tmp = tempfile::tempdir().unwrap();
    let core = core(tmp.path(), server.uri());
    core.save_api_key(KEY).await.unwrap();
    let preview = core.load_sample_csv().unwrap();
    let draft = core.draft_task(&preview.handle.handle_id, "").unwrap();
    let build = core.start_build(&draft.draft_id, true).await.unwrap();
    for _ in 0..1000 {
        if !server.received_requests().await.unwrap().is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        core.delete_data(DeleteOptions {
            include_tools: false,
        }),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(core.build_state(&build.build_id).is_err());
    assert!(core
        .delete_preview()
        .unwrap()
        .items
        .iter()
        .all(|i| i.count == 0));
    assert!(core.model_access().await.api_key.present);
    tokio::task::yield_now().await;
    assert!(core.list_activity().unwrap().is_empty());
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}
