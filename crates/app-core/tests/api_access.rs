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
                    extra_header_names: vec![],
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
    json!({"route":route,"service":"Example","operation":"read records","summary":"Read the records needed by this task.","connectorId":id,"api":api,"searchQuery":""})
}
fn proposal(server: &MockServer, auth: &str) -> Value {
    json!({"name":"Example records","url":format!("{}/records",server.uri()),"auth":auth,"headerName":"X-API-Key","docsUrl":format!("{}/docs",server.uri()),
        "method":"GET","setupInstructions":["Create a read-only credential in the provider account."],"prerequisites":["Access to the chosen records"],"coverage":"Complete records; verify account permissions and quotas.","extraHeaderNames":[],"cost":"unknown"})
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
                    extra_header_names: vec![],
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
            // Nothing to set up was found; the build may still try public sources, so it is not blocked.
            assert!(next.ready_to_build);
        }
        if auth == "oauth" {
            assert!(!next.ready_to_build);
        }
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
                    docs_url: None,
                    extra_header_names: vec![],
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

#[tokio::test]
async fn an_api_that_needs_an_app_id_header_sends_both_keys_from_the_vault_and_never_echoes_them() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/accounts"))
        .and(|r: &wiremock::Request| {
            r.headers.get("authorization").is_some_and(|v| v == "Bearer SYNTHETIC-USER-TOKEN")
                && r.headers.get("x-app-id").is_some_and(|v| v == "SYNTHETIC-APP-ID")
        })
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"items":[{"name":"Everyday"}]})))
        .expect(1)
        .mount(&server)
        .await;
    let f = Fixture::new(&server).await;
    let config = |extras: Vec<&str>| ApiConfig {
        name: "Bank accounts".into(),
        url: format!("{}/accounts", server.uri()),
        auth: ApiAuth::Bearer,
        header_name: String::new(),
        docs_url: None,
        extra_header_names: extras.into_iter().map(String::from).collect(),
    };
    // A second header with no value, a repeated header, and a third header are all refused.
    assert!(f.core.save_api_connection(config(vec!["X-App-Id"]), Some(Secret::new(b"SYNTHETIC-USER-TOKEN".to_vec()))).await.is_err());
    assert!(f.core.save_api_connection_with_extras(config(vec!["Authorization"]), Some(Secret::new(b"t".to_vec())), vec![Secret::new(b"v".to_vec())]).await.is_err());
    assert!(f.core.save_api_connection_with_extras(config(vec!["X-A", "X-B", "X-C"]), Some(Secret::new(b"t".to_vec())), (1..=3).map(|n| Secret::new(vec![b'0' + n])).collect()).await.is_err());
    let a = f
        .core
        .save_api_connection_with_extras(
            config(vec!["X-App-Id"]),
            Some(Secret::new(b"SYNTHETIC-USER-TOKEN".to_vec())),
            vec![Secret::new(b"SYNTHETIC-APP-ID".to_vec())],
        )
        .await
        .unwrap();
    let info = f.conns.test(&a.id).await.unwrap();
    assert!(matches!(info.state, ConnectionState::Connected { .. }));
    let outbound = serde_json::to_string(&f.core.list_outbound().unwrap()).unwrap();
    assert!(!outbound.contains("SYNTHETIC-APP-ID") && !outbound.contains("SYNTHETIC-USER-TOKEN"));
}

#[tokio::test]
async fn a_key_in_the_web_address_and_a_plainly_named_header_both_work_and_stay_out_of_every_record() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/me/posts"))
        .and(|r: &wiremock::Request| {
            let q: std::collections::HashMap<_, _> = r.url.query_pairs().into_owned().collect();
            q.get("access_token").is_some_and(|v| v == "SYNTHETIC-USER-TOKEN") && q.get("app_id").is_some_and(|v| v == "SYNTHETIC-APP-ID")
        })
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data":[{"id":"1"}]})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/streams"))
        .and(|r: &wiremock::Request| {
            r.headers.get("client-id").is_some_and(|v| v == "SYNTHETIC-CLIENT-ID") && r.headers.get("authorization").is_some_and(|v| v == "Bearer SYNTHETIC-USER-TOKEN")
        })
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data":[]})))
        .expect(1)
        .mount(&server)
        .await;
    let f = Fixture::new(&server).await;
    let config = |path: &str, auth: ApiAuth, name: &str, extra: &str| ApiConfig {
        name: "Social".into(),
        url: format!("{}{path}", server.uri()),
        auth,
        header_name: name.into(),
        docs_url: None,
        extra_header_names: vec![extra.into()],
    };
    // A key already pasted into the address is refused: it would be saved in plain view.
    assert!(f.core.save_api_connection_with_extras(config("/me/posts?access_token=x", ApiAuth::Query, "access_token", "app_id"), Some(Secret::new(b"t".to_vec())), vec![Secret::new(b"v".to_vec())]).await.is_err());
    assert!(f.core.save_api_connection_with_extras(config("/me/posts", ApiAuth::Query, "bad name", "app_id"), Some(Secret::new(b"t".to_vec())), vec![Secret::new(b"v".to_vec())]).await.is_err());
    let q = f.core.save_api_connection_with_extras(config("/me/posts", ApiAuth::Query, "access_token", "app_id"), Some(Secret::new(b"SYNTHETIC-USER-TOKEN".to_vec())), vec![Secret::new(b"SYNTHETIC-APP-ID".to_vec())]).await.unwrap();
    assert!(matches!(f.conns.test(&q.id).await.unwrap().state, ConnectionState::Connected { .. }));
    assert!(!q.mcp_url.contains("SYNTHETIC"), "the saved address never carries the key");
    let h = f.core.save_api_connection_with_extras(config("/streams", ApiAuth::Bearer, "", "Client-Id"), Some(Secret::new(b"SYNTHETIC-USER-TOKEN".to_vec())), vec![Secret::new(b"SYNTHETIC-CLIENT-ID".to_vec())]).await.unwrap();
    assert!(matches!(f.conns.test(&h.id).await.unwrap().state, ConnectionState::Connected { .. }));
    let outbound = serde_json::to_string(&f.core.list_outbound().unwrap()).unwrap();
    assert!(!outbound.contains("SYNTHETIC"));
}

#[tokio::test]
async fn an_api_already_saved_for_the_proposed_address_is_reused_instead_of_asking_for_setup_again() {
    let server = MockServer::start().await;
    Mock::given(path("/responses"))
        .respond_with(ResponseTemplate::new(200).set_body_json(answer(plan("custom_api", None, proposal(&server, "bearer")), vec![])))
        .mount(&server)
        .await;
    Mock::given(method("GET")).and(path("/records")).respond_with(ResponseTemplate::new(200).set_body_json(json!({"items":[]}))).mount(&server).await;
    let f = Fixture::new(&server).await;
    let saved = f.api(&server, "/records").await;
    let d = f.core.draft_task_no_input("Read my Example records").unwrap();
    // Saved but never tested: say so, and do not offer a second setup box for the same address.
    let p = f.core.access_preview(&f.conns, &d.draft_id).await.unwrap();
    let next = f.core.research_task_access(&f.conns, &d.draft_id, &p.payload_hash).await.unwrap();
    let report = next.access_plan.as_ref().unwrap();
    assert!(report.summary.contains("not passed its test") && report.api.is_none() && report.state != "needs_connection");
    // Once it passes its test, the task simply uses it.
    f.conns.test(&saved.id).await.unwrap();
    let p = f.core.access_preview(&f.conns, &d.draft_id).await.unwrap();
    assert!(p.payload_text.contains(&format!("{}/records", server.uri())), "the model is told what each connected API reads");
    let next = f.core.research_task_access(&f.conns, &d.draft_id, &p.payload_hash).await.unwrap();
    let report = next.access_plan.as_ref().unwrap();
    assert_eq!(report.state, "existing_connection");
    assert_eq!(report.connector_id.as_deref(), Some(saved.id.as_str()));
}

#[tokio::test]
async fn a_model_suggests_example_calls_that_run_with_the_keys_and_keep_only_field_names() {
    let server = MockServer::start().await;
    Mock::given(path("/responses"))
        .respond_with(ResponseTemplate::new(200).set_body_json(answer(
            json!({"examples":[
                {"label":"List accounts","path":"/accounts","note":"Your accounts and balances."},
                {"label":"Other site","path":"//evil.example/steal","note":"x"},
                {"label":"With query","path":"/accounts?access_token=x","note":"x"},
                {"label":"Missing","path":"/missing","note":"A path that does not exist."}]}),
            vec![],
        )))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/accounts"))
        .and(|r: &wiremock::Request| r.headers.get("authorization").is_some_and(|v| v == "Bearer SYNTHETIC-API-CREDENTIAL-0001"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"items":[{"name":"SYNTHETIC-PRIVATE-NAME","balance":{"available":123}}]})))
        .mount(&server)
        .await;
    Mock::given(method("GET")).and(path("/records")).respond_with(ResponseTemplate::new(200).set_body_json(json!({"items":[]}))).mount(&server).await;
    let f = Fixture::new(&server).await;
    let saved = f.api(&server, "/records").await;
    assert!(matches!(f.conns.test(&saved.id).await.unwrap().state, ConnectionState::Connected { .. }));
    let examples = f.core.suggest_api_examples(&saved.id).await.unwrap();
    // Off-site, query-string and non-path suggestions are dropped before anything is shown.
    assert_eq!(examples.iter().map(|e| e.label.as_str()).collect::<Vec<_>>(), ["List accounts", "Missing"]);
    let stored = f.core.run_api_example(&saved.id, &examples[0].url).await.unwrap();
    assert_eq!(stored[0].ok, Some(true));
    let result = stored[0].result.clone().unwrap();
    assert!(result.contains("balance") && result.contains("available") && !result.contains("SYNTHETIC-PRIVATE-NAME") && !result.contains("123"), "{result}");
    // A failing example is reported, and leaves the working connection working.
    let stored = f.core.run_api_example(&saved.id, &examples[1].url).await.unwrap();
    assert_eq!(stored[1].ok, Some(false));
    assert!(matches!(f.conns.list().into_iter().find(|c| c.id == saved.id).unwrap().state, ConnectionState::Connected { .. }));
    assert_eq!(f.core.api_examples(&saved.id).unwrap().len(), 2, "kept beside the API");
    // An address that was never suggested cannot be run with the keys.
    assert!(f.core.run_api_example(&saved.id, &format!("{}/anything", server.uri())).await.is_err());
}

#[tokio::test]
async fn debugging_a_connection_says_whether_the_address_or_the_keys_are_wrong() {
    let server = MockServer::start().await;
    Mock::given(method("GET")).and(path("/v1")).respond_with(ResponseTemplate::new(404)).mount(&server).await;
    Mock::given(method("GET")).and(path("/v1/accounts"))
        .and(|r: &wiremock::Request| !r.headers.get("authorization").is_some_and(|v| v == "Bearer SYNTHETIC-API-CREDENTIAL-0001"))
        .respond_with(ResponseTemplate::new(401)).mount(&server).await;
    Mock::given(method("GET")).and(path("/v1/accounts"))
        .and(|r: &wiremock::Request| r.headers.get("authorization").is_some_and(|v| v == "Bearer SYNTHETIC-API-CREDENTIAL-0001"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"items":[]}))).mount(&server).await;
    let f = Fixture::new(&server).await;
    // The bare base address: wrong whatever the keys are.
    let base = f.api(&server, "/v1").await;
    let report = f.core.debug_api(&base.id).await.unwrap();
    assert!(report.try_examples && report.verdict.contains("address itself is wrong"), "{:?}", report);
    assert_eq!(report.steps.len(), 2);
    // The right address with the right key works.
    let good = f.api(&server, "/v1/accounts").await;
    let report = f.core.debug_api(&good.id).await.unwrap();
    assert!(report.verdict.starts_with("It works") && !report.try_examples, "{}", report.verdict);
    // The right address with a wrong key: the address exists, the keys are what to re-check.
    let wrong = f.core.save_api_connection(ApiConfig { name: "Wrong key".into(), url: format!("{}/v1/accounts", server.uri()), auth: ApiAuth::Bearer, header_name: String::new(), docs_url: None, extra_header_names: vec![] }, Some(Secret::new(b"SYNTHETIC-WRONG".to_vec()))).await.unwrap();
    let report = f.core.debug_api(&wrong.id).await.unwrap();
    assert!(report.verdict.contains("keys were not accepted") && !report.try_examples, "{}", report.verdict);
    assert!(report.steps[0].detail.contains("address exists"));
    let all = serde_json::to_string(&report).unwrap();
    assert!(!all.contains("SYNTHETIC"));
}

#[tokio::test]
async fn debugging_repairs_a_key_pasted_with_bearer_or_quotes_and_keeps_it_only_if_that_works() {
    let server = MockServer::start().await;
    Mock::given(method("GET")).and(path("/v1/accounts"))
        .and(|r: &wiremock::Request| r.headers.get("authorization").is_some_and(|v| v == "Bearer SYNTHETIC-API-CREDENTIAL-0001"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"items":[]}))).mount(&server).await;
    Mock::given(method("GET")).and(path("/v1/accounts"))
        .and(|r: &wiremock::Request| !r.headers.get("authorization").is_some_and(|v| v == "Bearer SYNTHETIC-API-CREDENTIAL-0001"))
        .respond_with(ResponseTemplate::new(401)).mount(&server).await;
    let f = Fixture::new(&server).await;
    let save = |key: &str| f.core.save_api_connection(ApiConfig { name: "Pasted".into(), url: format!("{}/v1/accounts", server.uri()), auth: ApiAuth::Bearer, header_name: String::new(), docs_url: None, extra_header_names: vec![] }, Some(Secret::new(key.as_bytes().to_vec())));
    for pasted in ["Bearer Bearer SYNTHETIC-API-CREDENTIAL-0001", "\"SYNTHETIC-API-CREDENTIAL-0001\"", "Authorization: Bearer SYNTHETIC-API-CREDENTIAL-0001  "] {
        let saved = save(pasted).await.unwrap();
        let report = f.core.debug_api(&saved.id).await.unwrap();
        assert!(report.fixed && report.verdict.contains("repaired"), "{pasted}: {report:?}");
        // The repair is kept: the connection now passes its own test.
        assert!(matches!(f.conns.test(&saved.id).await.unwrap().state, ConnectionState::Connected { .. }));
        assert!(!serde_json::to_string(&report).unwrap().contains("SYNTHETIC"));
    }
    // A key that is simply wrong is put back exactly as it was and the hints stay.
    let wrong = save("Bearer SYNTHETIC-WRONG").await.unwrap();
    let report = f.core.debug_api(&wrong.id).await.unwrap();
    assert!(!report.fixed && report.verdict.contains("not accepted"), "{report:?}");
    assert!(report.steps.iter().any(|s| s.detail.contains("put your keys back")));
}

#[tokio::test]
async fn with_no_documentation_and_no_model_ideas_the_common_places_are_tested_and_a_working_one_is_found() {
    let server = MockServer::start().await;
    Mock::given(path("/responses")).respond_with(ResponseTemplate::new(200).set_body_json(answer(json!({"examples":[]}), vec![]))).mount(&server).await;
    Mock::given(method("GET")).and(path("/v1")).respond_with(ResponseTemplate::new(404)).mount(&server).await;
    Mock::given(method("GET")).and(path("/v1/accounts")).respond_with(ResponseTemplate::new(200).set_body_json(json!({"items":[{"name":"x"}]}))).mount(&server).await;
    Mock::given(method("GET")).and(path("/v1/me")).respond_with(ResponseTemplate::new(404)).mount(&server).await;
    let f = Fixture::new(&server).await;
    let saved = f.api(&server, "/v1").await;
    let examples = f.core.suggest_api_examples(&saved.id).await.unwrap();
    assert!(examples.iter().any(|e| e.url.ends_with("/v1/accounts")) && examples.iter().all(|e| e.url.starts_with(&server.uri())), "{examples:?}");
    let mut last = examples.clone();
    for e in &examples { last = f.core.run_api_example(&saved.id, &e.url).await.unwrap(); }
    let working: Vec<_> = last.iter().filter(|e| e.ok == Some(true)).map(|e| e.url.clone()).collect();
    assert_eq!(working.len(), 1);
    assert!(working[0].ends_with("/v1/accounts"));
}

#[tokio::test]
async fn working_keys_are_tried_on_another_address_of_the_same_service_before_any_setup_is_proposed() {
    let server = MockServer::start().await;
    Mock::given(path("/responses")).respond_with(ResponseTemplate::new(200).set_body_json(answer(plan("custom_api", None, proposal(&server, "bearer")), vec![]))).mount(&server).await;
    Mock::given(path("/docs")).respond_with(ResponseTemplate::new(200).set_body_string(format!("GET {}/records", server.uri()))).mount(&server).await;
    Mock::given(method("GET")).and(path("/me")).respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok":1}))).mount(&server).await;
    Mock::given(method("GET")).and(path("/records"))
        .and(|r: &wiremock::Request| r.headers.get("authorization").is_some_and(|v| v == "Bearer SYNTHETIC-API-CREDENTIAL-0001"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"items":[{"balance":{"available":1}}]}))).mount(&server).await;
    let f = Fixture::new(&server).await;
    let saved = f.api(&server, "/me").await;
    f.conns.test(&saved.id).await.unwrap();
    let d = f.core.draft_task_no_input("Read my Example records").unwrap();
    let p = f.core.access_preview(&f.conns, &d.draft_id).await.unwrap();
    let next = f.core.research_task_access(&f.conns, &d.draft_id, &p.payload_hash).await.unwrap();
    let report = next.access_plan.as_ref().unwrap();
    assert_eq!(report.state, "same_service_works", "{report:?}");
    assert_eq!(report.reuse_keys_from.as_deref(), Some(saved.id.as_str()));
    assert!(report.summary.contains("already work on this service"));
    // One click records that path on the same connection: no second connection, no second set of keys.
    let before = f.conns.list().len();
    let kept = f.core.add_api_address(&saved.id, &report.api.as_ref().unwrap().url).await.unwrap();
    assert_eq!(kept.id, saved.id);
    assert_eq!(f.conns.list().len(), before);
    let known = f.core.api_examples(&saved.id).unwrap();
    assert!(known.iter().any(|e| e.url.ends_with("/records") && e.ok == Some(true)));
    // The planner is taught the path, with field names only.
    let tools = f.conns.preview_tools(&saved.id).await.unwrap();
    assert!(tools[0].description.contains("/records returns") && tools[0].description.contains("balance"), "{}", tools[0].description);
    assert!(f.core.add_api_address(&saved.id, "https://elsewhere.example/records").await.is_err());
    // A path the keys cannot read is not kept.
    Mock::given(method("GET")).and(path("/nope")).respond_with(ResponseTemplate::new(404)).mount(&server).await;
    assert!(f.core.add_api_address(&saved.id, &format!("{}/nope", server.uri())).await.is_err());
    assert!(!f.core.api_examples(&saved.id).unwrap().iter().any(|e| e.url.ends_with("/nope")));
    // Reads may name any path on the service, and nothing else.
    let target = f.core.validate_api_read(&saved.id, &saved.mcp_url, "read", &json!({"path":"/records"})).unwrap();
    assert_eq!(target, format!("{}/records", server.uri()));
    assert_eq!(f.core.validate_api_read(&saved.id, &saved.mcp_url, "read", &json!({})).unwrap(), saved.mcp_url);
    for bad in [json!({"path":"records"}), json!({"path":"//evil.example/x"}), json!({"path":"/a?access_token=1"}), json!({"url":"https://evil.example/x"}), json!({"path":5})] {
        assert!(f.core.validate_api_read(&saved.id, &saved.mcp_url, "read", &bad).is_err(), "{bad}");
    }
}

#[tokio::test]
async fn a_list_in_pages_is_read_in_full_and_unknown_paging_fails() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/transactions"))
        .and(wiremock::matchers::query_param("cursor", "abc"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"items":[{"id":2}],"cursor":{"next":null}})))
        .expect(1..)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/transactions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"items":[{"id":1}],"cursor":{"next":"abc"}})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/more"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"items":[{"id":1}],"has_more":true})))
        .mount(&server)
        .await;
    let f = Fixture::new(&server).await;
    let paged = f.api(&server, "/transactions").await;
    assert!(matches!(f.conns.test(&paged.id).await.unwrap().state, ConnectionState::Connected { .. }));
    let target = f.core.validate_api_read(&paged.id, &paged.mcp_url, "read", &json!({})).unwrap();
    assert_eq!(target, paged.mcp_url);
    let more = f.api(&server, "/more").await;
    let err = f.conns.test(&more.id).await.unwrap_err().to_string();
    assert!(err.contains("one page of a longer list"), "{err}");
}

#[tokio::test]
async fn a_task_with_no_named_service_is_researched_by_its_problem_and_prefers_free_personal_access() {
    let server = MockServer::start().await;
    let query = "free personal API to read bank account transactions New Zealand";
    let mut first = plan("research", None, Value::Null);
    first["service"] = json!("");
    first["searchQuery"] = json!(query);
    Mock::given(path("/responses"))
        .and(|r: &wiremock::Request| r.body_json::<Value>().unwrap().get("tools").is_none())
        .respond_with(ResponseTemplate::new(200).set_body_json(answer(first, vec![])))
        .expect(1)
        .mount(&server)
        .await;
    let mut api = proposal(&server, "bearer");
    api["cost"] = json!("free_personal");
    Mock::given(path("/responses"))
        .and(|r: &wiremock::Request| r.body_json::<Value>().unwrap().get("tools").is_some())
        .respond_with(ResponseTemplate::new(200).set_body_json(answer(
            plan("custom_api", None, api),
            vec![format!("{}/docs", server.uri())],
        )))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path("/docs"))
        .respond_with(ResponseTemplate::new(200).set_body_string(format!("Official API: {}/records", server.uri())))
        .mount(&server)
        .await;
    let f = Fixture::new(&server).await;
    let d = f.core.draft_task_no_input("Get my bank statements").unwrap();
    let p = f.core.access_preview(&f.conns, &d.draft_id).await.unwrap();
    let report = f
        .core
        .research_task_access(&f.conns, &d.draft_id, &p.payload_hash)
        .await
        .unwrap()
        .access_plan
        .unwrap();
    assert!(report.searched, "an empty service must still search by the problem");
    assert_eq!(report.api.unwrap().cost, "free_personal");
    let requests = server.received_requests().await.unwrap();
    let search = requests
        .iter()
        .filter_map(|r| r.body_json::<Value>().ok())
        .find(|v| v.get("tools").is_some())
        .unwrap();
    let system = search["input"][0]["content"].as_str().unwrap();
    let user = search["input"][1]["content"].as_str().unwrap();
    assert!(user.contains(query), "the search runs on the problem phrase");
    assert!(user.contains("prefer a free option"));
    assert!(system.contains("free personal-access API") && system.contains("paid, partner-only or approval/accreditation-required"));
    assert!(system.contains("never by guessing a brand"));
}

#[tokio::test]
async fn a_chatty_answer_wrapped_in_prose_is_trimmed_not_rejected() {
    let server = MockServer::start().await;
    let mut api = proposal(&server, "bearer");
    api["setupInstructions"] = json!((0..12).map(|n| format!("Step {n}: {}", "x".repeat(700))).collect::<Vec<_>>());
    let mut chatty = plan("custom_api", None, api);
    chatty["summary"] = json!("s".repeat(2000));
    // The searching call answers with a sentence and a code fence around the JSON.
    let wrapped = format!("Here is what I found:\n```json\n{chatty}\n```\nHope that helps.");
    Mock::given(path("/responses"))
        .and(|r: &wiremock::Request| r.body_json::<Value>().unwrap().get("tools").is_none())
        .respond_with(ResponseTemplate::new(200).set_body_json(answer(plan("research", None, Value::Null), vec![])))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path("/responses"))
        .and(|r: &wiremock::Request| r.body_json::<Value>().unwrap().get("tools").is_some())
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"status":"completed",
            "output":[
                {"type":"web_search_call","status":"completed","results":[{"url":format!("{}/docs",server.uri()),"type":"text_result"}]},
                {"type":"message","content":[{"type":"output_text","text":wrapped}]}],
            "usage":{"input_tokens":100,"output_tokens":100}})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path("/docs"))
        .respond_with(ResponseTemplate::new(200).set_body_string(format!("Official API: {}/records", server.uri())))
        .mount(&server)
        .await;
    let f = Fixture::new(&server).await;
    let d = f.core.draft_task_no_input("Read Example records").unwrap();
    let p = f.core.access_preview(&f.conns, &d.draft_id).await.unwrap();
    let report = f
        .core
        .research_task_access(&f.conns, &d.draft_id, &p.payload_hash)
        .await
        .expect("a verbose, wrapped answer must not fail the check")
        .access_plan
        .unwrap();
    assert_eq!(report.state, "needs_connection");
    let api = report.api.unwrap();
    assert_eq!(api.setup_instructions.len(), 8);
    assert!(api.setup_instructions.iter().all(|l| l.chars().count() <= 500));
    assert!(report.summary.chars().count() <= 1600);
}

#[test]
fn a_path_without_the_services_version_prefix_reads_under_it() {
    use macroscope_app_core::api::api_base_path;
    assert_eq!(api_base_path("https://api.akahu.io/v1/me"), "/v1");
    assert_eq!(api_base_path("https://x.example/api/v2/users"), "/api/v2");
    assert_eq!(api_base_path("https://x.example/records"), "");
    assert_eq!(api_base_path("https://x.example/v1.2/a/b"), "/v1.2");
}

#[tokio::test]
async fn learning_an_api_for_a_task_reads_its_paths_once_and_teaches_the_field_names() {
    let server = MockServer::start().await;
    for (p, body) in [("/v1/me", json!({"item":{"name":"x"}})), ("/v1/accounts", json!({"items":[{"name":"Everyday","balance":{"current":12.5}}]}))] {
        Mock::given(method("GET")).and(path(p)).respond_with(ResponseTemplate::new(200).set_body_json(body)).mount(&server).await;
    }
    let f = Fixture::new(&server).await;
    let saved = f.api(&server, "/v1/me").await;
    f.conns.test(&saved.id).await.unwrap();
    let learned = f.core.learn_api_for_task(&saved.id, "Show my account balances").await.unwrap();
    let accounts = learned.iter().find(|e| e.url.ends_with("/v1/accounts")).expect("accounts path tried");
    assert_eq!(accounts.ok, Some(true));
    assert!(accounts.result.as_deref().unwrap().contains("balance") && !accounts.result.as_deref().unwrap().contains("12.5"));
    let tools = f.conns.preview_tools(&saved.id).await.unwrap();
    assert!(tools[0].description.contains("/v1/accounts returns"), "{}", tools[0].description);
    assert!(tools[0].description.contains("under /v1"), "{}", tools[0].description);
}

#[tokio::test]
async fn known_working_requests_that_cover_a_task_are_used_before_any_new_research() {
    let server = MockServer::start().await;
    for (p, body) in [("/v1/me", json!({"item":{"name":"x"}})), ("/v1/accounts", json!({"items":[{"name":"Everyday","balance":{"current":12.5}}]}))] {
        Mock::given(method("GET")).and(path(p)).respond_with(ResponseTemplate::new(200).set_body_json(body)).mount(&server).await;
    }
    Mock::given(path("/responses"))
        .and(|r: &wiremock::Request| String::from_utf8_lossy(&r.body).contains("api_cover"))
        .respond_with(ResponseTemplate::new(200).set_body_json(answer(json!({"covered":true,"paths":["/v1/accounts"]}), vec![])))
        .mount(&server)
        .await;
    let f = Fixture::new(&server).await;
    let saved = f.api(&server, "/v1/me").await;
    f.conns.test(&saved.id).await.unwrap();
    f.core.learn_api_for_task(&saved.id, "Show my account balances").await.unwrap();
    // Second task: the known /v1/accounts request covers it, so no documentation is read and no new paths are tried.
    Mock::given(method("GET")).and(path("/docs")).respond_with(ResponseTemplate::new(200)).expect(0).mount(&server).await;
    Mock::given(method("GET")).and(path("/v1/user")).respond_with(ResponseTemplate::new(200)).expect(0).mount(&server).await;
    let before = f.core.api_examples(&saved.id).unwrap().len();
    let learned = f.core.learn_api_for_task(&saved.id, "Total of my account balances").await.unwrap();
    assert_eq!(learned.len(), before);
    assert!(learned.iter().any(|e| e.url.ends_with("/v1/accounts") && e.ok == Some(true)));
}

#[tokio::test]
async fn a_keyless_api_whose_documentation_cannot_be_read_is_verified_by_answering_with_json() {
    let server = MockServer::start().await;
    Mock::given(path("/docs")).respond_with(ResponseTemplate::new(200).set_body_string("<div id=app></div>")).mount(&server).await;
    Mock::given(method("GET")).and(path("/records")).respond_with(ResponseTemplate::new(200).set_body_json(json!({"current":{"temperature":18.2}}))).mount(&server).await;
    Mock::given(path("/responses"))
        .respond_with(ResponseTemplate::new(200).set_body_json(answer(plan("public_api", None, proposal(&server, "none")), vec![])))
        .mount(&server)
        .await;
    let f = Fixture::new(&server).await;
    let d = f.core.draft_task_no_input("Weather in Melbourne right now").unwrap();
    let p = f.core.access_preview(&f.conns, &d.draft_id).await.unwrap();
    let next = f.core.research_task_access(&f.conns, &d.draft_id, &p.payload_hash).await.unwrap();
    assert_eq!(next.access_plan.unwrap().state, "public_api");
    assert!(next.ready_to_build);
}

#[tokio::test]
async fn a_keyed_api_whose_documentation_cannot_be_read_is_verified_by_asking_for_a_key() {
    let server = MockServer::start().await;
    Mock::given(path("/docs")).respond_with(ResponseTemplate::new(200).set_body_string("<div id=app></div>")).mount(&server).await;
    Mock::given(method("GET")).and(path("/records")).respond_with(ResponseTemplate::new(401)).mount(&server).await;
    Mock::given(path("/responses"))
        .respond_with(ResponseTemplate::new(200).set_body_json(answer(plan("custom_api", None, proposal(&server, "header")), vec![])))
        .mount(&server)
        .await;
    let f = Fixture::new(&server).await;
    let d = f.core.draft_task_no_input("Read my Example records").unwrap();
    let p = f.core.access_preview(&f.conns, &d.draft_id).await.unwrap();
    let next = f.core.research_task_access(&f.conns, &d.draft_id, &p.payload_hash).await.unwrap();
    let report = next.access_plan.unwrap();
    assert_eq!(report.state, "needs_connection", "{}", report.summary);
    assert!(report.api.is_some());
    // The expected refusal does not pause later reads of the service.
    let saved = f.api(&server, "/records").await;
    assert!(!f.conns.test(&saved.id).await.unwrap_err().to_string().contains("paused"));
}

#[tokio::test]
async fn the_model_fills_in_the_output_layout_and_name_and_a_typed_one_is_kept() {
    let server = MockServer::start().await;
    Mock::given(path("/responses"))
        .respond_with(ResponseTemplate::new(200).set_body_json(answer(
            json!({"exampleOutput":"Water: {water_cost}\nPower: {power_cost}\nEach: {each}","name":"“Split the bills.”\nextra"}),
            vec![],
        )))
        .expect(1)
        .mount(&server)
        .await;
    let f = Fixture::new(&server).await;
    let d = f.core.draft_task_no_input("Split the water and power bills four ways").unwrap();
    let filled = f.core.suggest_example_output(&d.draft_id).await.unwrap();
    assert_eq!(filled.example_output.as_deref(), Some("Water: {water_cost}\nPower: {power_cost}\nEach: {each}"));
    // Quotes, trailing punctuation and extra lines are dropped from the name.
    assert_eq!(filled.suggested_name.as_deref(), Some("Split the bills"));
    // A layout and name are already there, so no second model call is made and nothing is replaced.
    let typed = f.core.update_task(&d.draft_id, TaskKind::Program, "bills.txt", Some("Total: {total}".into())).unwrap();
    let kept = f.core.suggest_example_output(&typed.draft_id).await.unwrap();
    assert_eq!(kept.example_output.as_deref(), Some("Total: {total}"));
    assert_eq!(kept.suggested_name.as_deref(), Some("Split the bills"));
}

#[tokio::test]
async fn an_invalid_suggested_layout_is_dropped_but_the_name_is_kept() {
    let server = MockServer::start().await;
    Mock::given(path("/responses"))
        .respond_with(ResponseTemplate::new(200).set_body_json(answer(json!({"exampleOutput":"Water: {water_cost","name":"Water bill"}), vec![])))
        .mount(&server)
        .await;
    let f = Fixture::new(&server).await;
    let d = f.core.draft_task_no_input("Split the water bill").unwrap();
    let next = f.core.suggest_example_output(&d.draft_id).await.unwrap();
    assert_eq!(next.example_output, None);
    assert_eq!(next.suggested_name.as_deref(), Some("Water bill"));
}

#[tokio::test]
async fn editing_an_api_on_the_same_service_keeps_its_id_so_tools_built_on_it_still_read() {
    let server = MockServer::start().await;
    for p in ["/v1/me", "/v1/accounts"] {
        Mock::given(method("GET")).and(path(p)).respond_with(ResponseTemplate::new(200).set_body_json(json!({"items":[]}))).mount(&server).await;
    }
    let f = Fixture::new(&server).await;
    let saved = f.api(&server, "/v1/me").await;
    f.conns.test(&saved.id).await.unwrap();
    let mut config = f.core.api_config(&saved.id).unwrap();
    config.name = "Renamed".into();
    config.url = format!("{}/v1/accounts", server.uri());
    let edited = f.core.update_api_connection(&saved.id, config, Some(Secret::new(b"NEW-SYNTHETIC-KEY".to_vec())), vec![]).await.unwrap();
    assert_eq!(edited.id, saved.id);
    assert_eq!(edited.name, "Renamed");
    f.conns.test(&saved.id).await.unwrap();
    // A tool saved against the old address still reads through this connection.
    assert!(f.core.validate_api_read(&saved.id, &saved.mcp_url, "read", &json!({"path":"/v1/me"})).is_ok());
    let mut elsewhere = f.core.api_config(&saved.id).unwrap();
    elsewhere.url = "https://another.example/v1".into();
    assert!(f.core.update_api_connection(&saved.id, elsewhere, Some(Secret::new(b"K".to_vec())), vec![]).await.is_err());
}
