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
        second_body_raw.contains(FETCHED_CANARY),
        "correction carries a short excerpt of the fetched public page"
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
    assert!(
        correction_outbound
            .payload_text
            .as_deref()
            .unwrap_or_default()
            .contains(FETCHED_CANARY),
        "the outbound record shows exactly what was sent"
    );
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
    assert_eq!(model_calls.len(), 4, "initial build plus at most three corrections");
    assert_eq!(counter.load(Ordering::SeqCst), 4);
    let second_body_raw = String::from_utf8_lossy(&model_calls[1].body);
    assert!(!second_body_raw.contains(INPUT_CANARY));
    assert!(second_body_raw.contains(FETCHED_CANARY));
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

#[tokio::test]
async fn runtime_correction_contains_safe_detail_without_fixture_values_in_stored_events() {
    // Cover an unquoted arbitrary Error payload and a native TypeError whose
    // quoted property name is data, from both selected rows and fetched text.
    for (expression, expected) in [
        ("throw new Error(row.email)", "Error"),
        (
            "const missing = undefined; missing[row.email].trim()",
            "TypeError",
        ),
        ("throw new Error(macroscope.source('forecast'))", "Error"),
        (
            "const missing = undefined; missing[macroscope.source('forecast')].trim()",
            "TypeError",
        ),
        ("groupBy([])", "ReferenceError"),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let model_server = MockServer::start().await;
        let fetch_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/forecast"))
            .respond_with(ResponseTemplate::new(200).set_body_string(FETCHED_CANARY))
            .mount(&fetch_server)
            .await;
        let bad_code = format!("function run() {{\nconst row = macroscope.csv.parse(macroscope.input('input_csv'))[0];\n{expression};\n}}");
        let bad = program_for(&fetch_server.uri(), &bad_code);
        // Both attempts fail so the stored failure event also contains details.
        let counter = mount_model(&model_server, bad.clone(), bad).await;
        let core = core_at(temp.path(), model_server.uri());
        let source = temp.path().join("selected.log");
        std::fs::write(&source, format!("email\n{INPUT_CANARY}@example.invalid\n")).unwrap();
        let file = core.register_picked_file(&source).unwrap();
        let state = build_with_correction(&core, &file).await;
        assert_eq!(state.phase, BuildPhase::Failed, "{expression}: {state:?}");
        assert_eq!(counter.load(Ordering::SeqCst), 4);
        let detail = state
            .checks
            .iter()
            .find(|c| c.id == "sandbox")
            .unwrap()
            .detail
            .as_deref()
            .unwrap();
        assert!(
            detail.contains(&format!("{expected} at line 3")),
            "{expression}: {detail}"
        );
        if expected == "ReferenceError" {
            assert!(detail.contains("groupBy is not defined"), "{detail}");
        }
        if expected == "TypeError" {
            assert!(
                detail.contains("cannot read property … of undefined"),
                "{detail}"
            );
        }
        assert!(
            state.events.iter().any(|e| e.message.contains(detail)),
            "{state:?}"
        );
        let requests = model_server.received_requests().await.unwrap();
        let calls = model_requests(&requests);
        assert_eq!(calls.len(), 4);
        let packet: serde_json::Value = serde_json::from_slice(&calls[1].body).unwrap();
        let correction: serde_json::Value =
            serde_json::from_str(packet["input"][1]["content"].as_str().unwrap()).unwrap();
        assert_eq!(correction["failed_checks"][0]["detail"], detail);
        // Read back persisted events/checks rather than only the returned state.
        let stored = core.build_state(&state.build_id).unwrap();
        // The correction request and its outbound record may carry the public
        // web excerpt; the stored build state must not.
        let stored_json = serde_json::to_string(&stored).unwrap();
        assert!(!stored_json.contains(FETCHED_CANARY), "{stored_json}");
        for payload in [
            String::from_utf8_lossy(&calls[1].body).into_owned(),
            stored_json,
            serde_json::to_string(&core.list_outbound().unwrap()).unwrap(),
        ] {
            for secret in [INPUT_CANARY, "@example.invalid"] {
                assert!(
                    !payload.contains(secret),
                    "{expression}: fixture value in persisted/correction payload: {payload}"
                );
            }
        }
    }
}

struct SourceCorrection {
    _temp: tempfile::TempDir,
    core: AppCore,
    model: MockServer,
    web: MockServer,
}

impl SourceCorrection {
    async fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let model = MockServer::start().await;
        let web = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "high": 21, "low": 13, "private": FETCHED_CANARY,
            })))
            .mount(&web)
            .await;
        let core = core_at(temp.path(), model.uri());
        Self {
            _temp: temp,
            core,
            model,
            web,
        }
    }

    fn program(&self, declared: &str, read: &str) -> Program {
        let mut program = program_for(&self.web.uri(), "");
        program.web_sources[0] = WebSource {
            name: declared.into(),
            url: format!("{}/{declared}", self.web.uri()),
        };
        program.code = format!(
            r#"function run() {{
                const data = JSON.parse(macroscope.source("{read}"));
                macroscope.output("weather.txt", "Gisborne high : " + data.high + ", low : " + data.low);
            }}"#
        );
        program
    }

    async fn build(&self, first: Program, corrected: Program) -> BuildState {
        let count = mount_model(&self.model, first, corrected).await;
        self.core
            .save_api_key("mk-source-correction-test-1234567890")
            .await
            .unwrap();
        let draft = self
            .core
            .draft_task_no_input("Gisborne weather summary to weather.txt")
            .unwrap();
        let draft = self
            .core
            .update_task(
                &draft.draft_id,
                TaskKind::Program,
                "weather.txt",
                Some(EXAMPLE.into()),
            )
            .unwrap();
        let started = self.core.start_build(&draft.draft_id, true).await.unwrap();
        let state = wait(&self.core, &started.build_id).await;
        assert!((2..=4).contains(&count.load(Ordering::SeqCst)), "initial build plus one to three corrections");
        state
    }

    fn web_outbound(&self) -> Vec<OutboundRecord> {
        self.core
            .list_outbound()
            .unwrap()
            .into_iter()
            .filter(|o| o.purpose == "Web source")
            .collect()
    }
}

#[tokio::test]
async fn correction_fetches_renamed_source_and_reports_unknown_read_without_fetched_data() {
    let f = SourceCorrection::new().await;
    let first = f.program("weather", "gisborne_weather");
    let corrected = f.program("gisborne_weather", "gisborne_weather");
    let state = f.build(first, corrected).await;
    assert_eq!(state.phase, BuildPhase::Ready, "{state:?}");
    assert!(state.checks.iter().all(|c| c.passed), "{state:?}");
    assert_eq!(
        state.program.unwrap().web_sources[0].name,
        "gisborne_weather"
    );
    let outbound = f.web_outbound();
    assert_eq!(
        outbound.len(),
        2,
        "a second outbound record for the new source"
    );
    for name in ["weather", "gisborne_weather"] {
        assert!(outbound.iter().any(|o| o
            .payload_text
            .as_deref()
            .unwrap()
            .contains(&format!("/{name}"))));
    }
    let requests = f.web.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].url.path(), "/weather");
    assert_eq!(requests[1].url.path(), "/gisborne_weather");
    let requests = f.model.received_requests().await.unwrap();
    let calls = model_requests(&requests);
    let packet: serde_json::Value = serde_json::from_slice(&calls[1].body).unwrap();
    let correction: serde_json::Value =
        serde_json::from_str(packet["input"][1]["content"].as_str().unwrap()).unwrap();
    assert_eq!(correction["failed_checks"][0]["id"], "source_names");
    assert_eq!(
        correction["failed_checks"][0]["detail"],
        "The program reads source 'gisborne_weather' but declares only: weather"
    );
    let raw = String::from_utf8_lossy(&calls[1].body);
    assert!(raw.contains(FETCHED_CANARY), "{raw}");
    let initial: serde_json::Value = serde_json::from_slice(&calls[0].body).unwrap();
    assert!(initial["input"][1]["content"].as_str().unwrap().contains(
        "Every name passed to macroscope.source(name) must exactly equal a declared web_sources[].name"
    ));
}

#[tokio::test]
async fn correction_reuses_identical_sources_without_second_fetch() {
    let f = SourceCorrection::new().await;
    let first = f.program("weather", "gisborne_weather");
    let corrected = f.program("weather", "weather");
    let state = f.build(first, corrected).await;
    assert_eq!(state.phase, BuildPhase::Ready, "{state:?}");
    assert_eq!(f.web_outbound().len(), 1);
    assert_eq!(f.web.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn correction_rejects_unsafe_sources_before_any_corrected_fetch() {
    for (url, expected) in [
        ("http://example.invalid/weather", "URL must use https."),
        (
            "https://example.invalid/weather?api_key=SECRET_CANARY",
            "Web sources can't include keys or tokens in the URL",
        ),
    ] {
        let f = SourceCorrection::new().await;
        let first = f.program("weather", "gisborne_weather");
        let mut corrected = f.program("gisborne_weather", "gisborne_weather");
        // A valid new source comes first: validate the entire list before
        // fetching anything, even when only a later source is invalid.
        corrected.web_sources.push(WebSource {
            name: "unsafe".into(),
            url: url.into(),
        });
        let state = f.build(first, corrected).await;
        assert_eq!(state.phase, BuildPhase::Failed, "{state:?}");
        let failure = state.failure.unwrap();
        assert_eq!(failure.kind, BuildFailureKind::Checks);
        assert!(failure.message.contains(expected), "{}", failure.message);
        assert!(!failure.message.contains("SECRET_CANARY"));
        assert_eq!(
            f.web_outbound().len(),
            1,
            "only the initial fetch is logged"
        );
        let requests = f.web.received_requests().await.unwrap();
        assert_eq!(
            requests.len(),
            1,
            "nothing fetched for the corrected program"
        );
        assert_eq!(requests[0].url.path(), "/weather");
    }
}

#[tokio::test]
async fn correction_fetches_changed_url_even_when_source_name_is_unchanged() {
    let f = SourceCorrection::new().await;
    let first = f.program("weather", "gisborne_weather");
    let mut corrected = f.program("weather", "weather");
    corrected.web_sources[0].url = format!("{}/updated-weather", f.web.uri());
    let state = f.build(first, corrected).await;
    assert_eq!(state.phase, BuildPhase::Ready, "{state:?}");
    assert_eq!(f.web_outbound().len(), 2);
    let requests = f.web.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[1].url.path(), "/updated-weather");
}

#[tokio::test]
async fn correction_cannot_read_a_removed_source_from_the_previous_snapshot() {
    let f = SourceCorrection::new().await;
    let first = f.program("weather", "gisborne_weather");
    let mut corrected = f.program("weather", "weather");
    corrected.web_sources.clear();
    let state = f.build(first, corrected).await;
    assert_eq!(state.phase, BuildPhase::Failed, "{state:?}");
    assert!(state.checks.iter().any(|c| c.id == "source_names"
        && !c.passed
        && c.detail.as_deref()
            == Some("The program reads source 'weather' but declares only: (none)")));
    assert_eq!(f.web_outbound().len(), 1);
    assert_eq!(f.web.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn unknown_source_diagnostic_redacts_a_name_computed_from_fetched_data() {
    let f = SourceCorrection::new().await;
    let mut first = f.program("weather", "weather");
    first.code = r#"function run() {
        const data = JSON.parse(macroscope.source('weather'));
        macroscope.source(data.private);
    }"#
    .into();
    let corrected = f.program("weather", "weather");
    let state = f.build(first, corrected).await;
    assert_eq!(state.phase, BuildPhase::Ready, "{state:?}");
    let requests = f.model.received_requests().await.unwrap();
    let calls = model_requests(&requests);
    let raw = String::from_utf8_lossy(&calls[1].body);
    assert!(raw.contains("The program reads an undeclared source but declares only: weather"));
    assert!(raw.contains(FETCHED_CANARY), "{raw}");
    assert_eq!(f.web_outbound().len(), 1);
}

/// A web-source program whose checks all pass still gets one model review of
/// its real output against an excerpt of the fetched page.
#[tokio::test]
async fn passing_web_program_gets_one_review_round_with_a_page_excerpt() {
    let f = SourceCorrection::new().await;
    let first = f.program("weather", "weather");
    let corrected = f.program("weather", "weather");
    let state = f.build(first, corrected).await;
    assert_eq!(state.phase, BuildPhase::Ready, "{state:?}");
    let requests = f.model.received_requests().await.unwrap();
    let calls = model_requests(&requests);
    assert_eq!(calls.len(), 2, "one build call and one review call");
    let packet: serde_json::Value = serde_json::from_slice(&calls[1].body).unwrap();
    let review: serde_json::Value =
        serde_json::from_str(packet["input"][1]["content"].as_str().unwrap()).unwrap();
    assert_eq!(review["failed_checks"][0]["id"], "review");
    assert!(review["fetched_web_samples"]["weather"]
        .as_str()
        .unwrap()
        .contains(FETCHED_CANARY));
    assert!(review["current_output"]
        .as_str()
        .unwrap()
        .contains("Gisborne high : 21"));
    let summary = &review["fetched_web_summaries"]["weather"];
    assert!(summary["total_chars"].as_u64().unwrap() > 0, "{summary}");
    assert_eq!(summary["excerpt_truncated"], false);
    assert!(review["correction_instruction"]
        .as_str()
        .unwrap()
        .contains("covers everything the task asks about"));
}

/// A review can only improve a passing build: when the reviewed program fails
/// the checks, the version that passed is kept and the build stays Ready.
#[tokio::test]
async fn failing_review_change_keeps_the_passing_program() {
    let f = SourceCorrection::new().await;
    let first = f.program("weather", "weather");
    let worse = f.program("weather", "not_declared");
    let state = f.build(first.clone(), worse).await;
    assert_eq!(state.phase, BuildPhase::Ready, "{state:?}");
    assert_eq!(state.program.as_ref().unwrap().code, first.code);
    assert!(state.checks.iter().all(|c| c.passed), "{state:?}");
    assert!(state
        .events
        .iter()
        .any(|e| e.message.contains("version that passed was kept")));
}
