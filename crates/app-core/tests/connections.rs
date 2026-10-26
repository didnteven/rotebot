//! Connections through the core: OAuth (dynamic registration, PKCE) against an
//! in-process fake server, tokens only in the vault, suggestions, and restart.

use axum::extract::{Form, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use macroscope_app_core::connections::Connections;
use macroscope_app_core::dto::ConnectionState;
use macroscope_app_core::{AppCore, CoreConfig, NativeMuse};
use macroscope_vault::{MemoryStore, SecretRef, SecretStore};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

const ACCESS: &str = "ACCESS-TOKEN-CANARY-7f3a";

#[derive(Clone)]
struct Fake(Arc<String>);

async fn resource(State(f): State<Fake>) -> Json<Value> {
    Json(json!({"resource": format!("{}/mcp", f.0), "authorization_servers": [f.0.as_str()]}))
}
async fn metadata(State(f): State<Fake>) -> Json<Value> {
    Json(
        json!({"issuer": f.0.as_str(), "authorization_endpoint": format!("{}/authorize", f.0),
        "token_endpoint": format!("{}/token", f.0), "registration_endpoint": format!("{}/register", f.0),
        "code_challenge_methods_supported": ["S256"]}),
    )
}
async fn register() -> Json<Value> {
    Json(json!({"client_id": "dyn-client", "token_endpoint_auth_method": "none"}))
}
async fn authorize(Query(q): Query<HashMap<String, String>>) -> Redirect {
    assert_eq!(q["code_challenge_method"], "S256");
    Redirect::to(&format!(
        "{}?code=c1&state={}",
        q["redirect_uri"], q["state"]
    ))
}
async fn token(Form(form): Form<HashMap<String, String>>) -> Json<Value> {
    assert_eq!(form["grant_type"], "authorization_code");
    assert!(form.contains_key("code_verifier"));
    Json(
        json!({"access_token": ACCESS, "refresh_token": "R", "token_type": "Bearer", "expires_in": 3600}),
    )
}
async fn mcp(State(f): State<Fake>, headers: HeaderMap, Json(body): Json<Value>) -> Response {
    if headers.get("authorization").and_then(|v| v.to_str().ok())
        != Some(&format!("Bearer {ACCESS}"))
    {
        let challenge = format!(
            "Bearer resource_metadata=\"{}/.well-known/oauth-protected-resource\"",
            f.0
        );
        return (StatusCode::UNAUTHORIZED, [("www-authenticate", challenge)]).into_response();
    }
    let id = body.get("id").cloned();
    match body["method"].as_str() {
        Some("initialize") => Json(
            json!({"jsonrpc": "2.0", "id": id, "result": {"protocolVersion": "2025-06-18",
            "capabilities": {"tools": {}}, "serverInfo": {"name": "fake", "version": "1"}}}),
        )
        .into_response(),
        Some("tools/list") => Json(json!({"jsonrpc": "2.0", "id": id, "result": {"tools": [
            {"name": "search", "description": "Search", "inputSchema": {"type": "object"}},
            {"name": "draft", "inputSchema": {"type": "object"}}]}}))
        .into_response(),
        _ => StatusCode::ACCEPTED.into_response(),
    }
}

async fn fake_server() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let app = Router::new()
        .route("/.well-known/oauth-protected-resource", get(resource))
        .route("/.well-known/oauth-protected-resource/mcp", get(resource))
        .route("/.well-known/oauth-authorization-server", get(metadata))
        .route("/register", post(register))
        .route("/authorize", get(authorize))
        .route("/token", post(token))
        .route("/mcp", post(mcp))
        .with_state(Fake(Arc::new(base.clone())));
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    base
}

fn core(dir: &Path, vault: Arc<dyn SecretStore>) -> AppCore {
    AppCore::new(CoreConfig {
        data_dir: dir.join("data"),
        vault,
        gateway_base_url: "http://127.0.0.1:9".into(),
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

fn files(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out = vec![];
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(files(&p))
        } else {
            out.push(p)
        }
    }
    out
}

#[tokio::test]
async fn connect_custom_server_end_to_end_with_tokens_only_in_the_vault() {
    let base = fake_server().await;
    let tmp = tempfile::tempdir().unwrap();
    let vault: Arc<dyn SecretStore> = Arc::new(MemoryStore::default());
    let core = core(tmp.path(), vault.clone());
    let conns = Connections::new(core.clone(), vault.clone()).unwrap();
    assert_eq!(conns.list().len(), 9);
    let gmail = conns.list().into_iter().find(|c| c.id == "gmail").unwrap();
    assert_eq!(gmail.registration, "preregistered");
    assert_eq!(gmail.verification, "verified-live");
    assert!(
        conns.begin_connect("gmail").await.is_err(),
        "Gmail needs the user's OAuth client first"
    );

    conns
        .set_url("custom", &format!("{base}/mcp"))
        .await
        .unwrap();
    let start = conns.begin_connect("custom").await.unwrap();
    assert!(conns.allow_authorization_url(&start.authorization_url));
    assert!(matches!(
        conns
            .list()
            .iter()
            .find(|c| c.id == "custom")
            .unwrap()
            .state,
        ConnectionState::Connecting
    ));
    // Play the browser: follow the authorize redirect back to the loopback callback.
    let browser = reqwest::Client::builder().build().unwrap();
    let (resp, done) = tokio::join!(
        browser.get(&start.authorization_url).send(),
        conns.complete_connect(&start.pending_id)
    );
    assert!(resp.unwrap().status().is_success());
    let info = done.unwrap();
    assert!(
        matches!(info.state, ConnectionState::Connected { tool_count: 2, .. }),
        "{:?}",
        info.state
    );
    assert!(vault
        .get(&SecretRef::new("mcp-token:custom").unwrap())
        .unwrap()
        .is_some());

    drop(conns);
    drop(core);
    for f in files(&tmp.path().join("data")) {
        let bytes = std::fs::read(&f).unwrap();
        assert!(
            !bytes.windows(ACCESS.len()).any(|w| w == ACCESS.as_bytes()),
            "token on disk in {f:?}"
        );
    }

    // Restart: status persists; disconnect removes the token.
    let core = self::core(tmp.path(), vault.clone());
    let conns = Connections::new(core.clone(), vault.clone()).unwrap();
    assert!(matches!(
        conns
            .list()
            .iter()
            .find(|c| c.id == "custom")
            .unwrap()
            .state,
        ConnectionState::Connected { .. }
    ));
    let tested = conns.test("custom").await.unwrap();
    assert!(matches!(
        tested.state,
        ConnectionState::Connected { tool_count: 2, .. }
    ));
    let after = conns.disconnect("custom").await.unwrap();
    assert!(matches!(after.state, ConnectionState::NotConnected));
    assert!(vault
        .get(&SecretRef::new("mcp-token:custom").unwrap())
        .unwrap()
        .is_none());
    assert!(core
        .list_activity()
        .unwrap()
        .iter()
        .any(|a| a.title == "Connected Custom MCP server"));
}

#[tokio::test]
async fn client_credentials_live_in_the_vault_and_cancel_stops_a_pending_sign_in() {
    let tmp = tempfile::tempdir().unwrap();
    let vault: Arc<dyn SecretStore> = Arc::new(MemoryStore::default());
    let core = core(tmp.path(), vault.clone());
    let conns = Connections::new(core, vault.clone()).unwrap();
    assert!(conns
        .set_client_credentials("gmail", "has space", None)
        .is_err());
    let info = conns
        .set_client_credentials(
            "gmail",
            "1234.apps.googleusercontent.com",
            Some("CLIENT-SECRET-CANARY"),
        )
        .unwrap();
    assert!(info.has_client_credentials);
    let stored = vault
        .get(&SecretRef::new("mcp-client:gmail").unwrap())
        .unwrap()
        .unwrap();
    assert!(String::from_utf8_lossy(stored.expose()).contains("1234.apps"));

    let base = fake_server().await;
    conns
        .set_url("custom", &format!("{base}/mcp"))
        .await
        .unwrap();
    let start = conns.begin_connect("custom").await.unwrap();
    let pending = {
        let c = conns.clone();
        let id = start.pending_id.clone();
        tokio::spawn(async move { c.complete_connect(&id).await })
    };
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    conns.cancel("custom");
    let result = tokio::time::timeout(std::time::Duration::from_secs(5), pending)
        .await
        .expect("cancel returns promptly")
        .unwrap();
    assert!(result.is_err());
    // The service lock was released: a new sign-in can begin immediately.
    let again = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        conns.begin_connect("custom"),
    )
    .await;
    assert!(again
        .expect("service not stuck behind the cancelled sign-in")
        .is_ok());
}

#[tokio::test]
async fn suggestions_from_task_text_and_recording_respect_dismissal() {
    let tmp = tempfile::tempdir().unwrap();
    let vault: Arc<dyn SecretStore> = Arc::new(MemoryStore::default());
    let core = core(tmp.path(), vault.clone());
    let conns = Connections::new(core.clone(), vault).unwrap();
    let preview = core.load_sample_csv().unwrap();
    core.draft_task(
        &preview.handle.handle_id,
        "Clean it and post a summary to Slack",
    )
    .unwrap();
    core.database()
        .record_detection("gmail", "window_title", 3, 1)
        .unwrap();
    let s = conns.suggestions().unwrap();
    assert_eq!(
        s.iter()
            .map(|x| x.connector_id.as_str())
            .collect::<Vec<_>>(),
        ["gmail", "slack"]
    );
    assert_eq!(
        s[0].message,
        "You used Gmail while recording. Connect Gmail so tools can work with it."
    );
    assert_eq!(
        s[1].message,
        "You mentioned Slack in your task. Connect Slack so tools can work with it."
    );
    assert_eq!(s[1].signal_kind, "task_text");
    conns.dismiss("gmail").unwrap();
    conns.snooze("slack", 30).unwrap();
    assert!(conns.suggestions().unwrap().is_empty());
}
