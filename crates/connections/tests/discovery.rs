use axum::{
    body::{to_bytes, Body},
    extract::{Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json, Router,
};
use macroscope_connections::{
    catalog::{Catalog, Connector, Registration, Verification},
    mcp::{preview_tools, McpError},
    oauth::{ClientCredentials, MemoryTokenStore, OAuthClient, OAuthError},
    state::{ConnectionsService, MemoryStateStore},
};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tokio::net::TcpListener;
use url::Url;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Google,
    Atlassian,
    Challenge,
    Path,
    Origin,
    WrongIssuer,
    BadStatus,
    BadJson,
    Oversize,
}
#[derive(Default)]
struct Observed {
    gets: Vec<String>,
    bearer: Vec<(String, Option<String>)>,
    forms: Vec<HashMap<String, String>>,
}
struct Fake {
    mode: Mode,
    origin: String,
    issuer: String,
    resource: String,
    observed: Mutex<Observed>,
}
struct Server {
    fake: Arc<Fake>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Server {
    async fn start(mode: Mode, invalid_resource: Option<&str>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let issuer = if mode == Mode::Google {
            format!("{origin}/google")
        } else {
            origin.clone()
        };
        let resource = invalid_resource
            .map(|r| {
                if r.starts_with('/') {
                    format!("{origin}{r}")
                } else {
                    r.to_owned()
                }
            })
            .unwrap_or_else(|| origin.clone());
        let fake = Arc::new(Fake {
            mode,
            origin,
            issuer,
            resource,
            observed: Mutex::new(Observed::default()),
        });
        let app = Router::new().fallback(handle).with_state(fake.clone());
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        Self { fake, task }
    }
    fn url(&self) -> String {
        format!("{}/v1/mcp", self.fake.origin)
    }
    fn connector(&self) -> Connector {
        let id = if self.fake.mode == Mode::Google {
            "gmail"
        } else {
            "custom"
        };
        let mut connector = Catalog::embedded().get(id).unwrap().clone();
        connector.mcp_url = self.url();
        // A deliberately different AS path makes the catalog fallback observable.
        if self.fake.mode == Mode::Google {
            connector.auth.authorization_server = Some(self.fake.issuer.clone());
        }
        connector
    }
}
async fn handle(State(fake): State<Arc<Fake>>, request: Request<Body>) -> Response {
    let path = request.uri().path().to_owned();
    if request.method() == "GET" {
        fake.observed.lock().unwrap().gets.push(path.clone());
        if path == "/.well-known/oauth-protected-resource/v1/mcp" {
            match fake.mode {
                Mode::BadStatus => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
                Mode::BadJson => return "garbage".into_response(),
                Mode::Oversize => return "x".repeat(1024 * 1024 + 1).into_response(),
                _ => {}
            }
        }
        let prm = match fake.mode {
            Mode::Challenge | Mode::WrongIssuer => path == "/challenge-metadata",
            Mode::Path => path == "/.well-known/oauth-protected-resource/v1/mcp",
            Mode::Origin | Mode::BadStatus | Mode::BadJson | Mode::Oversize => {
                path == "/.well-known/oauth-protected-resource"
            }
            _ => false,
        };
        if prm {
            return Json(
                json!({"resource":fake.resource, "authorization_servers":[format!("{}/",fake.issuer)],
                "scopes_supported":["metadata-scope"]}),
            )
            .into_response();
        }
        let metadata = if fake.mode == Mode::Google {
            path == "/google/.well-known/openid-configuration"
        } else {
            path == "/.well-known/oauth-authorization-server"
        };
        if metadata {
            let mut value = json!({"issuer":if fake.mode == Mode::WrongIssuer { format!("{}/wrong",fake.issuer) } else {fake.issuer.clone()}, "authorization_endpoint":format!("{}/authorize",fake.origin),
                "token_endpoint":format!("{}/token",fake.origin)});
            if fake.mode != Mode::Google {
                value["registration_endpoint"] = json!(format!("{}/register", fake.origin));
            }
            return Json(value).into_response();
        }
        return (
            StatusCode::NOT_FOUND,
            [("content-type", "text/html")],
            "<html>Not found</html>",
        )
            .into_response();
    }
    if path == "/token" {
        let bytes = to_bytes(request.into_body(), 1024 * 1024).await.unwrap();
        let form: HashMap<String, String> =
            url::form_urlencoded::parse(&bytes).into_owned().collect();
        fake.observed.lock().unwrap().forms.push(form);
        return Json(json!({"access_token":"test-access", "refresh_token":"test-refresh", "expires_in":0, "token_type":"Bearer"})).into_response();
    }
    if path == "/v1/mcp" {
        let auth = request
            .headers()
            .get("authorization")
            .map(|h| h.to_str().unwrap().to_owned());
        let bytes = to_bytes(request.into_body(), 1024 * 1024).await.unwrap();
        let body: Value = serde_json::from_slice(&bytes).unwrap();
        let method = body["method"].as_str().unwrap();
        fake.observed
            .lock()
            .unwrap()
            .bearer
            .push((method.into(), auth.clone()));
        if fake.mode != Mode::Google && auth.is_none() {
            let challenge = if matches!(fake.mode, Mode::Challenge | Mode::WrongIssuer) {
                format!(
                    "Bearer resource_metadata=\"{}/challenge-metadata\", scope=\"read write\"",
                    fake.origin
                )
            } else {
                "Bearer realm=\"mcp\"".into()
            };
            return (StatusCode::UNAUTHORIZED, [("www-authenticate", challenge)]).into_response();
        }
        let result = match method {
            "initialize" => json!({"protocolVersion":"2025-06-18"}),
            "notifications/initialized" => return StatusCode::ACCEPTED.into_response(),
            "tools/list" => json!({"tools":[
                {"name":"read","inputSchema":{},"annotations":{"readOnlyHint":true,"destructiveHint":false}},
                {"name":"write","inputSchema":{},"annotations":{"readOnlyHint":false,"destructiveHint":true}},
                {"name":"unknown","inputSchema":{}}
            ]}),
            _ => panic!("unexpected MCP method"),
        };
        return Json(json!({"jsonrpc":"2.0","id":body["id"],"result":result})).into_response();
    }
    StatusCode::NOT_FOUND.into_response()
}
fn client() -> OAuthClient {
    OAuthClient::new(Arc::new(MemoryTokenStore::default())).unwrap()
}
fn query(url: &str) -> HashMap<String, String> {
    Url::parse(url)
        .unwrap()
        .query_pairs()
        .into_owned()
        .collect()
}

#[tokio::test]
async fn google_catalog_as_html_404_oidc_extra_params_and_open_preview() {
    let server = Server::start(Mode::Google, None).await;
    let connector = server.connector();
    let tokens = Arc::new(MemoryTokenStore::default());
    let mut catalog = Catalog::embedded();
    catalog.connectors = vec![connector.clone()];
    let mut service =
        ConnectionsService::with_catalog(catalog, tokens, Arc::new(MemoryStateStore::default()))
            .unwrap();
    let tools = preview_tools(&server.url()).await.unwrap();
    assert_eq!(tools.len(), 3);
    assert_eq!(tools[0].annotations.read_only_hint, Some(true));
    assert_eq!(tools[0].annotations.destructive_hint, Some(false));
    assert_eq!(tools[1].annotations.read_only_hint, Some(false));
    assert_eq!(tools[1].annotations.destructive_hint, Some(true));
    assert_eq!(tools[2].annotations.read_only_hint, None);
    assert_eq!(tools[2].annotations.destructive_hint, None);
    assert!(server
        .fake
        .observed
        .lock()
        .unwrap()
        .bearer
        .iter()
        .all(|(_, auth)| auth.is_none()));
    let (url, pending) = service
        .begin_connect(
            "gmail",
            Some(ClientCredentials::new(
                "desktop-client",
                Some("test-secret".into()),
            )),
        )
        .await
        .unwrap();
    let auth = query(url.as_str());
    assert!(!auth.contains_key("resource"));
    assert_eq!(auth["scope"], connector.auth.scopes.join(" "));
    assert_eq!(auth["access_type"], "offline");
    assert_eq!(auth["prompt"], "select_account consent");
    let mut callback = Url::parse(&auth["redirect_uri"]).unwrap();
    callback
        .query_pairs_mut()
        .append_pair("code", "test-code")
        .append_pair("state", &auth["state"]);
    assert!(reqwest::get(callback).await.unwrap().status().is_success());
    server.fake.observed.lock().unwrap().bearer.clear();
    service.complete_connect(pending).await.unwrap();
    service.test("gmail").await.unwrap();
    let data = server.fake.observed.lock().unwrap();
    assert_eq!(
        data.gets,
        vec![
            "/.well-known/oauth-protected-resource/v1/mcp",
            "/.well-known/oauth-protected-resource",
            "/.well-known/oauth-authorization-server/google",
            "/google/.well-known/openid-configuration"
        ]
    );
    assert_eq!(data.forms.len(), 2);
    assert_eq!(data.forms[0]["grant_type"], "authorization_code");
    assert_eq!(data.forms[1]["grant_type"], "refresh_token");
    assert!(data.forms.iter().all(|f| !f.contains_key("resource")));
    assert_eq!(data.bearer.len(), 6);
    assert!(data
        .bearer
        .iter()
        .all(|(_, auth)| auth.as_deref() == Some("Bearer test-access")));
}

#[tokio::test]
async fn discovery_precedence_origin_as_and_scope_fallbacks() {
    for mode in [Mode::Atlassian, Mode::Challenge, Mode::Path, Mode::Origin] {
        let server = Server::start(mode, None).await;
        let connector = server.connector();
        let client = client();
        let result = client
            .discover_connector(&connector, &server.url())
            .await
            .unwrap();
        assert_eq!(result.metadata.issuer, server.fake.origin);
        assert!(result.metadata.registration_endpoint.is_some());
        let expected_scopes = match mode {
            Mode::Challenge => vec!["read", "write"],
            Mode::Atlassian => vec![],
            _ => vec!["metadata-scope"],
        };
        assert_eq!(result.scopes, expected_scopes);
        let expected_resource = if mode == Mode::Atlassian {
            server.url()
        } else {
            server.fake.origin.clone()
        };
        assert_eq!(result.resource, expected_resource);
        let mut expected_gets = match mode {
            Mode::Challenge => vec!["/challenge-metadata"],
            Mode::Path => vec!["/.well-known/oauth-protected-resource/v1/mcp"],
            _ => vec![
                "/.well-known/oauth-protected-resource/v1/mcp",
                "/.well-known/oauth-protected-resource",
            ],
        };
        expected_gets.push("/.well-known/oauth-authorization-server");
        assert_eq!(server.fake.observed.lock().unwrap().gets, expected_gets);
        let (url, pending) = client
            .begin(
                &connector,
                &server.url(),
                None,
                Some(ClientCredentials::new("client", None)),
            )
            .await
            .unwrap();
        let auth = query(url.as_str());
        assert_eq!(auth["resource"], expected_resource);
        assert_eq!(
            auth.get("scope").map(String::as_str),
            if expected_scopes.is_empty() {
                None
            } else if mode == Mode::Challenge {
                Some("read write")
            } else {
                Some("metadata-scope")
            }
        );
        let mut callback = Url::parse(&auth["redirect_uri"]).unwrap();
        callback
            .query_pairs_mut()
            .append_pair("code", "test-code")
            .append_pair("state", &auth["state"]);
        reqwest::get(callback).await.unwrap();
        client.complete(pending).await.unwrap();
        client
            .access_token(&connector.id, &server.url())
            .await
            .unwrap();
        assert!(matches!(
            client
                .access_token(&connector.id, &format!("{}/other", server.fake.origin))
                .await,
            Err(OAuthError::NeedsReauth)
        ));
        let data = server.fake.observed.lock().unwrap();
        assert_eq!(data.forms.len(), 2);
        assert!(data
            .forms
            .iter()
            .all(|f| f["resource"] == expected_resource));
    }
}

#[tokio::test]
async fn catalog_scopes_override_challenge_and_prm() {
    let server = Server::start(Mode::Challenge, None).await;
    let mut connector = server.connector();
    connector.auth.scopes = vec!["catalog-scope".into()];
    // PRM must take precedence over a conflicting catalog AS fallback.
    connector.auth.authorization_server = Some("https://unused.invalid".into());
    let result = client()
        .discover_connector(&connector, &server.url())
        .await
        .unwrap();
    assert_eq!(result.scopes, ["catalog-scope"]);
    assert_eq!(result.metadata.issuer, server.fake.issuer);
}

#[tokio::test]
async fn mismatched_prm_resource_is_rejected() {
    for resource in [
        "http://localhost:1",
        "https://evil.example",
        "http://127.0.0.1:1/wrong",
        "/other",
    ] {
        let server = Server::start(Mode::Challenge, Some(resource)).await;
        assert!(matches!(
            client()
                .discover_connector(&server.connector(), &server.url())
                .await,
            Err(OAuthError::Discovery)
        ));
    }
}

#[tokio::test]
async fn fixed_redirect_port_is_used_and_busy_port_fails_clearly() {
    let server = Server::start(Mode::Atlassian, None).await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let mut connector = server.connector();
    connector.auth.fixed_redirect_port = Some(port);
    let client = client();
    let error = client
        .begin(
            &connector,
            &server.url(),
            None,
            Some(ClientCredentials::new("client", None)),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, OAuthError::RedirectPortUnavailable(p) if p == port));
    assert!(error.to_string().contains(&port.to_string()));
    drop(listener);
    let (url, _pending) = client
        .begin(
            &connector,
            &server.url(),
            None,
            Some(ClientCredentials::new("client", None)),
        )
        .await
        .unwrap();
    assert_eq!(
        query(url.as_str())["redirect_uri"],
        format!("http://127.0.0.1:{port}/callback")
    );
}

#[test]
fn catalog_live_facts_and_defaults() {
    let catalog = Catalog::embedded();
    for connector in &catalog.connectors {
        if connector.id == "custom" {
            continue;
        }
        assert_eq!(connector.verification, Verification::VerifiedLive);
        assert_eq!(connector.source, "unauthenticated probe 2026-10-27");
        if connector.auth.registration == Registration::Preregistered {
            assert!(!connector.auth.client_setup.as_ref().unwrap().is_empty());
        }
        if ["slack", "github"].contains(&connector.id.as_str()) {
            assert_eq!(connector.auth.registration, Registration::Preregistered);
            assert_eq!(connector.auth.fixed_redirect_port, Some(53682));
        }
    }
    let auth = &catalog.get("custom").unwrap().auth;
    assert!(auth.send_resource_indicator);
    assert!(auth.fixed_redirect_port.is_none());
    assert!(auth.extra_authorize_params.is_empty());
}

#[tokio::test]
async fn protected_preview_returns_auth_required() {
    let server = Server::start(Mode::Challenge, None).await;
    assert!(matches!(
        preview_tools(&server.url()).await,
        Err(McpError::AuthRequired { .. })
    ));
}

#[tokio::test]
async fn different_issuer_path_is_rejected() {
    let server = Server::start(Mode::WrongIssuer, None).await;
    assert!(matches!(
        client()
            .discover_connector(&server.connector(), &server.url())
            .await,
        Err(OAuthError::Discovery)
    ));
}

#[tokio::test]
async fn discovery_tries_root_prm_after_a_broken_path_prm() {
    for mode in [Mode::BadStatus, Mode::BadJson, Mode::Oversize] {
        let server = Server::start(mode, None).await;
        let result = client()
            .discover_connector(&server.connector(), &server.url())
            .await
            .unwrap();
        assert_eq!(result.resource, server.fake.origin);
        assert_eq!(result.scopes, ["metadata-scope"]);
    }
}

#[tokio::test]
async fn discovery_tries_fallback_after_transport_failure_and_fails_when_all_prm_fail() {
    let server = Server::start(Mode::Origin, None).await;
    let result = client()
        .discover(&server.url(), Some("http://127.0.0.1:9/prm"))
        .await
        .unwrap();
    assert_eq!(result.issuer, server.fake.origin);
    // A hint error must not silently downgrade to catalog/origin discovery when
    // every PRM candidate is absent or broken.
    let server = Server::start(Mode::Atlassian, None).await;
    assert!(client()
        .discover(&server.url(), Some("http://127.0.0.1:9/prm"))
        .await
        .is_err());
}
