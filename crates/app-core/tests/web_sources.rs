//! Web sources end to end (ADR 0004): a program with no input file reads a
//! host-fetched weather API through `macroscope.source(name)`.
//!
//! One blocking std HTTP server stands in for both the model endpoint and
//! the weather API, so the test can change responses, redirect, and
//! overserve. The worker runs through the explicit synthetic test runtime
//! (same pattern as program_flow): this suite exercises host fetching and
//! plan binding, not the Seatbelt boundary, which a nested test sandbox
//! cannot enter. The worker still gets no network access: every byte the
//! program sees was fetched by the host. Loopback binds must work; a bind
//! failure fails the test, it never skips.
use macroscope_app_core::{dto::*, AppCore, CoreConfig, NativeMuse};
use macroscope_builder::{Program, WebSource};
use macroscope_vault::MemoryStore;
use serde_json::json;
use std::{
    io::{Read, Write},
    net::TcpListener,
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

const CODE: &str = r#"function run() {
    const api = macroscope.source("forecast");
    const data = JSON.parse(api);
    const high = data.hourly.temperature_2m[0];
    macroscope.output("weather.txt", "Auckland high " + high + "C\n");
}"#;

struct StandInState {
    forecast: Vec<u8>,
    redirect_to: String,
    /// Canned `POST /responses` body for the fake model endpoint.
    model: Vec<u8>,
}

struct StandIn {
    base: String,
    _thread: std::thread::JoinHandle<()>,
}

/// Binds the stand-in server. A bind failure fails the test; it never skips.
fn start_stand_in(state: Arc<Mutex<StandInState>>) -> StandIn {
    let listener = TcpListener::bind("127.0.0.1:0").expect("stand-in bind failed");
    let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let thread = std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let state = state.clone();
            std::thread::spawn(move || serve(stream, &state));
        }
    });
    StandIn {
        base,
        _thread: thread,
    }
}

fn read_exact(stream: &mut std::net::TcpStream, raw: &mut Vec<u8>, len: usize) {
    let mut buf = [0u8; 4096];
    while raw.len() < len {
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => raw.extend_from_slice(&buf[..n]),
            Err(_) => break,
        }
    }
}

fn head_end(raw: &[u8]) -> Option<usize> {
    raw.windows(4).position(|w| w == b"\r\n\r\n").map(|i| i + 4)
}

fn serve(mut stream: std::net::TcpStream, state: &Arc<Mutex<StandInState>>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
    let mut raw = Vec::new();
    let mut buf = [0u8; 4096];
    // Headers first; the body (if any) follows per Content-Length.
    while head_end(&raw).is_none() && raw.len() < 65536 {
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => raw.extend_from_slice(&buf[..n]),
            Err(_) => break,
        }
    }
    let Some(head_len) = head_end(&raw) else {
        return;
    };
    // Consume the request body (model posts are small) so the close is clean.
    let head = String::from_utf8_lossy(&raw[..head_len]).into_owned();
    let content_len = head
        .lines()
        .skip(1)
        .filter_map(|l| l.split_once(':'))
        .find(|(k, _)| k.trim().eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.trim().parse::<usize>().ok())
        .unwrap_or(0)
        .min(8 * 1024 * 1024);
    read_exact(&mut stream, &mut raw, head_len + content_len);
    let mut parts = head.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let route = parts.next().unwrap_or("/").to_string();
    let state = state.lock().unwrap();
    let (status, extra, body): (&str, String, Vec<u8>) = match (method.as_str(), route.as_str()) {
        ("POST", "/responses") => ("200 OK", String::new(), state.model.clone()),
        ("GET", "/forecast") => ("200 OK", String::new(), state.forecast.clone()),
        ("GET", "/redirect") => (
            "302 Found",
            format!("Location: {}\r\n", state.redirect_to),
            Vec::new(),
        ),
        ("GET", "/big") => ("200 OK", String::new(), vec![b'x'; 3 * 1024 * 1024]),
        _ => ("404 Not Found", String::new(), b"no".to_vec()),
    };
    drop(state);
    let _ = stream.write_all(
        format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .as_bytes(),
    );
    let _ = stream.write_all(&body);
}

#[cfg(feature = "synthetic-runtime-tests")]
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
    // Explicit synthetic worker launches for tests (same pattern as
    // program_flow): this suite exercises host fetching and plan binding,
    // not the Seatbelt boundary. Production never selects synthetic
    // execution automatically.
    #[cfg(feature = "synthetic-runtime-tests")]
    core.use_synthetic_runtime_for_tests(
        &runner_path(),
        Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../runner/profile.sb")),
    );
    // A local stand-in API: http is allowed only through this test-only flag,
    // never on release code paths.
    core.set_allow_insecure_web_sources_for_tests(true);
    core
}

/// Canned fake-model answer serving `program` for the next build.
fn model_body_for(program: &Program) -> Vec<u8> {
    json!({
        "status":"completed", "output":[{"type":"message","content":[{
            "type":"output_text", "text":serde_json::to_string(program).unwrap()
        }]}], "usage":{"input_tokens":3000,"output_tokens":400}
    })
    .to_string()
    .into_bytes()
}

async fn wait(core: &AppCore, id: &str) -> BuildState {
    for _ in 0..1000 {
        let state = core.build_state(id).unwrap();
        if state.phase.is_terminal() {
            return state;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("web source build timed out");
}

fn program_for(base: &str, route: &str) -> Program {
    Program {
        summary: "Summarize the Auckland forecast".into(),
        output_name: "weather.txt".into(),
        code: CODE.into(),
        web_sources: vec![WebSource {
            name: "forecast".into(),
            url: format!("{base}{route}"),
        }],
    }
}

async fn build_weather(core: &AppCore) -> ToolDetail {
    core.save_api_key("mk-weather-test-canary-1234567890")
        .await
        .unwrap();
    let draft = core
        .draft_task_no_input(
            "Get today's forecast for Auckland from Open-Meteo and write a short summary to weather.txt",
        )
        .unwrap();
    assert!(draft.no_input);
    assert_eq!(draft.kind, TaskKind::Program);
    let draft = core
        .update_task(&draft.draft_id, TaskKind::Program, "weather.txt", None)
        .unwrap();
    assert!(draft.ready_to_build, "{:?}", draft.blockers);
    let started = core.start_build(&draft.draft_id, true).await.unwrap();
    let state = wait(core, &started.build_id).await;
    assert_eq!(state.phase, BuildPhase::Ready, "{state:?}");
    assert!(state.checks.iter().all(|c| c.passed), "{:#?}", state.checks);
    core.save_tool(&started.build_id, "Auckland weather")
        .await
        .unwrap()
}

fn out_dir(core: &AppCore, root: &Path, name: &str) -> FileHandle {
    let dir = root.join(name);
    std::fs::create_dir_all(&dir).unwrap();
    core.register_picked_dir(&dir).unwrap()
}

#[tokio::test]
async fn weather_build_save_stage_approve_commit_exact_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let weather_state = Arc::new(Mutex::new(StandInState {
        forecast: br#"{"hourly":{"temperature_2m":[21.5]}}"#.to_vec(),
        redirect_to: String::new(),
        model: Vec::new(),
    }));
    let server = start_stand_in(weather_state.clone());
    weather_state.lock().unwrap().model = model_body_for(&program_for(&server.base, "/forecast"));
    let core = core_at(temp.path(), server.base.clone());
    let tool = build_weather(&core).await;
    assert!(tool.no_input);
    assert_eq!(tool.program.as_ref().unwrap().web_sources.len(), 1);
    assert!(
        tool.access.iter().any(|a| a.contains("127.0.0.1")),
        "{:?}",
        tool.access
    );
    let folder = out_dir(&core, temp.path(), "Reports");
    let staged = match core
        .stage_run_no_input(&tool.summary_row.id, &folder.handle_id, "weather.txt")
        .await
        .unwrap()
    {
        StageResult::Staged(staged) => staged,
        result => panic!("{result:?}"),
    };
    assert_eq!(
        staged.preview_text.as_deref(),
        Some("Auckland high 21.5C"),
        "output derives from the fetched data"
    );
    let outbound = core.list_outbound().unwrap();
    assert!(
        outbound.iter().any(|o| o.purpose == "Web source"
            && o.destination == "127.0.0.1"
            && o.payload_text
                .as_deref()
                .is_some_and(|u| u.contains("/forecast"))),
        "{outbound:?}"
    );
    let result = core.commit_run(&staged.run_id, &staged.plan_hash).unwrap();
    let CommitResult::Committed(receipt) = result else {
        panic!("{result:?}")
    };
    let expected = b"Auckland high 21.5C\n";
    assert_eq!(std::fs::read(&receipt.output_path).unwrap(), expected);
    assert_eq!(receipt.plan_hash, staged.plan_hash);

    // Changed fetched data stages a new plan; the old approval no longer works.
    weather_state.lock().unwrap().forecast = br#"{"hourly":{"temperature_2m":[18.0]}}"#.to_vec();
    let folder2 = out_dir(&core, temp.path(), "Reports2");
    let restaged = match core
        .stage_run_no_input(&tool.summary_row.id, &folder2.handle_id, "weather.txt")
        .await
        .unwrap()
    {
        StageResult::Staged(staged) => staged,
        result => panic!("{result:?}"),
    };
    assert_ne!(
        restaged.plan_hash, staged.plan_hash,
        "plan hash covers the fetched data"
    );
    assert_eq!(restaged.preview_text.as_deref(), Some("Auckland high 18C"));
    // A stale approval no longer works, and the rejected run is discarded:
    // committing again needs a fresh stage (same pattern as program_flow).
    assert!(core
        .commit_run(&restaged.run_id, &staged.plan_hash)
        .is_err());
    let restaged = match core
        .stage_run_no_input(&tool.summary_row.id, &folder2.handle_id, "weather.txt")
        .await
        .unwrap()
    {
        StageResult::Staged(staged) => staged,
        result => panic!("{result:?}"),
    };
    let result = core
        .commit_run(&restaged.run_id, &restaged.plan_hash)
        .unwrap();
    assert!(matches!(result, CommitResult::Committed(_)));
}

#[tokio::test]
async fn unapproved_host_rejected_at_run() {
    let temp = tempfile::tempdir().unwrap();
    let weather_state = Arc::new(Mutex::new(StandInState {
        forecast: br#"{"hourly":{"temperature_2m":[21.5]}}"#.to_vec(),
        redirect_to: String::new(),
        model: Vec::new(),
    }));
    let server = start_stand_in(weather_state.clone());
    weather_state.lock().unwrap().model = model_body_for(&program_for(&server.base, "/forecast"));
    let core = core_at(temp.path(), server.base.clone());
    let tool = build_weather(&core).await;
    // A changed host needs a new build and approval: strip the grant's hosts.
    let revision = core
        .database()
        .next_grant_revision(&tool.summary_row.id)
        .unwrap();
    core.database()
        .put_grant(
            "grant_tampered",
            &tool.summary_row.id,
            &tool.version_hash,
            revision,
            &json!({"read": "selected_input", "create": "one_output_in_selected_root", "webHosts": []}),
        )
        .unwrap();
    let folder = out_dir(&core, temp.path(), "Reports");
    match core
        .stage_run_no_input(&tool.summary_row.id, &folder.handle_id, "weather.txt")
        .await
        .unwrap()
    {
        StageResult::Failed(envelope) => assert_eq!(
            envelope.failure,
            RunFailure::Revoked {
                message: "This tool's approved sites changed. Build it again and approve the sites before running.".into()
            }
        ),
        staged => panic!("unapproved host staged: {staged:?}"),
    }
}

#[tokio::test]
async fn redirect_to_another_host_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let weather_state = Arc::new(Mutex::new(StandInState {
        forecast: Vec::new(),
        redirect_to: String::new(),
        model: Vec::new(),
    }));
    let server = start_stand_in(weather_state.clone());
    let port = server.base.rsplit(':').next().unwrap().to_string();
    {
        let mut state = weather_state.lock().unwrap();
        state.redirect_to = format!("http://127.0.0.2:{port}/forecast");
        state.model = model_body_for(&program_for(&server.base, "/redirect"));
    }
    let core = core_at(temp.path(), server.base.clone());
    core.save_api_key("mk-weather-test-canary-1234567890")
        .await
        .unwrap();
    let draft = core
        .draft_task_no_input(
            "Get today's forecast for Auckland and write a short summary to weather.txt",
        )
        .unwrap();
    let draft = core
        .update_task(&draft.draft_id, TaskKind::Program, "weather.txt", None)
        .unwrap();
    let started = core.start_build(&draft.draft_id, true).await.unwrap();
    let state = wait(&core, &started.build_id).await;
    assert_eq!(state.phase, BuildPhase::Failed, "{state:?}");
    assert!(
        state.checks.iter().any(|c| c.id == "web_sources"
            && !c.passed
            && c.detail.as_deref().is_some_and(|d| d.contains("redirect"))),
        "{:#?}",
        state.checks
    );
}

#[tokio::test]
async fn oversize_response_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let weather_state = Arc::new(Mutex::new(StandInState {
        forecast: Vec::new(),
        redirect_to: String::new(),
        model: Vec::new(),
    }));
    let server = start_stand_in(weather_state.clone());
    weather_state.lock().unwrap().model = model_body_for(&program_for(&server.base, "/big"));
    let core = core_at(temp.path(), server.base.clone());
    core.save_api_key("mk-weather-test-canary-1234567890")
        .await
        .unwrap();
    let draft = core
        .draft_task_no_input(
            "Get today's forecast for Auckland and write a short summary to weather.txt",
        )
        .unwrap();
    let draft = core
        .update_task(&draft.draft_id, TaskKind::Program, "weather.txt", None)
        .unwrap();
    let started = core.start_build(&draft.draft_id, true).await.unwrap();
    let state = wait(&core, &started.build_id).await;
    assert_eq!(state.phase, BuildPhase::Failed, "{state:?}");
    assert!(
        state.checks.iter().any(|c| c.id == "web_sources"
            && !c.passed
            && c.detail.as_deref().is_some_and(|d| d.contains("2 MB"))),
        "{:#?}",
        state.checks
    );
}

#[tokio::test]
async fn local_only_blocks_fetches_with_a_clear_message() {
    let temp = tempfile::tempdir().unwrap();
    let weather_state = Arc::new(Mutex::new(StandInState {
        forecast: br#"{"hourly":{"temperature_2m":[21.5]}}"#.to_vec(),
        redirect_to: String::new(),
        model: Vec::new(),
    }));
    let server = start_stand_in(weather_state.clone());
    weather_state.lock().unwrap().model = model_body_for(&program_for(&server.base, "/forecast"));
    let core = core_at(temp.path(), server.base.clone());
    let tool = build_weather(&core).await;
    core.set_local_only(true).await.unwrap();
    let folder = out_dir(&core, temp.path(), "Reports");
    match core
        .stage_run_no_input(&tool.summary_row.id, &folder.handle_id, "weather.txt")
        .await
        .unwrap()
    {
        StageResult::Failed(envelope) => match envelope.failure {
            RunFailure::Internal { message } => {
                assert!(message.contains("Local only"), "{message}")
            }
            other => panic!("{other:?}"),
        },
        staged => panic!("fetch ran under Local only: {staged:?}"),
    }
}

#[tokio::test]
async fn non_https_source_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let program = Program {
        summary: "Summarize the Auckland forecast".into(),
        output_name: "weather.txt".into(),
        code: CODE.into(),
        web_sources: vec![WebSource {
            name: "forecast".into(),
            url: "http://api.open-meteo.com/v1/forecast".into(),
        }],
    };
    let weather_state = Arc::new(Mutex::new(StandInState {
        forecast: Vec::new(),
        redirect_to: String::new(),
        model: model_body_for(&program),
    }));
    let server = start_stand_in(weather_state);
    let core = core_at(temp.path(), server.base.clone());
    core.save_api_key("mk-weather-test-canary-1234567890")
        .await
        .unwrap();
    let draft = core
        .draft_task_no_input(
            "Get today's forecast for Auckland and write a short summary to weather.txt",
        )
        .unwrap();
    let draft = core
        .update_task(&draft.draft_id, TaskKind::Program, "weather.txt", None)
        .unwrap();
    let started = core.start_build(&draft.draft_id, true).await.unwrap();
    let state = wait(&core, &started.build_id).await;
    assert_eq!(state.phase, BuildPhase::Failed, "{state:?}");
    assert!(
        state
            .failure
            .as_ref()
            .is_some_and(|f| f.message.contains("https")),
        "{:?}",
        state.failure
    );
}

/// The staging fetch is awaited end to end (no `block_in_place`), so it works
/// on a current-thread runtime: the `#[tokio::test]` default.
#[tokio::test]
async fn web_fetch_works_on_a_current_thread_runtime() {
    let weather_state = Arc::new(Mutex::new(StandInState {
        forecast: br#"{"hourly":{"temperature_2m":[21.5]}}"#.to_vec(),
        redirect_to: String::new(),
        model: Vec::new(),
    }));
    let server = start_stand_in(weather_state);
    let text = macroscope_app_core::web::fetch_one(
        &format!("{}/forecast", server.base),
        &["127.0.0.1".to_string()],
        true,
        "forecast",
    )
    .await
    .expect("fetch on a current-thread runtime");
    assert_eq!(text, r#"{"hourly":{"temperature_2m":[21.5]}}"#);
}

/// The same fetch, plus the async stage path that used to `block_in_place`
/// (and panicked on a current-thread runtime), works on a multi-thread
/// runtime without blocking a worker. The stage half uses a CSV tool so it
/// needs no OS sandbox; the web end to end covers programs with sources.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn web_fetch_and_stage_work_on_a_multi_thread_runtime() {
    let weather_state = Arc::new(Mutex::new(StandInState {
        forecast: br#"{"hourly":{"temperature_2m":[21.5]}}"#.to_vec(),
        redirect_to: String::new(),
        model: Vec::new(),
    }));
    let server = start_stand_in(weather_state);
    let text = macroscope_app_core::web::fetch_one(
        &format!("{}/forecast", server.base),
        &["127.0.0.1".to_string()],
        true,
        "forecast",
    )
    .await
    .expect("fetch on a multi-thread runtime");
    assert_eq!(text, r#"{"hourly":{"temperature_2m":[21.5]}}"#);

    let temp = tempfile::tempdir().unwrap();
    let core = core_at(temp.path(), "http://127.0.0.1:9".into());
    let tool = core.install_sample_tool().unwrap();
    let input = core.load_sample_csv().unwrap().handle;
    let dir = core.register_picked_dir(temp.path()).unwrap();
    match core
        .stage_run(
            &tool.summary_row.id,
            &input.handle_id,
            &dir.handle_id,
            "orders_ready.csv",
        )
        .await
        .unwrap()
    {
        StageResult::Staged(staged) => {
            assert_eq!((staged.rows_in, staged.rows_out), (18, 18));
        }
        result => panic!("{result:?}"),
    }
}
