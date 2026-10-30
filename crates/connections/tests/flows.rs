use axum::{
    body::{Body, Bytes},
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
    Form, Json, Router,
};
use macroscope_connections::{
    catalog::Catalog,
    mcp::{McpClient, McpError},
    oauth::{
        pkce_challenge, AccessToken, ClientCredentials, MemoryTokenStore, OAuthClient, OAuthError,
        TokenStore,
    },
    state::{
        ConnectionError, ConnectionState, ConnectionsService, JsonFileStateStore, MemoryStateStore,
        StateStore,
    },
};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    convert::Infallible,
    sync::{Arc, Mutex},
};
use tokio::{net::TcpListener, task::JoinHandle};
use url::Url;

const ACCESS: &str = "access-secret-canary";
const REFRESH: &str = "refresh-secret-canary";
const CLIENT_SECRET: &str = "client-secret-canary";
#[derive(Clone)]
struct Fake(Arc<FakeInner>);
struct FakeInner {
    origin: String,
    oidc_only: bool,
    expire_first: bool,
    data: Mutex<FakeData>,
}
#[derive(Default)]
struct FakeData {
    registrations: usize,
    refreshes: usize,
    revocations: usize,
    initializations: usize,
    lists: usize,
    calls: usize,
    codes: HashMap<String, HashMap<String, String>>,
    tokens: HashSet<String>,
}
struct Server {
    fake: Fake,
    task: JoinHandle<()>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Server {
    async fn start(expire_first: bool, oidc_only: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let fake = Fake(Arc::new(FakeInner {
            origin: format!("http://{}", listener.local_addr().unwrap()),
            expire_first,
            oidc_only,
            data: Mutex::new(FakeData::default()),
        }));
        let router = Router::new()
            .route("/.well-known/oauth-protected-resource", get(resource))
            .route("/.well-known/oauth-authorization-server", get(metadata))
            .route("/.well-known/openid-configuration", get(oidc))
            .route("/register", post(register))
            .route("/authorize", get(authorize))
            .route("/token", post(token))
            .route("/revoke", post(revoke))
            .route("/mcp/{mode}", post(mcp))
            .with_state(fake.clone());
        let task = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        Self { fake, task }
    }
    fn url(&self, mode: &str) -> String {
        format!("{}/mcp/{mode}", self.fake.0.origin)
    }
    fn service(
        &self,
    ) -> (
        ConnectionsService,
        Arc<MemoryTokenStore>,
        Arc<MemoryStateStore>,
    ) {
        let tokens = Arc::new(MemoryTokenStore::default());
        let state = Arc::new(MemoryStateStore::default());
        (
            ConnectionsService::new(tokens.clone(), state.clone()).unwrap(),
            tokens,
            state,
        )
    }
}
async fn resource(State(fake): State<Fake>) -> Json<Value> {
    Json(json!({"authorization_servers":[fake.0.origin]}))
}
fn metadata_value(fake: &Fake) -> Value {
    json!({"issuer":fake.0.origin, "authorization_endpoint":format!("{}/authorize",fake.0.origin),
        "token_endpoint":format!("{}/token",fake.0.origin), "registration_endpoint":format!("{}/register",fake.0.origin),
        "revocation_endpoint":format!("{}/revoke",fake.0.origin), "code_challenge_methods_supported":["S256"]})
}
async fn metadata(State(fake): State<Fake>) -> Response {
    if fake.0.oidc_only {
        StatusCode::NOT_FOUND.into_response()
    } else {
        Json(metadata_value(&fake)).into_response()
    }
}
async fn oidc(State(fake): State<Fake>) -> Json<Value> {
    Json(metadata_value(&fake))
}
async fn register(State(fake): State<Fake>, Json(body): Json<Value>) -> Json<Value> {
    let redirect = Url::parse(body["redirect_uris"][0].as_str().unwrap()).unwrap();
    assert_eq!(redirect.host_str(), Some("127.0.0.1"));
    assert_eq!(redirect.path(), "/callback");
    assert_eq!(body["token_endpoint_auth_method"], "none");
    assert_eq!(
        body["grant_types"],
        json!(["authorization_code", "refresh_token"])
    );
    fake.0.data.lock().unwrap().registrations += 1;
    Json(json!({"client_id":"dynamic-client", "token_endpoint_auth_method":"none"}))
}
async fn authorize(
    State(fake): State<Fake>,
    Query(query): Query<HashMap<String, String>>,
) -> Redirect {
    assert_eq!(query["response_type"], "code");
    assert_eq!(query["code_challenge_method"], "S256");
    assert_eq!(query["code_challenge"].len(), 43);
    assert!(query["state"].len() >= 43);
    if fake.0.expire_first {
        assert!(!query.contains_key("resource"));
        assert_eq!(query["access_type"], "offline");
        assert_eq!(query["prompt"], "select_account consent");
    } else {
        assert!(query["resource"].starts_with(&fake.0.origin));
    }
    let mut data = fake.0.data.lock().unwrap();
    let code = format!("code-{}", data.codes.len());
    data.codes.insert(code.clone(), query.clone());
    let mut redirect = Url::parse(&query["redirect_uri"]).unwrap();
    redirect
        .query_pairs_mut()
        .append_pair("code", &code)
        .append_pair("state", &query["state"]);
    Redirect::to(redirect.as_str())
}
async fn token(State(fake): State<Fake>, Form(form): Form<HashMap<String, String>>) -> Response {
    let mut data = fake.0.data.lock().unwrap();
    if form.get("client_id").map(String::as_str) == Some("manual-client") {
        assert_eq!(
            form.get("client_secret").map(String::as_str),
            Some(CLIENT_SECRET)
        );
    } else {
        assert_eq!(
            form.get("client_id").map(String::as_str),
            Some("dynamic-client")
        );
    }
    let is_refresh = form["grant_type"] == "refresh_token";
    if is_refresh {
        assert_eq!(form["refresh_token"], REFRESH);
        if fake.0.expire_first {
            assert!(!form.contains_key("resource"));
        } else {
            assert!(form["resource"].starts_with(&fake.0.origin));
        }
        data.refreshes += 1;
    } else {
        assert_eq!(form["grant_type"], "authorization_code");
        let Some(auth) = data.codes.remove(&form["code"]) else {
            return StatusCode::BAD_REQUEST.into_response();
        };
        if pkce_challenge(&form["code_verifier"]) != auth["code_challenge"]
            || form.get("resource") != auth.get("resource")
            || form["redirect_uri"] != auth["redirect_uri"]
            || form["client_id"] != auth["client_id"]
        {
            return (StatusCode::BAD_REQUEST, Json(json!({"error":"invalid_grant","error_description":format!("{ACCESS} {REFRESH} {CLIENT_SECRET}")}))).into_response();
        }
    }
    let access = format!("{ACCESS}-{}", data.refreshes);
    data.tokens.insert(access.clone());
    Json(
        json!({"access_token":access, "refresh_token":REFRESH, "token_type":"Bearer",
        "expires_in":if fake.0.expire_first && !is_refresh {0} else {3600}}),
    )
    .into_response()
}
async fn revoke(State(fake): State<Fake>, Form(form): Form<HashMap<String, String>>) -> StatusCode {
    assert_eq!(form["token"], REFRESH);
    fake.0.data.lock().unwrap().revocations += 1;
    StatusCode::OK
}
async fn mcp(
    State(fake): State<Fake>,
    Path(mode): Path<String>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    let authorized = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .is_some_and(|t| fake.0.data.lock().unwrap().tokens.contains(t));
    if !authorized {
        let challenge = if mode == "fallback" {
            "Bearer realm=\"mcp\"".into()
        } else {
            format!("Bearer realm=\"mcp\", resource_metadata=\"{}/.well-known/oauth-protected-resource\"", fake.0.origin)
        };
        return (StatusCode::UNAUTHORIZED, [("WWW-Authenticate", challenge)]).into_response();
    }
    assert!(headers["accept"]
        .to_str()
        .unwrap()
        .contains("text/event-stream"));
    let method = body["method"].as_str().unwrap();
    let result = if method == "initialize" {
        assert_eq!(body["params"]["protocolVersion"], "2025-06-18");
        assert_eq!(body["params"]["clientInfo"]["name"], "macroscope");
        assert_eq!(body["params"]["capabilities"], json!({}));
        fake.0.data.lock().unwrap().initializations += 1;
        json!({"protocolVersion": if mode == "old" {"2025-03-26"} else if mode == "unsupported" {"2099-01-01"} else {"2025-06-18"},
            "capabilities":{"tools":{}}, "serverInfo":{"name":"fake", "version":"1"}})
    } else {
        assert_eq!(headers["mcp-session-id"], "test-session");
        assert_eq!(
            headers["mcp-protocol-version"],
            if mode == "old" {
                "2025-03-26"
            } else {
                "2025-06-18"
            }
        );
        match method {
            "notifications/initialized" => return StatusCode::ACCEPTED.into_response(),
            "tools/list" => {
                fake.0.data.lock().unwrap().lists += 1;
                if mode == "oversize" {
                    return (
                        [("content-type", "application/json")],
                        "x".repeat(1024 * 1024 + 1),
                    )
                        .into_response();
                }
                if mode == "chunked" {
                    let chunks =
                        (0..1025).map(|_| Ok::<_, Infallible>(Bytes::from(vec![b'x'; 1024])));
                    return (
                        [("content-type", "application/json")],
                        Body::from_stream(tokio_stream(chunks)),
                    )
                        .into_response();
                }
                if body["params"].get("cursor").is_some() {
                    assert_eq!(body["params"]["cursor"], "page-2");
                    json!({"tools":[{"name":"write", "inputSchema":{"type":"object"},"annotations":{"destructiveHint":true}}]})
                } else {
                    json!({"tools":[{"name":"read", "description":format!("\u{001b}{}\n", "é".repeat(1100)),
                        "inputSchema":{"type":"object"},"annotations":{"readOnlyHint":true,"destructiveHint":false}}], "nextCursor":"page-2"})
                }
            }
            "tools/call" => {
                fake.0.data.lock().unwrap().calls += 1;
                json!({"content":[]})
            }
            _ => panic!("unexpected method"),
        }
    };
    let value = json!({"jsonrpc":"2.0", "id":body["id"], "result":result});
    if mode == "sse" {
        // Ignore a notification and a response with another id; multiline CRLF event data.
        let event = format!(": keepalive\r\ndata: {{\"jsonrpc\":\"2.0\",\"method\":\"notifications/progress\"}}\r\n\r\ndata: {{\"id\":999,\"result\":{{}}}}\r\n\r\ndata: {{\"jsonrpc\":\"2.0\",\r\ndata: \"id\":{},\"result\":{}}}\r\n\r\n", body["id"], value["result"]);
        // Keep the stream open after the matching response, to test early return.
        let (sender, receiver) = tokio::sync::mpsc::channel::<Result<Bytes, Infallible>>(8);
        tokio::spawn(async move {
            for chunk in event.as_bytes().chunks(37) {
                if sender
                    .send(Ok(Bytes::copy_from_slice(chunk)))
                    .await
                    .is_err()
                {
                    return;
                }
            }
            let _ = sender.closed().await;
        });
        (
            [
                ("content-type", "text/event-stream"),
                ("Mcp-Session-Id", "test-session"),
            ],
            Body::from_stream(receiver_stream(receiver)),
        )
            .into_response()
    } else {
        ([("Mcp-Session-Id", "test-session")], Json(value)).into_response()
    }
}
// Stream helpers keep the only additional fake-server dependency axum.
fn tokio_stream<I: Iterator<Item = Result<Bytes, Infallible>> + Send + 'static>(
    iter: I,
) -> impl futures_core::Stream<Item = Result<Bytes, Infallible>> + Send {
    struct IterStream<I>(I);
    impl<I: Iterator + Unpin> futures_core::Stream for IterStream<I> {
        type Item = I::Item;
        fn poll_next(
            mut self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Option<Self::Item>> {
            std::task::Poll::Ready(self.0.next())
        }
    }
    // All callers use an Unpin iterator.
    IterStream(Box::new(iter) as Box<dyn Iterator<Item = Result<Bytes, Infallible>> + Send>)
}
fn receiver_stream(
    receiver: tokio::sync::mpsc::Receiver<Result<Bytes, Infallible>>,
) -> impl futures_core::Stream<Item = Result<Bytes, Infallible>> + Send {
    struct ReceiverStream(tokio::sync::mpsc::Receiver<Result<Bytes, Infallible>>);
    impl futures_core::Stream for ReceiverStream {
        type Item = Result<Bytes, Infallible>;
        fn poll_next(
            mut self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Option<Self::Item>> {
            self.0.poll_recv(cx)
        }
    }
    // Keep ownership in a plain wrapper so dropping the response closes the sender.
    ReceiverStream(receiver)
}

async fn browser(url: &str) -> reqwest::Response {
    reqwest::get(url).await.unwrap()
}
async fn connect(
    service: &mut ConnectionsService,
    id: &str,
    credentials: Option<ClientCredentials>,
) {
    let (url, handle) = service.begin_connect(id, credentials).await.unwrap();
    let (response, status) = tokio::join!(browser(url.as_str()), service.complete_connect(handle));
    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers()["content-security-policy"]
        .to_str()
        .unwrap()
        .contains("default-src 'none'"));
    let page = response.text().await.unwrap();
    assert!(page.contains("You can close this tab"));
    assert!(!page.contains("<script"));
    assert!(matches!(
        status.unwrap().state,
        ConnectionState::Connected { tool_count: 2, .. }
    ));
}

#[tokio::test]
async fn full_dynamic_flow_json_and_sse() {
    for mode in ["json", "sse", "old"] {
        let server = Server::start(false, false).await;
        let (mut service, tokens, state) = server.service();
        service.set_url("custom", &server.url(mode)).await.unwrap();
        connect(&mut service, "custom", None).await;
        let bytes = tokens.get("mcp-token:custom").unwrap().unwrap();
        assert!(String::from_utf8(bytes).unwrap().contains(ACCESS));
        let persisted = serde_json::to_string(&state.load().unwrap()).unwrap();
        for secret in [ACCESS, REFRESH, CLIENT_SECRET] {
            assert!(!persisted.contains(secret));
        }
        service.test("custom").await.unwrap();
        let token = OAuthClient::new(tokens.clone())
            .unwrap()
            .access_token("custom", &server.url(mode))
            .await
            .unwrap();
        let mut client = McpClient::connect(&server.url(mode), Some(token))
            .await
            .unwrap();
        assert_eq!(
            client.protocol_version(),
            if mode == "old" {
                "2025-03-26"
            } else {
                "2025-06-18"
            }
        );
        let tools = client.list_tools().await.unwrap();
        assert_eq!(tools.len(), 2);
        let description = tools[0].description.as_ref().unwrap();
        assert_eq!(description.chars().count(), 1000);
        assert!(!description.chars().any(char::is_control));
        assert_eq!(tools[0].annotations.read_only_hint, Some(true));
        assert_eq!(server.fake.0.data.lock().unwrap().calls, 0);
        service.disconnect("custom").await.unwrap();
        assert!(tokens.get("mcp-token:custom").unwrap().is_none());
        assert_eq!(server.fake.0.data.lock().unwrap().revocations, 1);
        assert_eq!(
            service
                .list()
                .iter()
                .find(|s| s.connector_id == "custom")
                .unwrap()
                .state,
            ConnectionState::NotConnected
        );
    }
}
#[tokio::test]
async fn preregistered_client_and_refresh_on_expiry() {
    let server = Server::start(true, false).await;
    let (mut service, tokens, _) = server.service();
    service.set_url("gmail", &server.url("json")).await.unwrap();
    connect(
        &mut service,
        "gmail",
        Some(ClientCredentials::new(
            "manual-client",
            Some(CLIENT_SECRET.into()),
        )),
    )
    .await;
    service.test("gmail").await.unwrap();
    let data = server.fake.0.data.lock().unwrap();
    assert_eq!(data.registrations, 0);
    assert_eq!(data.refreshes, 1);
    let client = String::from_utf8(tokens.get("mcp-token:gmail").unwrap().unwrap()).unwrap();
    assert!(client.contains(CLIENT_SECRET));
}
#[tokio::test]
async fn fallback_resource_discovery_and_openid_configuration() {
    let server = Server::start(false, true).await;
    let (mut service, _, _) = server.service();
    service
        .set_url("custom", &server.url("fallback"))
        .await
        .unwrap();
    connect(&mut service, "custom", None).await;
}
#[tokio::test]
async fn wrong_state_and_pkce_mismatch_are_rejected_without_secret_errors() {
    for (parameter, value) in [
        ("state", "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
        (
            "code_challenge",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ),
    ] {
        let server = Server::start(false, false).await;
        let (mut service, tokens, _) = server.service();
        service
            .set_url("custom", &server.url("json"))
            .await
            .unwrap();
        let (authorization, handle) = service.begin_connect("custom", None).await.unwrap();
        let mut url = Url::parse(authorization.as_str()).unwrap();
        let mut query: HashMap<String, String> = url.query_pairs().into_owned().collect();
        query.insert(parameter.into(), value.into());
        url.query_pairs_mut().clear().extend_pairs(query);
        if parameter == "state" {
            // A redirect carrying a foreign state is ignored (404), and does not
            // consume the callback: the genuine redirect afterwards still completes.
            let completion = tokio::spawn(async move {
                let result = service.complete_connect(handle).await;
                (service, result)
            });
            assert_eq!(browser(url.as_str()).await.status(), StatusCode::NOT_FOUND);
            assert!(tokens.get("mcp-token:custom").unwrap().is_none());
            assert_eq!(
                browser(authorization.as_str()).await.status(),
                StatusCode::OK
            );
            let (_, result) = completion.await.unwrap();
            result.unwrap();
            assert!(tokens.get("mcp-token:custom").unwrap().is_some());
            continue;
        }
        let (_, result) = tokio::join!(browser(url.as_str()), service.complete_connect(handle));
        let error = result.unwrap_err();
        assert!(matches!(error, ConnectionError::OAuth(OAuthError::Token)));
        for secret in [ACCESS, REFRESH, CLIENT_SECRET] {
            assert!(!format!("{error:?} {error}").contains(secret));
        }
        assert!(tokens.get("mcp-token:custom").unwrap().is_none());
    }
}
#[tokio::test]
async fn response_size_caps_and_unsupported_version() {
    for mode in ["oversize", "chunked", "unsupported"] {
        let server = Server::start(false, false).await;
        let (mut service, _, _) = server.service();
        service.set_url("custom", &server.url(mode)).await.unwrap();
        let (url, handle) = service.begin_connect("custom", None).await.unwrap();
        let (_, result) = tokio::join!(browser(url.as_str()), service.complete_connect(handle));
        let error = result.unwrap_err();
        assert!(match mode {
            "unsupported" => matches!(error, ConnectionError::Mcp(McpError::UnsupportedVersion)),
            _ => matches!(error, ConnectionError::Mcp(McpError::ResponseTooLarge)),
        });
    }
}
#[tokio::test]
async fn url_validation_and_401_challenge() {
    let server = Server::start(false, false).await;
    for url in [
        "http://example.com/mcp",
        "ftp://localhost/mcp",
        "https://user:secret@example.com/mcp",
        "http://127.0.0.2/mcp",
        "http://localhost.evil/mcp",
        "https://example.com/#fragment",
    ] {
        assert!(matches!(
            McpClient::connect(url, None).await,
            Err(McpError::InvalidUrl)
        ));
    }
    let error = McpClient::connect(&server.url("json"), None)
        .await
        .unwrap_err();
    assert!(
        matches!(error,McpError::AuthRequired {resource_metadata_url:Some(ref url), ..} if url == &format!("{}/.well-known/oauth-protected-resource",server.fake.0.origin))
    );
    let local = server.url("json").replace("127.0.0.1", "localhost");
    assert!(matches!(
        McpClient::connect(&local, None).await,
        Err(McpError::AuthRequired { .. })
    ));
    let (mut service, _, _) = server.service();
    assert!(service
        .set_url("custom", "http://example.com/mcp")
        .await
        .is_err());
}
#[tokio::test]
async fn url_change_invalidates_pending_and_tokens() {
    let server = Server::start(false, false).await;
    let (mut service, tokens, _) = server.service();
    service
        .set_url("custom", &server.url("json"))
        .await
        .unwrap();
    let (_, handle) = service.begin_connect("custom", None).await.unwrap();
    service.set_url("custom", &server.url("old")).await.unwrap();
    assert!(matches!(
        service.complete_connect(handle).await,
        Err(ConnectionError::StaleHandle)
    ));
    connect(&mut service, "custom", None).await;
    service
        .set_url("custom", &server.url("json"))
        .await
        .unwrap();
    assert!(tokens.get("mcp-token:custom").unwrap().is_none());
}
#[tokio::test]
async fn secrets_are_absent_from_debug_and_transport_errors() {
    let server = Server::start(false, false).await;
    let (mut service, tokens, _) = server.service();
    service
        .set_url("custom", &server.url("json"))
        .await
        .unwrap();
    let (url, handle) = service
        .begin_connect(
            "custom",
            Some(ClientCredentials::new(
                "manual-client",
                Some(CLIENT_SECRET.into()),
            )),
        )
        .await
        .unwrap();
    for secret in [ACCESS, REFRESH, CLIENT_SECRET] {
        assert!(!format!("{:?}", AccessToken::new(secret)).contains(secret));
        assert!(
            !format!("{:?}", ClientCredentials::new(secret, Some(secret.into()))).contains(secret)
        );
        assert!(!format!("{url:?} {handle:?}").contains(secret));
    }
    let state = Url::parse(url.as_str())
        .unwrap()
        .query_pairs()
        .find(|(k, _)| k == "state")
        .unwrap()
        .1
        .to_string();
    assert!(!format!("{url:?} {handle:?}").contains(&state));
    let (_, result) = tokio::join!(browser(url.as_str()), service.complete_connect(handle));
    result.unwrap();
    let token = OAuthClient::new(tokens)
        .unwrap()
        .access_token("custom", &server.url("json"))
        .await
        .unwrap();
    let client = McpClient::connect(&server.url("json"), Some(token))
        .await
        .unwrap();
    assert!(!format!("{client:?}").contains(ACCESS));
    let reflected_url = format!("{}/?token={ACCESS}", server.fake.0.origin);
    let error = McpClient::connect(&reflected_url, Some(AccessToken::new(ACCESS)))
        .await
        .unwrap_err();
    assert!(!format!("{error:?} {error}").contains(ACCESS));
}
#[test]
fn catalog_and_atomic_state_store_round_trip() {
    let catalog = Catalog::embedded();
    assert_eq!(catalog.connectors.len(), 9);
    assert_eq!(catalog.get("gmail").unwrap().auth.scopes, ["https://www.googleapis.com/auth/gmail.modify"]);
    assert!(Catalog::from_json("{\"schema\":\"other\",\"version\":1,\"connectors\":[]}").is_err());
    // Keep all task-created files inside the owned crate and put JSON under data/.
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join(format!("test-state-{}.json", std::process::id()));
    let store = Arc::new(JsonFileStateStore::new(&path));
    let mut service =
        ConnectionsService::new(Arc::new(MemoryTokenStore::default()), store.clone()).unwrap();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime
        .block_on(service.set_url("custom", "https://example.com/mcp"))
        .unwrap();
    assert_eq!(store.load().unwrap(), service.list());
    let mut statuses = store.load().unwrap();
    statuses[0].state = ConnectionState::Connecting;
    store.save(&statuses).unwrap();
    service =
        ConnectionsService::new(Arc::new(MemoryTokenStore::default()), store.clone()).unwrap();
    assert_eq!(service.list()[0].state, ConnectionState::NotConnected);
    let files = std::fs::read_dir(&directory)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert!(!files.iter().any(|f| f.contains(".tmp-")));
    std::fs::remove_file(path).unwrap();
}

#[tokio::test]
async fn callback_expires_after_five_minutes_and_is_single_use() {
    let server = Server::start(false, false).await;
    let (mut service, _, _) = server.service();
    service
        .set_url("custom", &server.url("json"))
        .await
        .unwrap();
    let (url, handle) = service.begin_connect("custom", None).await.unwrap();
    let redirect = Url::parse(url.as_str())
        .unwrap()
        .query_pairs()
        .find(|(k, _)| k == "redirect_uri")
        .unwrap()
        .1
        .to_string();
    tokio::time::pause();
    tokio::time::advance(std::time::Duration::from_secs(301)).await;
    let error = service.complete_connect(handle).await.unwrap_err();
    assert!(matches!(error, ConnectionError::OAuth(OAuthError::Timeout)));
    tokio::time::resume();
    assert!(reqwest::get(&redirect).await.is_err());

    let (url, handle) = service.begin_connect("custom", None).await.unwrap();
    // The listener handles the callback before complete_connect is called.
    let response = browser(url.as_str()).await;
    assert_eq!(response.status(), StatusCode::OK);
    service.complete_connect(handle).await.unwrap();
    assert!(reqwest::get(response.url().clone()).await.is_err());
}
#[tokio::test]
async fn failed_reconnect_preserves_existing_authorization_and_revocation_is_best_effort() {
    let server = Server::start(false, false).await;
    let (mut service, tokens, _) = server.service();
    service
        .set_url("custom", &server.url("json"))
        .await
        .unwrap();
    connect(&mut service, "custom", None).await;
    let previous = tokens.get("mcp-token:custom").unwrap().unwrap();
    let (url, handle) = service.begin_connect("custom", None).await.unwrap();
    let url = Url::parse(url.as_str()).unwrap();
    let query: HashMap<String, String> = url.query_pairs().into_owned().collect();
    // The user presses "Deny": the provider redirects back with the real state and
    // error=access_denied. The flow ends as Denied and the old tokens survive.
    let redirect = Url::parse(&query["redirect_uri"]).unwrap();
    let denied = format!("{}?error=access_denied&state={}", redirect, query["state"]);
    let (response, result) = tokio::join!(reqwest::get(&denied), service.complete_connect(handle));
    assert_eq!(response.unwrap().status(), StatusCode::OK);
    assert!(matches!(
        result.unwrap_err(),
        ConnectionError::OAuth(OAuthError::Denied)
    ));
    assert_eq!(tokens.get("mcp-token:custom").unwrap().unwrap(), previous);
    // An unreachable revocation endpoint must never prevent local deletion.
    server.task.abort();
    service.disconnect("custom").await.unwrap();
    assert!(tokens.get("mcp-token:custom").unwrap().is_none());
}
#[test]
fn pkce_s256_rfc7636_vector() {
    assert_eq!(
        pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
        "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
    );
}
#[tokio::test]
async fn stray_connections_do_not_consume_the_callback() {
    let server = Server::start(false, false).await;
    let (mut service, tokens, _) = server.service();
    service
        .set_url("custom", &server.url("json"))
        .await
        .unwrap();
    let (authorization, handle) = service.begin_connect("custom", None).await.unwrap();
    let query: HashMap<String, String> = Url::parse(authorization.as_str())
        .unwrap()
        .query_pairs()
        .into_owned()
        .collect();
    let redirect = Url::parse(&query["redirect_uri"]).unwrap();
    let completion = tokio::spawn(async move { service.complete_connect(handle).await });
    // A preconnect that never sends a request, then a favicon fetch.
    let addr = format!("127.0.0.1:{}", redirect.port().unwrap());
    drop(tokio::net::TcpStream::connect(&addr).await.unwrap());
    let favicon = format!("http://{addr}/favicon.ico");
    assert_eq!(
        reqwest::get(&favicon).await.unwrap().status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        browser(authorization.as_str()).await.status(),
        StatusCode::OK
    );
    completion.await.unwrap().unwrap();
    assert!(tokens.get("mcp-token:custom").unwrap().is_some());
}
