//! One automatic correction round for program builds: a failed example check
//! triggers a single follow-up model call with shapes only, then the build
//! is Ready only if the corrected program passes.
//!
//! The fake model endpoint returns a bad program first and a corrected program
//! second; the fetch stand-in serves host-fetched web data. Both the input
//! rows and the fetched body use canary strings that must never appear in the
//! correction request.
#![cfg(feature = "synthetic-runtime-tests")]
use macroscope_app_core::{dto::*, AppCore, CoreConfig, NativeMuse};
use macroscope_builder::{Program, WebSource};
use macroscope_vault::MemoryStore;
use serde_json::json;
use std::{
    path::Path,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

const INPUT_CANARY: &str = "INPUT_CANARY_7f3a9c2e1b4d";
const FETCHED_CANARY: &str = "FETCHED_CANARY_9e8d7c6b5a4f";
const EXAMPLE: &str = "Gisborne high : 20, low : 12";
const BAD_CODE: &str =
    r#"function run() { macroscope.output("weather.txt", "Gisborne, high: 19.7, low: 13.5"); }"#;
const GOOD_CODE: &str =
    r#"function run() { macroscope.output("weather.txt", "Gisborne high : 19.7, low : 13.5"); }"#;
const STILL_BAD_CODE: &str =
    r#"function run() { macroscope.output("weather.txt", "Gisborne, high: 18.1, low: 11.9"); }"#;

fn runner_path() -> std::path::PathBuf {
    std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("macroscope-runner")
}

fn core_at(root: &Path, endpoint: String) -> AppCore {
    let core = AppCore::new(CoreConfig {
        data_dir: root.join("data"),
        vault: Arc::new(MemoryStore::default()),
        gateway_base_url: endpoint,
        home: root.join("empty-home"),
        muse: Arc::new(NativeMuse {
            home: root.join("empty-home"),
        }),
        app_version: "test".into(),
        platform: "macos".into(),
        now_ms: CoreConfig::system_clock(),
    })
    .unwrap();
    core.use_synthetic_runtime_for_tests(
        &runner_path(),
        Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../runner/profile.sb")),
    );
    core.set_allow_insecure_web_sources_for_tests(true);
    core
}

fn responses_body(program: &Program) -> serde_json::Value {
    json!({
        "status": "completed",
        "output": [{
            "type": "message",
            "content": [{
                "type": "output_text",
                "text": serde_json::to_string(program).unwrap(),
            }],
        }],
        "usage": {"input_tokens": 3000, "output_tokens": 400},
    })
}

fn program_for(fetch_base: &str, code: &str) -> Program {
    Program {
        connection_sources: vec![],
        model_step: None,
        summary: "Gisborne weather".into(),
        output_name: "weather.txt".into(),
        code: code.into(),
        web_sources: vec![WebSource {
            name: "forecast".into(),
            url: format!("{fetch_base}/forecast"),
        }],
    }
}

async fn wait(core: &AppCore, id: &str) -> BuildState {
    for _ in 0..1000 {
        let state = core.build_state(id).unwrap();
        if state.phase.is_terminal() {
            return state;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("correction build timed out");
}

async fn mount_model(
    model_server: &MockServer,
    first: Program,
    second: Program,
) -> Arc<AtomicUsize> {
    let count = Arc::new(AtomicUsize::new(0));
    let counter = count.clone();
    let first_body = responses_body(&first);
    let second_body = responses_body(&second);
    Mock::given(method("POST"))
        .and(path("/responses"))
        .respond_with(move |_: &wiremock::Request| {
            let n = counter.fetch_add(1, Ordering::SeqCst);
            if n == 0 {
                ResponseTemplate::new(200).set_body_json(first_body.clone())
            } else {
                ResponseTemplate::new(200).set_body_json(second_body.clone())
            }
        })
        .mount(model_server)
        .await;
    count
}

async fn build_with_correction(core: &AppCore, input: &FileHandle) -> BuildState {
    core.save_api_key("mk-correction-test-canary-1234567890")
        .await
        .unwrap();
    let draft = core
        .draft_task(&input.handle_id, "Gisborne weather summary to weather.txt")
        .unwrap();
    assert_eq!(draft.kind, TaskKind::Program);
    let draft = core
        .update_task(
            &draft.draft_id,
            TaskKind::Program,
            "weather.txt",
            Some(EXAMPLE.into()),
        )
        .unwrap();
    let started = core.start_build(&draft.draft_id, true).await.unwrap();
    wait(core, &started.build_id).await
}

fn model_requests(requests: &[wiremock::Request]) -> Vec<&wiremock::Request> {
    requests
        .iter()
        .filter(|r| r.url.path() == "/responses")
        .collect()
}

#[tokio::test]
async fn correction_passes_on_second_attempt_ready_with_both_events() {
    let temp = tempfile::tempdir().unwrap();
    let model_server = MockServer::start().await;
    let fetch_server = MockServer::start().await;
    let fetch_base = fetch_server.uri();
    let fetched_body = format!(r#"{{"t":21,"s":"{FETCHED_CANARY}"}}"#);
    Mock::given(method("GET"))
        .and(path("/forecast"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fetched_body))
        .mount(&fetch_server)
        .await;
    let bad = program_for(&fetch_base, BAD_CODE);
    let good = program_for(&fetch_base, GOOD_CODE);
    let count2 = mount_model(&model_server, bad, good).await;
    let core = core_at(temp.path(), model_server.uri());
    let source = temp.path().join("selected.log");
    std::fs::write(
        &source,
        format!("picked rows {INPUT_CANARY} stay local\nsecond line\n"),
    )
    .unwrap();
    let file = core.register_picked_file(&source).unwrap();
    let state = build_with_correction(&core, &file).await;
    assert_eq!(state.phase, BuildPhase::Ready, "{state:?}");
    assert!(state.checks.iter().all(|c| c.passed), "{:#?}", state.checks);
    let messages: Vec<&str> = state.events.iter().map(|e| e.message.as_str()).collect();
    assert!(
        messages
            .iter()
            .any(|m| m.contains("Checks failed; asking the model to correct the program")),
        "{messages:?}"
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("Checking the corrected program")),
        "{messages:?}"
    );
    let requests = model_server.received_requests().await.unwrap();
    let model_calls = model_requests(&requests);
    assert_eq!(model_calls.len(), 2, "exactly one correction round");
    assert_eq!(count2.load(Ordering::SeqCst), 2);
    let wire_body: serde_json::Value =
        serde_json::from_slice(&model_calls[1].body).expect("wire body is JSON");
    let user_content = wire_body["input"][1]["content"]
        .as_str()
        .expect("user content");
    let correction_json: serde_json::Value =
        serde_json::from_str(user_content).expect("correction is JSON");
    let previous = correction_json["previous_program_code"]
        .as_str()
        .expect("previous code");
    assert!(
        previous.contains("Gisborne, high:"),
        "correction carries the previous program code"
    );
    let failed = correction_json["failed_checks"]
        .as_array()
        .expect("failed checks");
    assert!(!failed.is_empty());
    assert!(
        failed.iter().any(|c| c["id"] == "example"),
        "correction names the failed example check"
    );
    let instruction = correction_json["correction_instruction"]
        .as_str()
        .expect("instruction");
    assert!(
        instruction.contains("corrected program") && instruction.contains("same schema"),
        "correction instructs a corrected program in the same schema"
    );
    let second_body_raw = String::from_utf8_lossy(&model_calls[1].body);
    assert!(
        !second_body_raw.contains(INPUT_CANARY),
        "correction must not contain input rows"
    );
    assert!(
        !second_body_raw.contains(FETCHED_CANARY),
        "correction must not contain fetched data"
    );
    assert!(
        user_content.contains('#'),
        "shapes only in correction details"
    );
    let outbound = core.list_outbound().unwrap();
    assert!(
        outbound
            .iter()
            .any(|o| o.purpose == "Program correction packet"),
        "{outbound:?}"
    );
    let correction_outbound = outbound
        .iter()
        .find(|o| o.purpose == "Program correction packet")
        .unwrap();
    assert!(!correction_outbound
        .payload_text
        .as_deref()
        .unwrap_or_default()
        .contains(INPUT_CANARY));
    assert!(!correction_outbound
        .payload_text
        .as_deref()
        .unwrap_or_default()
        .contains(FETCHED_CANARY));
    let model_costs: i64 = outbound
        .iter()
        .filter(|o| o.purpose.contains("packet"))
        .filter_map(|o| o.cost_micros)
        .sum();
    assert!(model_costs > 0);
    assert_eq!(state.spent_micros, Some(model_costs));
}

#[tokio::test]
async fn correction_failure_stays_failed_with_second_details() {
    let temp = tempfile::tempdir().unwrap();
    let model_server = MockServer::start().await;
    let fetch_server = MockServer::start().await;
    let fetch_base = fetch_server.uri();
    let fetched_body = format!(r#"{{"t":21,"s":"{FETCHED_CANARY}"}}"#);
    Mock::given(method("GET"))
        .and(path("/forecast"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fetched_body))
        .mount(&fetch_server)
        .await;
    let bad = program_for(&fetch_base, BAD_CODE);
    let still_bad = program_for(&fetch_base, STILL_BAD_CODE);
    let counter = mount_model(&model_server, bad, still_bad).await;
    let core = core_at(temp.path(), model_server.uri());
    let source = temp.path().join("selected.log");
    std::fs::write(
        &source,
        format!("picked rows {INPUT_CANARY} stay local\nsecond line\n"),
    )
    .unwrap();
    let file = core.register_picked_file(&source).unwrap();
    let state = build_with_correction(&core, &file).await;
    assert_eq!(state.phase, BuildPhase::Failed, "{state:?}");
    assert!(
        state.checks.iter().any(|c| !c.passed),
        "{:#?}",
        state.checks
    );
    let messages: Vec<&str> = state.events.iter().map(|e| e.message.as_str()).collect();
    assert!(
        messages
            .iter()
            .any(|m| m.contains("Checks failed; asking the model to correct the program")),
        "{messages:?}"
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("Checking the corrected program")),
        "{messages:?}"
    );
    let requests = model_server.received_requests().await.unwrap();
    let model_calls = model_requests(&requests);
    assert_eq!(model_calls.len(), 2, "at most one correction round");
    assert_eq!(counter.load(Ordering::SeqCst), 2);
    let second_body_raw = String::from_utf8_lossy(&model_calls[1].body);
    assert!(!second_body_raw.contains(INPUT_CANARY));
    assert!(!second_body_raw.contains(FETCHED_CANARY));
    let failure = state.failure.expect("failure");
    assert_eq!(failure.kind, BuildFailureKind::Checks);
    let example_detail = state
        .checks
        .iter()
        .find(|c| c.id == "example")
        .expect("example check")
        .detail
        .clone()
        .unwrap_or_default();
    assert!(
        failure
            .message
            .contains(&example_detail[..example_detail.len().min(40)]),
        "failure carries the second attempt's details: {}",
        failure.message
    );
}
