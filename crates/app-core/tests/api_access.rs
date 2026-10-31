//! Production core/gateway/transport paths against local provider stand-ins.
use macroscope_app_core::{
    api::{ApiAuth, ApiConfig},
    connections::Connections,
    dto::*,
    AppCore, CoreConfig, MuseProbe,
};
use macroscope_muse_client::PlanStatus;
use macroscope_vault::{MemoryStore, Secret};
use serde_json::{json, Value};
use std::{future::Future, pin::Pin, sync::Arc};
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

struct NoMuse;
impl MuseProbe for NoMuse {
    fn plan_status(&self) -> Pin<Box<dyn Future<Output = PlanStatus> + Send + '_>> {
        Box::pin(async { PlanStatus::NotSignedIn })
    }
    fn version(&self) -> Option<String> {
        None
    }
}
struct Fixture {
    core: AppCore,
    conns: Connections,
    _dir: tempfile::TempDir,
}
impl Fixture {
    async fn new(server: &MockServer) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let vault = Arc::new(MemoryStore::default());
        let core = AppCore::new(CoreConfig {
            data_dir: dir.path().join("data"),
            vault: vault.clone(),
            gateway_base_url: server.uri(),
            home: dir.path().join("empty-home"),
            muse: Arc::new(NoMuse),
            app_version: "test".into(),
            platform: "macos".into(),
            now_ms: CoreConfig::system_clock(),
        })
        .unwrap();
        core.set_allow_insecure_web_sources_for_tests(true);
        core.save_api_key("mk-SYNTHETIC-MODEL-CREDENTIAL-0001")
            .await
            .unwrap();
        let conns = Connections::new(core.clone(), vault).unwrap();
        Self {
            core,
            conns,
            _dir: dir,
        }
    }
    async fn api(&self, server: &MockServer, route: &str) -> ConnectorInfo {
        self.core
            .save_api_connection(
                ApiConfig {
                    name: "Example records".into(),
                    url: format!("{}{route}", server.uri()),
                    auth: ApiAuth::Bearer,
                    header_name: String::new(),
                    docs_url: Some(format!("{}/docs", server.uri())),
                },
                Some(Secret::new(b"SYNTHETIC-API-CREDENTIAL-0001".to_vec())),
            )
            .await
            .unwrap()
    }
}
fn answer(plan: Value, sources: Vec<String>) -> Value {
    let mut output =
        vec![json!({"type":"message","content":[{"type":"output_text","text":plan.to_string()}]})];
    if !sources.is_empty() {
        output.insert(0, json!({"type":"web_search_call","status":"completed","results":sources.into_iter().map(|url| json!({"url":url,"type":"text_result"})).collect::<Vec<_>>()}));
    }
    json!({"status":"completed","output":output,"usage":{"input_tokens":100,"output_tokens":100}})
}
fn plan(route: &str, id: Option<&str>, api: Value) -> Value {
    json!({"route":route,"service":"Example","operation":"read records","summary":"Read the records needed by this task.","connectorId":id,"api":api})
}
fn proposal(server: &MockServer, auth: &str) -> Value {
    json!({"name":"Example records","url":format!("{}/records",server.uri()),"auth":auth,"headerName":"X-API-Key","docsUrl":format!("{}/docs",server.uri()),
        "method":"GET","setupInstructions":["Create a read-only credential in the provider account."],"prerequisites":["Access to the chosen records"],"coverage":"Complete records; verify account permissions and quotas."})
}

#[tokio::test]
async fn a_known_public_api_is_checked_without_search_or_credential_setup() {
    let server = MockServer::start().await;
    Mock::given(path("/responses"))
        .respond_with(ResponseTemplate::new(200).set_body_json(answer(
            plan("public_api", None, proposal(&server, "none")),
            vec![],
        )))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path("/docs"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(format!("Public GET {}/records", server.uri())),
        )
        .expect(1)
        .mount(&server)
        .await;
    let f = Fixture::new(&server).await;
    let d = f
        .core
        .draft_task_no_input("Read public Example records")
        .unwrap();
    let p = f.core.access_preview(&f.conns, &d.draft_id).await.unwrap();
    let next = f
        .core
        .research_task_access(&f.conns, &d.draft_id, &p.payload_hash)
        .await
        .unwrap();
    let report = next.access_plan.as_ref().unwrap();
    assert_eq!(report.state, "public_api");
    assert!(!report.searched);
    assert!(next.ready_to_build);
    assert!(next.required_connections.is_empty());
    let outbound = f.core.outbound_preview(&d.draft_id).await.unwrap();
    assert!(outbound.payload_text.contains("access_research"));
    assert!(server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .all(|r| r.url.path() != "/records"));
}

#[tokio::test]
async fn a_supported_service_can_offer_its_existing_setup_instead_of_a_custom_api() {
    let server = MockServer::start().await;
    Mock::given(path("/responses"))
        .respond_with(ResponseTemplate::new(200).set_body_json(answer(
            plan("connect_service", Some("gmail"), Value::Null),
            vec![],
        )))
        .expect(1)
        .mount(&server)
        .await;
    let f = Fixture::new(&server).await;
    let d = f
        .core
        .draft_task_no_input("Read my Gmail messages")
        .unwrap();
    let p = f.core.access_preview(&f.conns, &d.draft_id).await.unwrap();
    assert!(p.payload_text.contains("availableServices"));
    let next = f
        .core
        .research_task_access(&f.conns, &d.draft_id, &p.payload_hash)
        .await
        .unwrap();
    let report = next.access_plan.unwrap();
    assert_eq!(report.state, "connect_service");
    assert_eq!(report.connector_id.as_deref(), Some("gmail"));
    assert!(report.api.is_none());
    assert!(!report.searched);
    assert!(!next.ready_to_build);
}

#[cfg(feature = "synthetic-runtime-tests")]
#[tokio::test]
async fn public_source_auth_refusal_and_rate_limit_stop_without_a_repair_or_alternate_source() {
    for status in [401, 403, 429] {
        let server = MockServer::start().await;
        Mock::given(path("/denied"))
            .respond_with(ResponseTemplate::new(status))
            .expect(1)
            .mount(&server)
            .await;
        let program = json!({"summary":"Read a public source","output_name":"output.txt",
            "code":"function run() { macroscope.output('output.txt', macroscope.source('source')); }",
            "web_sources":[{"name":"source","url":format!("{}/denied",server.uri())}],"connection_sources":[],"model_step":null});
        Mock::given(path("/responses")).respond_with(ResponseTemplate::new(200).set_body_json(json!({"status":"completed",
            "output":[{"type":"message","content":[{"type":"output_text","text":program.to_string()}]}],"usage":{"input_tokens":100,"output_tokens":100}}))).expect(1).mount(&server).await;
        let f = Fixture::new(&server).await;
        let runner = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("macroscope-runner");
        f.core.use_synthetic_runtime_for_tests(
            &runner,
            std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../runner/profile.sb")),
        );
        let d = f
            .core
            .draft_task_no_input("Read a public source into output.txt")
            .unwrap();
        let b = f.core.start_build(&d.draft_id, true).await.unwrap();
        let mut terminal = None;
        for _ in 0..500 {
            let s = f.core.build_state(&b.build_id).unwrap();
            if s.phase.is_terminal() {
                terminal = Some(s);
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        let s = terminal.expect("build timeout");
        assert_eq!(s.phase, BuildPhase::Failed);
        assert!(s
            .failure
            .unwrap()
            .message
            .contains("No retry or source substitution"));
        assert!(s.checks.iter().any(|c| c.id == "web_sources" && !c.passed));
    }
}

#[tokio::test]
async fn no_auth_and_documented_key_headers_use_get_without_bearer_fallback() {
    for auth in [ApiAuth::None, ApiAuth::Header] {
        let server = MockServer::start().await;
        let expects_key = auth == ApiAuth::Header;
        Mock::given(method("GET"))
            .and(path("/records"))
            .and(move |r: &wiremock::Request| {
                !r.headers.contains_key("authorization")
                    && r.body.is_empty()
                    && if expects_key {
                        r.headers
                            .get("x-api-key")
                            .is_some_and(|s| s == "SYNTHETIC-HEADER-KEY")
                    } else {
                        !r.headers.contains_key("x-api-key")
                    }
            })
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok":true})))
            .expect(1)
            .mount(&server)
            .await;
        let f = Fixture::new(&server).await;
        let a = f
            .core
            .save_api_connection(
                ApiConfig {
                    name: "Example".into(),
                    url: format!("{}/records", server.uri()),
                    auth,
                    header_name: "X-API-Key".into(),
                    docs_url: None,
                },
                if expects_key {
                    Some(Secret::new(b"SYNTHETIC-HEADER-KEY".to_vec()))
                } else {
                    None
                },
            )
            .await
            .unwrap();
        let info = f.conns.test(&a.id).await.unwrap();
        assert!(matches!(info.state, ConnectionState::Connected { .. }));
        assert!(!serde_json::to_string(&f.core.list_outbound().unwrap())
            .unwrap()
            .contains("SYNTHETIC-HEADER-KEY"));
    }
}

#[tokio::test]
async fn custom_model_routes_use_their_own_model_and_never_silently_fall_back_to_meta_search() {
    let server = MockServer::start().await;
    Mock::given(path("/chat/completions")).respond_with(|request: &wiremock::Request| {
        let body = request.body_json::<Value>().unwrap();
        let value = if body["response_format"]["json_schema"]["name"] == "endpoint_test" { json!({"ok":true}) } else { plan("research", None, Value::Null) };
        ResponseTemplate::new(200).set_body_json(json!({"choices":[{"message":{"content":value.to_string()},"finish_reason":"stop"}],"usage":{"prompt_tokens":100,"completion_tokens":100}}))
    }).expect(2).mount(&server).await;
    let f = Fixture::new(&server).await;
    f.core
        .save_custom_endpoint(&server.uri(), "test-model")
        .await
        .unwrap();
    f.core
        .save_custom_endpoint_token("SYNTHETIC-CUSTOM-MODEL-CREDENTIAL")
        .await
        .unwrap();
    f.core.test_custom_endpoint().await.unwrap();
    f.core
        .select_route(ModelRoute::CustomEndpoint)
        .await
        .unwrap();
    let d = f.core.draft_task_no_input("Read Example records").unwrap();
    let p = f.core.access_preview(&f.conns, &d.draft_id).await.unwrap();
    assert!(!p.search_available);
    let next = f
        .core
        .research_task_access(&f.conns, &d.draft_id, &p.payload_hash)
        .await
        .unwrap();
    assert_eq!(next.access_plan.unwrap().state, "needs_research");
    assert!(server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .all(|r| r.url.path() == "/chat/completions"
            && r.body_json::<Value>().unwrap().get("tools").is_none()));
}

#[tokio::test]
async fn custom_api_test_selection_and_outbound_packet_keep_credentials_in_the_host() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/records"))
        .and(wiremock::matchers::header(
            "authorization",
            "Bearer SYNTHETIC-API-CREDENTIAL-0001",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"records":[{"id":"one"}]})))
        .expect(1)
        .mount(&server)
        .await;
    let f = Fixture::new(&server).await;
    let api = f.api(&server, "/records").await;
    let d = f
        .core
        .draft_task_no_input("Read my Example records")
        .unwrap();
    assert!(f
        .conns
        .select_for_task(&d.draft_id, std::slice::from_ref(&api.id))
        .await
        .is_err());
    f.conns.test(&api.id).await.unwrap();
    let selected = f
        .conns
        .select_for_task(&d.draft_id, std::slice::from_ref(&api.id))
        .await
        .unwrap();
    assert_eq!(selected.required_connections[0].tools[0].name, "read");
    assert_eq!(
        selected.required_connections[0].tools[0].read_only,
        Some(true)
    );
    let preview = f.core.outbound_preview(&d.draft_id).await.unwrap();
    assert!(!preview.payload_text.contains("SYNTHETIC-API-CREDENTIAL"));
    let serialized = serde_json::to_string(&f.core.privacy_overview().await.unwrap()).unwrap();
    assert!(!serialized.contains("SYNTHETIC-API-CREDENTIAL"));
    assert!(matches!(
        f.conns
            .list()
            .iter()
            .find(|c| c.id == api.id)
            .unwrap()
            .state,
        ConnectionState::Connected { .. }
    ));
}

#[tokio::test]
async fn redirects_are_never_followed_and_no_error_echo_reaches_ui_or_history() {
    for (route, response) in [
        (
            "/redirect",
            ResponseTemplate::new(302).insert_header("location", "/trap"),
        ),
        (
            "/echo",
            ResponseTemplate::new(200)
                .set_body_json(json!({"echo":"SYNTHETIC-API-CREDENTIAL-0001"})),
        ),
        (
            "/forbidden",
            ResponseTemplate::new(403).set_body_string("SYNTHETIC-API-CREDENTIAL-0001"),
        ),
        (
            "/partial",
            ResponseTemplate::new(200).set_body_json(json!({"records":[],"offset":"more"})),
        ),
        (
            "/large",
            ResponseTemplate::new(200).set_body_bytes(vec![b'x'; 4 * 1024 * 1024 + 1]),
        ),
    ] {
        let server = MockServer::start().await;
        Mock::given(path(route))
            .respond_with(response)
            .mount(&server)
            .await;
        let f = Fixture::new(&server).await;
        let api = f.api(&server, route).await;
        let error = f.conns.test(&api.id).await.unwrap_err().to_string();
        assert!(!error.contains("SYNTHETIC-API-CREDENTIAL"));
        assert!(!serde_json::to_string(&f.conns.list())
            .unwrap()
            .contains("SYNTHETIC-API-CREDENTIAL"));
        assert!(
            !serde_json::to_string(&f.core.privacy_overview().await.unwrap())
                .unwrap()
                .contains("SYNTHETIC-API-CREDENTIAL")
        );
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
    }
}

#[tokio::test]
async fn rate_limits_apply_across_connections_on_the_same_origin_and_local_only_sends_nothing() {
    let server = MockServer::start().await;
    Mock::given(path("/limited"))
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "120"))
        .mount(&server)
        .await;
    Mock::given(path("/records"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&server)
        .await;
    let f = Fixture::new(&server).await;
    let a = f.api(&server, "/limited").await;
    let b = f.api(&server, "/records").await;
    assert!(f.conns.test(&a.id).await.is_err());
    assert!(f
        .conns
        .test(&b.id)
        .await
        .unwrap_err()
        .to_string()
        .contains("rate limited"));
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
    f.core.set_local_only(true).await.unwrap();
    assert!(f.conns.test(&b.id).await.is_err());
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn emergency_stop_cancels_inflight_api_io_and_acknowledges_the_barrier() {
    let server = MockServer::start().await;
    Mock::given(path("/slow"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({}))
                .set_delay(std::time::Duration::from_secs(3)),
        )
        .mount(&server)
        .await;
    let f = Fixture::new(&server).await;
    let a = f.api(&server, "/slow").await;
    let conns = f.conns.clone();
    let id = a.id.clone();
    let pending = tokio::spawn(async move { conns.test(&id).await });
    for _ in 0..100 {
        if !server.received_requests().await.unwrap().is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
    tokio::time::timeout(
        std::time::Duration::from_secs(1),
        f.core.set_local_only(true),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(pending.await.unwrap().is_err());
}

#[tokio::test]
async fn known_suitable_connection_is_reused_without_search_or_another_account_read() {
    let server = MockServer::start().await;
    Mock::given(path("/records"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"records":[]})))
        .expect(1)
        .mount(&server)
        .await;
    let f = Fixture::new(&server).await;
    let a = f.api(&server, "/records").await;
    f.conns.test(&a.id).await.unwrap();
    Mock::given(method("POST"))
        .and(path("/responses"))
        .respond_with(ResponseTemplate::new(200).set_body_json(answer(
            plan("existing_connection", Some(&a.id), Value::Null),
            vec![],
        )))
        .expect(1)
        .mount(&server)
        .await;
    let d = f
        .core
        .draft_task_no_input("Read my Example records")
        .unwrap();
    let p = f.core.access_preview(&f.conns, &d.draft_id).await.unwrap();
    assert!(!p.payload_text.contains("SYNTHETIC-API-CREDENTIAL"));
    let next = f
        .core
        .research_task_access(&f.conns, &d.draft_id, &p.payload_hash)
        .await
        .unwrap();
    assert_eq!(
        next.access_plan.as_ref().unwrap().state,
        "existing_connection"
    );
    assert!(!next.access_plan.as_ref().unwrap().searched);
    assert!(!next.ready_to_build); // Still needs an explicit per-tool selection.
    let selected = f.conns.select_for_task(&d.draft_id, &[a.id]).await.unwrap();
    assert!(selected.ready_to_build);
    let requests = server.received_requests().await.unwrap();
    let model = requests
        .iter()
        .find(|r| r.method == "POST")
        .unwrap()
        .body_json::<Value>()
        .unwrap();
    assert!(model.get("tools").is_none());
}

#[tokio::test]
async fn missing_access_uses_hosted_search_and_fetched_documentation_before_showing_setup() {
    let server = MockServer::start().await;
    Mock::given(path("/responses"))
        .and(|r: &wiremock::Request| r.body_json::<Value>().unwrap().get("tools").is_none())
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(answer(plan("research", None, Value::Null), vec![])),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path("/responses"))
        .and(|r: &wiremock::Request| r.body_json::<Value>().unwrap().get("tools").is_some())
        .respond_with(ResponseTemplate::new(200).set_body_json(answer(
            plan("custom_api", None, proposal(&server, "bearer")),
            vec![format!("{}/docs", server.uri())],
        )))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path("/docs"))
        .respond_with(ResponseTemplate::new(200).set_body_string(format!(
            "Official API: {}/records accepts a GET with a Bearer credential.",
            server.uri()
        )))
        .expect(1)
        .mount(&server)
        .await;
    let f = Fixture::new(&server).await;
    let d = f
        .core
        .draft_task_no_input("Read private Example records")
        .unwrap();
    let p = f.core.access_preview(&f.conns, &d.draft_id).await.unwrap();
    let next = f
        .core
        .research_task_access(&f.conns, &d.draft_id, &p.payload_hash)
        .await
        .unwrap();
    let report = next.access_plan.unwrap();
    assert_eq!(report.state, "needs_connection");
    assert!(report.searched);
    assert!(report.api.is_some());
    assert!(!next.ready_to_build);
    assert!(f.conns.allow_documentation_url(&report.sources[0]) == false); // HTTP is test-only and never browser-authorized.
    let requests = server.received_requests().await.unwrap();
    let search = requests
        .iter()
        .filter_map(|r| r.body_json::<Value>().ok())
        .find(|v| v.get("tools").is_some())
        .unwrap();
    assert_eq!(search["max_tool_calls"], 3);
    assert_eq!(search["tools"][0]["type"], "web_search");
    assert!(!search["input"][1]["content"]
        .as_str()
        .unwrap()
        .contains("private"));
    assert!(requests.iter().all(|r| r.url.path() != "/records"));
}

#[tokio::test]
async fn invented_documentation_cannot_produce_a_connection_box_and_oauth_needs_an_adapter() {
    for auth in ["bearer", "oauth"] {
        let server = MockServer::start().await;
        let docs = if auth == "oauth" {
            format!("API endpoint {} uses OAuth", server.uri())
        } else {
            "No such endpoint is documented".into()
        };
        Mock::given(path("/docs"))
            .respond_with(ResponseTemplate::new(200).set_body_string(docs))
            .mount(&server)
            .await;
        Mock::given(path("/responses"))
            .respond_with(ResponseTemplate::new(200).set_body_json(answer(
                plan("custom_api", None, proposal(&server, auth)),
                vec![],
            )))
            .mount(&server)
            .await;
        let f = Fixture::new(&server).await;
        let d = f.core.draft_task_no_input("Read Example records").unwrap();
        let p = f.core.access_preview(&f.conns, &d.draft_id).await.unwrap();
        let next = f
            .core
            .research_task_access(&f.conns, &d.draft_id, &p.payload_hash)
            .await
            .unwrap();
        let report = next.access_plan.unwrap();
        if auth == "oauth" {
            assert_eq!(report.state, "needs_adapter");
            assert!(report.api.unwrap().config().is_none());
        } else {
            assert_eq!(report.state, "needs_research");
            assert!(report.api.is_none());
        }
        assert!(!next.ready_to_build);
    }
}

#[tokio::test]
async fn stale_review_and_private_network_targets_fail_before_dispatch() {
    let server = MockServer::start().await;
    let f = Fixture::new(&server).await;
    let d = f.core.draft_task_no_input("Read Example records").unwrap();
    let p = f.core.access_preview(&f.conns, &d.draft_id).await.unwrap();
    f.core.set_local_only(true).await.unwrap();
    assert!(f
        .core
        .research_task_access(&f.conns, &d.draft_id, &p.payload_hash)
        .await
        .is_err());
    for url in [
        "https://127.0.0.1/private",
        "https://169.254.169.254/latest",
        "https://api.example.com?api_key=hidden",
    ] {
        assert!(f
            .core
            .save_api_connection(
                ApiConfig {
                    name: "Unsafe".into(),
                    url: url.into(),
                    auth: ApiAuth::None,
                    header_name: String::new(),
                    docs_url: None
                },
                None
            )
            .await
            .is_err());
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[cfg(feature = "synthetic-runtime-tests")]
#[tokio::test]
async fn custom_api_is_read_by_the_existing_approved_build_and_saved_tool_pipeline() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/records"))
        .and(wiremock::matchers::header(
            "authorization",
            "Bearer SYNTHETIC-API-CREDENTIAL-0001",
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"records":[{"id":"actual-api-result"}]})),
        )
        .expect(3)
        .mount(&server)
        .await;
    let f = Fixture::new(&server).await;
    let runner = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("macroscope-runner");
    f.core.use_synthetic_runtime_for_tests(
        &runner,
        std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../runner/profile.sb")),
    );
    let a = f.api(&server, "/records").await;
    f.conns.test(&a.id).await.unwrap();
    let program = json!({"summary":"Read records","output_name":"records.txt",
        "code":"function run() { const data = JSON.parse(macroscope.source('records')); macroscope.output('records.txt', data.records[0].id + '\\n'); }",
        "web_sources":[],"connection_sources":[{"name":"records","connectorId":a.id,"toolName":"read","argsJson":"{}"}],"model_step":null});
    Mock::given(method("POST")).and(path("/responses")).respond_with(ResponseTemplate::new(200).set_body_json(json!({"status":"completed",
        "output":[{"type":"message","content":[{"type":"output_text","text":program.to_string()}]}],"usage":{"input_tokens":100,"output_tokens":100}}))).mount(&server).await;
    let d = f
        .core
        .draft_task_no_input("Read my Example records into records.txt")
        .unwrap();
    f.core
        .update_task(&d.draft_id, TaskKind::Program, "records.txt", None)
        .unwrap();
    f.conns
        .select_for_task(&d.draft_id, &[a.id.clone()])
        .await
        .unwrap();
    let build = f.core.start_build(&d.draft_id, true).await.unwrap();
    let mut terminal = None;
    for _ in 0..500 {
        let state = f.core.build_state(&build.build_id).unwrap();
        if state.phase.is_terminal() {
            terminal = Some(state);
            break;
        }
        if state.phase == BuildPhase::Candidate {
            if let Some(p) = &state.program {
                f.core.approve_build_sources(&build.build_id, p).unwrap();
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let state = terminal.expect("build timeout");
    assert_eq!(state.phase, BuildPhase::Ready, "{state:?}");
    let tool = f
        .core
        .save_tool(&build.build_id, "API records")
        .await
        .unwrap();
    let dir = f._dir.path().join("output");
    std::fs::create_dir_all(&dir).unwrap();
    let folder = f.core.register_picked_dir(&dir).unwrap();
    let staged = f
        .core
        .stage_run_no_input(&tool.summary_row.id, &folder.handle_id, "records.txt")
        .await
        .unwrap();
    let StageResult::Staged(staged) = staged else {
        panic!("{staged:?}")
    };
    assert!(staged
        .preview_text
        .unwrap()
        .starts_with("actual-api-result"));
    assert!(!serde_json::to_string(&f.core.list_outbound().unwrap())
        .unwrap()
        .contains("SYNTHETIC-API-CREDENTIAL"));
    f.core.discard_run(&staged.run_id).unwrap();
    Mock::given(method("GET"))
        .and(path("/records"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"records":[]}))
                .set_delay(std::time::Duration::from_secs(3)),
        )
        .with_priority(1)
        .expect(1)
        .mount(&server)
        .await;
    let core = f.core.clone();
    let tool_id = tool.summary_row.id.clone();
    let folder_id = folder.handle_id.clone();
    let pending = tokio::spawn(async move {
        core.stage_run_no_input(&tool_id, &folder_id, "pending.txt")
            .await
    });
    for _ in 0..100 {
        if server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|r| r.url.path() == "/records")
            .count()
            == 4
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    assert_eq!(
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|r| r.url.path() == "/records")
            .count(),
        4
    );
    tokio::time::timeout(
        std::time::Duration::from_secs(1),
        f.core.set_local_only(true),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(pending.await.unwrap().is_err());
    f.core.set_local_only(false).await.unwrap();
    f.conns.disconnect(&a.id).await.unwrap();
    let revoked = f
        .core
        .stage_run_no_input(&tool.summary_row.id, &folder.handle_id, "again.txt")
        .await;
    assert!(matches!(revoked, Err(_) | Ok(StageResult::Failed(_))));
    assert_eq!(
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|r| r.url.path() == "/records")
            .count(),
        4
    );
}
