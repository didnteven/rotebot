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
        ("GET", "/big") => ("200 OK", String::new(), vec![b'x'; 5 * 1024 * 1024]),
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
    core_at_clock(root, endpoint, CoreConfig::system_clock())
}
fn core_at_clock(
    root: &Path,
    endpoint: String,
    clock: Arc<dyn Fn() -> i64 + Send + Sync>,
) -> AppCore {
    core_at_clock_vault(root, endpoint, clock, Arc::new(MemoryStore::default()))
}
fn core_at_clock_vault(
    root: &Path,
    endpoint: String,
    clock: Arc<dyn Fn() -> i64 + Send + Sync>,
    vault: Arc<MemoryStore>,
) -> AppCore {
    let core = AppCore::new(CoreConfig {
        data_dir: root.join("data"),
        vault,
        gateway_base_url: endpoint,
        home: root.join("empty-home"),
        muse: Arc::new(NativeMuse {
            home: root.join("empty-home"),
        }),
        app_version: "test".into(),
        platform: "macos".into(),
        now_ms: clock,
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
        connection_sources: vec![],
        model_step: None,
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
            && c.detail.as_deref().is_some_and(|d| d.contains("4 MB"))),
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
        connection_sources: vec![],
        model_step: None,
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

#[cfg(feature = "synthetic-runtime-tests")]
mod schedules {
    use super::*;
    use macroscope_app_core::conditions::{SendWhen, Side};
    use macroscope_app_core::schedules::*;
    use std::sync::atomic::{AtomicI64, Ordering};
    #[derive(Default)]
    struct Notifications(Mutex<Vec<(String, String)>>);
    impl Notifier for Notifications {
        fn notify(&self, title: &str, body: &str) -> Result<(), String> {
            self.0.lock().unwrap().push((title.into(), body.into()));
            Ok(())
        }
    }
    /// Records every send; `fail` makes the mail server refuse.
    #[derive(Default)]
    struct FakeMailer {
        sent: Mutex<Vec<(String, String, String, String)>>,
        fail: bool,
    }
    impl ResultMailer for FakeMailer {
        fn send<'a>(
            &'a self,
            connector_id: &'a str,
            to: &'a str,
            subject: &'a str,
            body: &'a str,
        ) -> MailFuture<'a> {
            Box::pin(async move {
                if self.fail {
                    return Err("the mail server refused it".into());
                }
                self.sent.lock().unwrap().push((
                    connector_id.into(),
                    to.into(),
                    subject.into(),
                    body.into(),
                ));
                Ok(())
            })
        }
    }
    fn at(s: &str) -> i64 {
        chrono::DateTime::parse_from_rfc3339(s)
            .unwrap()
            .timestamp_millis()
    }
    struct Fixture {
        _tmp: tempfile::TempDir,
        core: AppCore,
        tool: ToolDetail,
        folder: FileHandle,
        clock: Arc<AtomicI64>,
        notifications: Arc<Notifications>,
        vault: Arc<MemoryStore>,
    }
    impl Fixture {
        async fn new() -> Self {
            let tmp = tempfile::tempdir().unwrap();
            let state = Arc::new(Mutex::new(StandInState {
                forecast: br#"{"hourly":{"temperature_2m":[21.5]}}"#.to_vec(),
                redirect_to: String::new(),
                model: Vec::new(),
            }));
            let server = start_stand_in(state.clone());
            state.lock().unwrap().model = model_body_for(&program_for(&server.base, "/forecast"));
            let clock = Arc::new(AtomicI64::new(at("2026-10-28T06:00:00+13:00")));
            let c = clock.clone();
            let vault = Arc::new(MemoryStore::default());
            let core = core_at_clock_vault(
                tmp.path(),
                server.base,
                Arc::new(move || c.load(Ordering::SeqCst)),
                vault.clone(),
            );
            core.set_schedule_timezone(chrono_tz::Pacific::Auckland);
            let notifications = Arc::new(Notifications::default());
            core.set_notifier(notifications.clone());
            let tool = build_weather(&core).await;
            let folder = out_dir(&core, tmp.path(), "Reports");
            Self {
                _tmp: tmp,
                core,
                tool,
                folder,
                clock,
                notifications,
                vault,
            }
        }
        fn enable(&self, file: bool) -> Schedule {
            self.enable_with(file, None)
        }
        fn enable_with(&self, file: bool, email: Option<EmailDelivery>) -> Schedule {
            self.enable_when(file, email, SendWhen::Always)
        }
        fn enable_when(
            &self,
            file: bool,
            email: Option<EmailDelivery>,
            send_when: SendWhen,
        ) -> Schedule {
            self.core
                .set_schedule(
                    &self.tool.summary_row.id,
                    ScheduleRequest {
                        frequency: Frequency::Daily {
                            time: "07:00".into(),
                        },
                        outputs: ScheduleOutputs {
                            today: true,
                            notification: true,
                            folder: if file {
                                Some(self.folder.clone())
                            } else {
                                None
                            },
                            filename: "weather-{date}.txt".into(),
                            email,
                            send_when,
                        },
                        version_hash: self.tool.version_hash.clone(),
                        approved: true,
                    },
                )
                .unwrap()
        }
        async fn run(&self, s: &str) -> Vec<TodayEntry> {
            let now = at(s);
            self.clock.store(now, Ordering::SeqCst);
            self.core.run_due_schedules(now).await.unwrap()
        }
    }
    #[tokio::test]
    async fn scheduled_weather_uses_normal_stage_receipt_outbound_and_exact_notification() {
        let f = Fixture::new().await;
        let schedule = f.enable(true);
        assert!(f.run("2026-10-28T06:59:00+13:00").await.is_empty());
        let before = f.core.list_outbound().unwrap().len();
        let entries = f.run("2026-10-28T07:00:00+13:00").await;
        assert_eq!(entries.len(), 1);
        let e = &entries[0];
        assert_eq!(e.status, "done");
        assert_eq!(e.body, "Auckland high 21.5C\n");
        let path = Path::new(&f.folder.display_path).join("weather-2026-10-28.txt");
        assert_eq!(std::fs::read(&path).unwrap(), b"Auckland high 21.5C\n");
        assert_eq!(e.file_created.as_deref(), path.to_str());
        assert_eq!(f.core.list_today().unwrap()[0].entries.len(), 1);
        assert_eq!(
            *f.notifications.0.lock().unwrap(),
            vec![("Auckland weather".into(), "Auckland high 21.5C".into())]
        );
        let receipt = &f.core.list_receipts().unwrap()[0];
        assert_eq!(receipt.trigger, RunTrigger::Schedule);
        assert_eq!(receipt.schedule_id.as_deref(), Some(schedule.id.as_str()));
        assert_eq!(receipt.run_id, e.run_id.as_deref().unwrap());
        assert_eq!(receipt.status, ReceiptStatus::Committed);
        assert_eq!(f.core.list_outbound().unwrap().len(), before + 1);
        assert_eq!(f.core.list_outbound().unwrap()[0].purpose, "Web source");
        // The activity log says what happened in plain words, never the result or a bare action count.
        let ran = f
            .core
            .list_activity()
            .unwrap()
            .into_iter()
            .find(|a| a.kind == "run")
            .unwrap();
        assert_eq!(
            ran.detail.as_deref(),
            Some("Created weather-2026-10-28.txt")
        );
        assert!(f.run("2026-10-28T23:59:00+13:00").await.is_empty());
    }
    fn me() -> Option<EmailDelivery> {
        Some(EmailDelivery {
            connector_id: "gmail-mail".into(),
            to: "me@example.com".into(),
        })
    }
    #[tokio::test]
    async fn scheduled_result_is_emailed_to_the_approved_address_and_noted_on_today() {
        let f = Fixture::new().await;
        let mailer = Arc::new(FakeMailer::default());
        f.core.set_mailer(mailer.clone());
        f.enable_with(false, me());
        let entries = f.run("2026-10-28T07:00:00+13:00").await;
        assert_eq!(entries[0].status, "done");
        let sent = mailer.sent.lock().unwrap().clone();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0, "gmail-mail");
        assert_eq!(sent[0].1, "me@example.com");
        assert_eq!(sent[0].2, "Auckland weather · 2026-10-28");
        assert!(
            sent[0].3.starts_with("Auckland high 21.5C"),
            "{}",
            sent[0].3
        );
        assert_eq!(
            entries[0].note.as_deref(),
            Some("Emailed to me@example.com")
        );
        assert_eq!(
            f.core.list_today().unwrap()[0].entries[0].note.as_deref(),
            Some("Emailed to me@example.com")
        );
        // One send per occurrence.
        assert!(f.run("2026-10-28T23:59:00+13:00").await.is_empty());
        assert_eq!(mailer.sent.lock().unwrap().len(), 1);
    }
    #[tokio::test]
    async fn a_refused_or_missing_mailbox_never_fails_the_run() {
        let f = Fixture::new().await;
        f.core.set_mailer(Arc::new(FakeMailer {
            fail: true,
            ..Default::default()
        }));
        f.enable_with(false, me());
        let e = &f.run("2026-10-28T07:00:00+13:00").await[0];
        assert_eq!(e.status, "done");
        assert_eq!(
            e.note.as_deref(),
            Some("Email not sent: the mail server refused it")
        );
        let f = Fixture::new().await;
        f.enable_with(false, me());
        let e = &f.run("2026-10-28T07:00:00+13:00").await[0];
        assert_eq!(e.status, "done");
        assert_eq!(
            e.note.as_deref(),
            Some("Email not sent: email is not available")
        );
    }
    #[tokio::test]
    async fn a_condition_that_is_not_met_holds_back_the_notification_and_email_but_today_records_the_run(
    ) {
        let f = Fixture::new().await;
        let mailer = Arc::new(FakeMailer::default());
        f.core.set_mailer(mailer.clone());
        f.enable_when(
            false,
            me(),
            SendWhen::Above {
                near: "high".into(),
                side: Side::After,
                value: 25.0,
            },
        );
        let e = &f.run("2026-10-28T07:00:00+13:00").await[0];
        assert_eq!(e.status, "done");
        assert_eq!(e.body, "Auckland high 21.5C\n");
        assert!(!e.notification);
        assert!(f.notifications.0.lock().unwrap().is_empty());
        assert!(mailer.sent.lock().unwrap().is_empty());
        assert_eq!(
            e.note.as_deref(),
            Some("“high” was 21.5, not above 25, so nothing was sent")
        );
        assert_eq!(f.core.list_today().unwrap()[0].entries.len(), 1);
    }
    #[tokio::test]
    async fn a_condition_that_is_met_sends_both_and_explains_why() {
        let f = Fixture::new().await;
        let mailer = Arc::new(FakeMailer::default());
        f.core.set_mailer(mailer.clone());
        f.enable_when(
            false,
            me(),
            SendWhen::Above {
                near: "C".into(),
                side: Side::Before,
                value: 20.0,
            },
        );
        let e = &f.run("2026-10-28T07:00:00+13:00").await[0];
        assert!(e.notification);
        assert_eq!(f.notifications.0.lock().unwrap().len(), 1);
        assert_eq!(mailer.sent.lock().unwrap().len(), 1);
        assert_eq!(
            e.note.as_deref(),
            Some("“C” was 21.5, above 20. Emailed to me@example.com")
        );
    }
    #[tokio::test]
    async fn changed_sends_the_first_run_and_holds_an_identical_next_run() {
        let f = Fixture::new().await;
        let mailer = Arc::new(FakeMailer::default());
        f.core.set_mailer(mailer.clone());
        f.enable_when(false, me(), SendWhen::Changed);
        let first = &f.run("2026-10-28T07:00:00+13:00").await[0];
        assert!(first.notification);
        assert_eq!(mailer.sent.lock().unwrap().len(), 1);
        let second = &f.run("2026-10-29T07:00:00+13:00").await[0];
        assert_eq!(second.status, "done");
        assert!(!second.notification);
        assert_eq!(
            mailer.sent.lock().unwrap().len(),
            1,
            "an unchanged result must not be emailed again"
        );
        assert_eq!(
            second.note.as_deref(),
            Some("The result didn’t change, so nothing was sent")
        );
    }
    #[tokio::test]
    async fn an_unsound_condition_cannot_be_approved() {
        let f = Fixture::new().await;
        let request = |send_when| ScheduleRequest {
            frequency: Frequency::Daily {
                time: "07:00".into(),
            },
            outputs: ScheduleOutputs {
                today: true,
                notification: true,
                folder: None,
                filename: "weather.txt".into(),
                email: None,
                send_when,
            },
            version_hash: f.tool.version_hash.clone(),
            approved: true,
        };
        let id = &f.tool.summary_row.id;
        assert!(f
            .core
            .set_schedule(id, request(SendWhen::Contains { text: " ".into() }))
            .is_err());
        assert!(f
            .core
            .set_schedule(
                id,
                request(SendWhen::Below {
                    near: "x".into(),
                    side: Side::After,
                    value: f64::NAN
                })
            )
            .is_err());
        assert!(f.core.set_schedule(id, request(SendWhen::Changed)).is_ok());
        // The condition is part of what was approved: it survives a reload.
        assert_eq!(
            f.core.list_schedules().unwrap()[0].outputs.send_when,
            SendWhen::Changed
        );
    }
    #[tokio::test]
    async fn only_one_plain_address_can_be_approved() {
        let f = Fixture::new().await;
        for bad in [
            "",
            "me",
            "me@",
            "@example.com",
            "me@example",
            "a@b.com, c@d.com",
            "Me <me@example.com>",
            "me@example.com\nBcc: x@y.com",
            "me @example.com",
            "me@-example.com",
        ] {
            let request = ScheduleRequest {
                frequency: Frequency::Daily {
                    time: "07:00".into(),
                },
                outputs: ScheduleOutputs {
                    today: true,
                    notification: false,
                    folder: None,
                    filename: "weather.txt".into(),
                    email: Some(EmailDelivery {
                        connector_id: "gmail-mail".into(),
                        to: bad.into(),
                    }),
                    send_when: Default::default(),
                },
                version_hash: f.tool.version_hash.clone(),
                approved: true,
            };
            assert!(
                f.core
                    .set_schedule(&f.tool.summary_row.id, request)
                    .is_err(),
                "{bad:?} was accepted"
            );
        }
    }
    #[tokio::test]
    async fn missed_three_days_coalesces_once_and_digest_only_creates_no_file() {
        let f = Fixture::new().await;
        f.enable(false);
        let entries = f.run("2026-10-31T09:14:00+13:00").await;
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].status, "done");
        assert_eq!(entries[0].file_created, None);
        let ran = f
            .core
            .list_activity()
            .unwrap()
            .into_iter()
            .find(|a| a.kind == "run")
            .unwrap();
        assert_eq!(ran.detail.as_deref(), Some("Finished"));
        assert_eq!(
            entries[0].note.as_deref(),
            Some("Missed at 7:00 (Rotebot was closed); ran at 9:14")
        );
        assert!(std::fs::read_dir(&f.folder.display_path)
            .unwrap()
            .next()
            .is_none());
        assert!(f.run("2026-10-31T10:00:00+13:00").await.is_empty());
    }
    #[tokio::test]
    async fn launch_after_three_days_uses_persisted_grant_and_runs_once() {
        let f = Fixture::new().await;
        f.enable(true);
        let Fixture {
            _tmp,
            core,
            clock,
            notifications,
            vault,
            ..
        } = f;
        drop(core);
        let now = at("2026-10-31T09:14:00+13:00");
        clock.store(now, Ordering::SeqCst);
        let c = clock.clone();
        let reopened = core_at_clock_vault(
            _tmp.path(),
            "http://127.0.0.1:9".into(),
            Arc::new(move || c.load(Ordering::SeqCst)),
            vault,
        );
        reopened.set_schedule_timezone(chrono_tz::Pacific::Auckland);
        reopened.set_notifier(notifications);
        let entries = reopened.run_due_schedules(now).await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].status, "done");
        assert_eq!(
            entries[0].note.as_deref(),
            Some("Missed at 7:00 (Rotebot was closed); ran at 9:14")
        );
        assert!(reopened
            .run_due_schedules(now + 30_000)
            .await
            .unwrap()
            .is_empty());
    }
    #[tokio::test]
    async fn schedule_rejects_stale_unapproved_empty_outputs_and_replaced_folder() {
        let f = Fixture::new().await;
        let original = f.enable(true);
        let mut request = ScheduleRequest {
            frequency: original.frequency,
            outputs: original.outputs,
            version_hash: f.tool.version_hash.clone(),
            approved: false,
        };
        assert!(f
            .core
            .set_schedule(&f.tool.summary_row.id, request.clone())
            .is_err());
        request.approved = true;
        request.version_hash = "old-version".into();
        assert!(f
            .core
            .set_schedule(&f.tool.summary_row.id, request.clone())
            .is_err());
        request.version_hash = f.tool.version_hash.clone();
        request.outputs = ScheduleOutputs {
            today: false,
            notification: false,
            folder: None,
            filename: "weather.txt".into(),
            email: None,
            send_when: Default::default(),
        };
        assert!(f
            .core
            .set_schedule(&f.tool.summary_row.id, request)
            .is_err());
        let path = Path::new(&f.folder.display_path);
        std::fs::rename(path, path.with_file_name("OldReports")).unwrap();
        std::fs::create_dir(path).unwrap();
        assert_eq!(
            f.core.list_schedules().unwrap()[0].status,
            "needs_reapproval"
        );
        assert!(f.run("2026-10-28T07:00:00+13:00").await.is_empty());
    }
    #[tokio::test]
    async fn evaluator_runs_weekly_monthly_and_each_dst_day_once() {
        for (before, frequency, due, later) in [
            (
                "2026-10-27T06:00:00+13:00",
                Frequency::Weekly {
                    weekday: 3,
                    time: "07:00".into(),
                },
                "2026-10-28T07:00:00+13:00",
                "2026-10-29T07:00:00+13:00",
            ),
            (
                "2026-11-29T06:00:00+13:00",
                Frequency::Monthly {
                    day: 31,
                    time: "07:00".into(),
                },
                "2026-11-30T07:00:00+13:00",
                "2026-11-30T08:00:00+13:00",
            ),
            (
                "2026-02-27T06:00:00+13:00",
                Frequency::Monthly {
                    day: 31,
                    time: "07:00".into(),
                },
                "2026-02-28T07:00:00+13:00",
                "2026-02-28T08:00:00+13:00",
            ),
            (
                "2026-09-26T06:00:00+12:00",
                Frequency::Daily {
                    time: "02:30".into(),
                },
                "2026-09-27T03:00:00+13:00",
                "2026-09-27T03:30:00+13:00",
            ),
            (
                "2026-04-04T06:00:00+13:00",
                Frequency::Daily {
                    time: "02:30".into(),
                },
                "2026-04-05T02:30:00+13:00",
                "2026-04-05T02:30:00+12:00",
            ),
        ] {
            let f = Fixture::new().await;
            f.clock.store(at(before), Ordering::SeqCst);
            f.core
                .set_schedule(
                    &f.tool.summary_row.id,
                    ScheduleRequest {
                        frequency,
                        outputs: ScheduleOutputs {
                            today: true,
                            notification: true,
                            folder: None,
                            filename: "weather.txt".into(),
                            email: None,
                            send_when: Default::default(),
                        },
                        version_hash: f.tool.version_hash.clone(),
                        approved: true,
                    },
                )
                .unwrap();
            assert!(f
                .core
                .run_due_schedules(at(due) - 1)
                .await
                .unwrap()
                .is_empty());
            let entries = f.run(due).await;
            assert_eq!(entries.len(), 1, "{due}");
            assert_eq!(entries[0].status, "done");
            assert!(f.run(later).await.is_empty(), "{later}");
        }
    }
    #[tokio::test]
    async fn local_only_skips_without_fetch_or_outbound_and_emergency_pauses() {
        let f = Fixture::new().await;
        f.enable(true);
        f.core.set_local_only(true).await.unwrap();
        let before = f.core.list_outbound().unwrap().len();
        let entries = f.run("2026-10-28T07:00:00+13:00").await;
        assert_eq!(entries[0].status, "skipped");
        assert_eq!(entries[0].body, "Skipped at 7:00 — Local only is on");
        assert_eq!(f.core.list_outbound().unwrap().len(), before);
        assert_eq!(
            f.notifications.0.lock().unwrap()[0].1,
            "Skipped: Local only is on"
        );
        f.core.emergency_stop().await.unwrap();
        assert_eq!(
            f.core.list_schedules().unwrap()[0].status,
            "emergency_paused"
        );
        assert!(f.run("2026-10-29T07:00:00+13:00").await.is_empty());
    }
    #[tokio::test]
    async fn a_new_version_saved_with_its_own_approval_keeps_the_schedule() {
        let f = Fixture::new().await;
        f.enable(true);
        let db = f.core.database();
        let mut row = db.tool(&f.tool.summary_row.id).unwrap().unwrap();
        let mut version = db
            .version(&f.tool.summary_row.id, &f.tool.version_hash)
            .unwrap()
            .unwrap();
        let (_, rev, scope) = db
            .active_grant(&f.tool.summary_row.id, &f.tool.version_hash)
            .unwrap()
            .unwrap();
        version.hash = "edited-version".into();
        db.insert_version(&version).unwrap();
        db.put_grant(
            "edited",
            &f.tool.summary_row.id,
            &version.hash,
            rev + 1,
            &scope,
        )
        .unwrap();
        row.active_version = Some(version.hash.clone());
        db.put_tool(&row).unwrap();
        let schedules = f.core.list_schedules().unwrap();
        assert_eq!(schedules[0].status, "enabled");
        assert_eq!(schedules[0].version_hash, "edited-version");
    }
    #[tokio::test]
    async fn changed_version_source_or_grant_needs_reapproval() {
        for change in ["version", "source", "grant"] {
            let f = Fixture::new().await;
            f.enable(true);
            if change == "grant" {
                f.core
                    .database()
                    .put_grant(
                        "changed",
                        &f.tool.summary_row.id,
                        &f.tool.version_hash,
                        99,
                        &json!({"webHosts":["other.test"]}),
                    )
                    .unwrap();
            } else {
                let mut row = f
                    .core
                    .database()
                    .tool(&f.tool.summary_row.id)
                    .unwrap()
                    .unwrap();
                if change == "version" {
                    row.active_version = Some("replacement".into());
                } else {
                    let mut version = f
                        .core
                        .database()
                        .version(&f.tool.summary_row.id, &f.tool.version_hash)
                        .unwrap()
                        .unwrap();
                    version.hash = "rebuilt-with-changed-source".into();
                    version.mapping["program"]["web_sources"][0]["url"] =
                        json!("https://other.test/forecast");
                    row.active_version = Some(version.hash.clone());
                    f.core.database().insert_version(&version).unwrap();
                }
                f.core.database().put_tool(&row).unwrap();
            }
            let schedules = f.core.list_schedules().unwrap();
            assert_eq!(schedules[0].status, "needs_reapproval", "{change}");
            assert!(f.run("2026-10-28T07:00:00+13:00").await.is_empty());
            assert!(f
                .core
                .schedule_status(&f.tool.summary_row.id, "enabled")
                .is_err());
        }
    }
    #[tokio::test]
    async fn collision_suffix_never_overwrites_and_exhaustion_fails_clearly() {
        let f = Fixture::new().await;
        f.enable(true);
        let root = Path::new(&f.folder.display_path);
        std::fs::write(root.join("weather-2026-10-28.txt"), b"keep").unwrap();
        let entries = f.run("2026-10-28T07:00:00+13:00").await;
        assert_eq!(entries[0].status, "done");
        assert_eq!(
            std::fs::read(root.join("weather-2026-10-28 (2).txt")).unwrap(),
            b"Auckland high 21.5C\n"
        );
        assert_eq!(
            std::fs::read(root.join("weather-2026-10-28.txt")).unwrap(),
            b"keep"
        );
        for n in 1..=9 {
            let name = if n == 1 {
                "weather-2026-10-29.txt".into()
            } else {
                format!("weather-2026-10-29 ({n}).txt")
            };
            std::fs::write(root.join(name), b"keep").unwrap();
        }
        let entries = f.run("2026-10-29T07:00:00+13:00").await;
        assert_eq!(entries[0].status, "failed");
        assert!(entries[0].body.contains("(9)"));
        assert_eq!(
            f.notifications.0.lock().unwrap().last().unwrap().1,
            "Failed: Open Today for details"
        );
    }
    #[tokio::test]
    async fn active_manual_run_skips_schedule_then_commit_adds_manual_today_and_privacy_deletes() {
        let f = Fixture::new().await;
        f.enable(true);
        let StageResult::Staged(staged) = f
            .core
            .stage_run_no_input(&f.tool.summary_row.id, &f.folder.handle_id, "weather.txt")
            .await
            .unwrap()
        else {
            panic!("stage failed")
        };
        let entries = f.run("2026-10-28T07:00:00+13:00").await;
        assert_eq!(entries[0].status, "skipped");
        assert!(entries[0].body.contains("already active"));
        f.core
            .commit_run(&staged.run_id, &staged.plan_hash)
            .unwrap();
        assert!(f.core.list_today().unwrap()[0]
            .entries
            .iter()
            .any(|e| e.trigger == "manual" && e.status == "done"));
        assert!(f
            .core
            .privacy_overview()
            .await
            .unwrap()
            .stored
            .iter()
            .any(|r| r.label == "Today entries" && r.count == 2));
        f.core
            .delete_data(DeleteOptions {
                include_tools: true,
            })
            .await
            .unwrap();
        assert!(f.core.list_today().unwrap().is_empty());
        assert!(f.core.list_schedules().unwrap().is_empty());
    }
}

const CODE_V2: &str = r#"function run() {
    const api = macroscope.source("forecast");
    const data = JSON.parse(api);
    macroscope.output("weather.txt", "Auckland low " + data.hourly.temperature_2m[0] + "C\n");
}"#;

async fn preview_text(core: &AppCore, tool_id: &str) -> String {
    match core.preview_run_no_input(tool_id).await.unwrap() {
        StageResult::Staged(p) => p.preview_text.unwrap_or_default().trim().to_string(),
        other => panic!("{other:?}"),
    }
}

async fn edit_and_save(core: &AppCore, goal: &str, replaces: &str) -> ToolDetail {
    let draft = core.draft_task_no_input(goal).unwrap();
    let draft = core.set_draft_replaces(&draft.draft_id, replaces).unwrap();
    assert_eq!(draft.replaces_tool_id.as_deref(), Some(replaces));
    let draft = core
        .update_task(&draft.draft_id, TaskKind::Program, "weather.txt", None)
        .unwrap();
    let started = core.start_build(&draft.draft_id, true).await.unwrap();
    let built = wait(core, &started.build_id).await;
    assert_eq!(built.phase, BuildPhase::Ready, "{built:?}");
    core.save_tool(&started.build_id, "Auckland weather")
        .await
        .unwrap()
}

#[tokio::test]
async fn editing_a_saved_tool_adds_a_version_and_older_ones_can_be_restored() {
    let temp = tempfile::tempdir().unwrap();
    let state = Arc::new(Mutex::new(StandInState {
        forecast: br#"{"hourly":{"temperature_2m":[21.5]}}"#.to_vec(),
        redirect_to: String::new(),
        model: Vec::new(),
    }));
    let server = start_stand_in(state.clone());
    state.lock().unwrap().model = model_body_for(&program_for(&server.base, "/forecast"));
    let core = core_at(temp.path(), server.base.clone());
    let v1 = build_weather(&core).await;
    let id = v1.summary_row.id.clone();
    assert_eq!(v1.summary_row.version, 1);
    assert!(v1.goal.contains("Auckland"), "{}", v1.goal);
    assert_eq!(preview_text(&core, &id).await, "Auckland high 21.5C");

    let mut program = program_for(&server.base, "/forecast");
    program.code = CODE_V2.into();
    state.lock().unwrap().model = model_body_for(&program);
    let v2 = edit_and_save(&core, &format!("{} Show the low instead.", v1.goal), &id).await;
    assert_eq!(v2.summary_row.id, id, "an edit keeps the same tool");
    assert_eq!(v2.summary_row.version, 2);
    assert_eq!(v2.summary_row.name, "Auckland weather");
    assert_ne!(v2.version_hash, v1.version_hash);
    assert!(v2.goal.contains("Show the low instead"));
    assert_eq!(core.list_tools().unwrap().len(), 1);
    assert_eq!(preview_text(&core, &id).await, "Auckland low 21.5C");

    let numbers = |core: &AppCore| {
        core.list_tool_versions(&id)
            .unwrap()
            .iter()
            .map(|v| (v.number, v.active))
            .collect::<Vec<_>>()
    };
    assert_eq!(numbers(&core), vec![(2, true), (1, false)]);

    let back = core.restore_tool_version(&id, &v1.version_hash).unwrap();
    assert_eq!(back.version_hash, v1.version_hash);
    assert_eq!(back.summary_row.version, 1);
    assert!(!back.goal.contains("Show the low instead"));
    assert_eq!(preview_text(&core, &id).await, "Auckland high 21.5C");
    assert_eq!(numbers(&core), vec![(2, false), (1, true)]);

    // Restoring does not lose the newer version, and new edits continue after the highest number.
    let v3 = edit_and_save(&core, "Auckland low again", &id).await;
    assert_eq!(v3.summary_row.version, 3);
    let forward = core.restore_tool_version(&id, &v2.version_hash).unwrap();
    assert_eq!(forward.summary_row.version, 2);
    assert!(core.restore_tool_version(&id, "sha-unknown").is_err());
}

#[tokio::test]
async fn a_paused_tool_stays_paused_when_edited_and_resumes_on_the_new_version() {
    let temp = tempfile::tempdir().unwrap();
    let state = Arc::new(Mutex::new(StandInState {
        forecast: br#"{"hourly":{"temperature_2m":[21.5]}}"#.to_vec(),
        redirect_to: String::new(),
        model: Vec::new(),
    }));
    let server = start_stand_in(state.clone());
    state.lock().unwrap().model = model_body_for(&program_for(&server.base, "/forecast"));
    let core = core_at(temp.path(), server.base.clone());
    let v1 = build_weather(&core).await;
    let id = v1.summary_row.id.clone();
    core.revoke_tool(&id).unwrap();
    let mut program = program_for(&server.base, "/forecast");
    program.code = CODE_V2.into();
    state.lock().unwrap().model = model_body_for(&program);
    let v2 = edit_and_save(&core, &v1.goal, &id).await;
    assert_eq!(v2.summary_row.state, "paused");
    let resumed = core.restore_tool(&id).unwrap();
    assert_eq!(resumed.summary_row.state, "enabled");
    assert_eq!(resumed.version_hash, v2.version_hash);
}

#[tokio::test]
async fn the_sample_cannot_be_edited_and_a_deleted_tool_saves_as_a_new_one() {
    let temp = tempfile::tempdir().unwrap();
    let state = Arc::new(Mutex::new(StandInState {
        forecast: br#"{"hourly":{"temperature_2m":[21.5]}}"#.to_vec(),
        redirect_to: String::new(),
        model: Vec::new(),
    }));
    let server = start_stand_in(state.clone());
    state.lock().unwrap().model = model_body_for(&program_for(&server.base, "/forecast"));
    let core = core_at(temp.path(), server.base.clone());
    let sample = core.install_sample_tool().unwrap();
    let draft = core.draft_task_no_input("anything").unwrap();
    assert!(core
        .set_draft_replaces(&draft.draft_id, &sample.summary_row.id)
        .is_err());

    let v1 = build_weather(&core).await;
    let draft = core.draft_task_no_input(&v1.goal).unwrap();
    core.set_draft_replaces(&draft.draft_id, &v1.summary_row.id)
        .unwrap();
    let draft = core
        .update_task(&draft.draft_id, TaskKind::Program, "weather.txt", None)
        .unwrap();
    let started = core.start_build(&draft.draft_id, true).await.unwrap();
    assert_eq!(
        wait(&core, &started.build_id).await.phase,
        BuildPhase::Ready
    );
    core.delete_tool(&v1.summary_row.id).unwrap();
    let saved = core
        .save_tool(&started.build_id, "Auckland weather")
        .await
        .unwrap();
    assert_ne!(saved.summary_row.id, v1.summary_row.id);
    assert_eq!(saved.summary_row.version, 1);
}
