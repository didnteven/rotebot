//! OAuth secrets stay in the host process and TokenStore. Debug/errors are redacted.
use crate::{
    catalog::{Connector, Registration},
    http,
    mcp::{McpClient, McpError},
    now_ms,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    time::Instant,
};
use url::Url;
use zeroize::{Zeroize, Zeroizing};

#[derive(Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AccessToken(String);
impl AccessToken {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}
impl Drop for AccessToken {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}
impl std::fmt::Debug for AccessToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("[REDACTED]")
    }
}

/// Vault adapters must return content-free StoreError values, never backend messages.
pub trait TokenStore: Send + Sync {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StoreError>;
    fn set(&self, key: &str, bytes: &[u8]) -> Result<(), StoreError>;
    fn delete(&self, key: &str) -> Result<(), StoreError>;
}
#[derive(Debug, thiserror::Error)]
#[error("secret store unavailable")]
pub struct StoreError;
#[derive(Default)]
pub struct MemoryTokenStore(Mutex<HashMap<String, Vec<u8>>>);
impl TokenStore for MemoryTokenStore {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StoreError> {
        Ok(self.0.lock().map_err(|_| StoreError)?.get(key).cloned())
    }
    fn set(&self, key: &str, bytes: &[u8]) -> Result<(), StoreError> {
        if let Some(mut old) = self
            .0
            .lock()
            .map_err(|_| StoreError)?
            .insert(key.into(), bytes.to_vec())
        {
            old.zeroize();
        }
        Ok(())
    }
    fn delete(&self, key: &str) -> Result<(), StoreError> {
        if let Some(mut old) = self.0.lock().map_err(|_| StoreError)?.remove(key) {
            old.zeroize();
        }
        Ok(())
    }
}
impl Drop for MemoryTokenStore {
    fn drop(&mut self) {
        if let Ok(store) = self.0.get_mut() {
            for bytes in store.values_mut() {
                bytes.zeroize();
            }
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ClientCredentials {
    pub client_id: String,
    pub client_secret: Option<AccessToken>,
    #[serde(default)]
    pub token_endpoint_auth_method: Option<String>,
}
impl ClientCredentials {
    pub fn new(client_id: impl Into<String>, secret: Option<String>) -> Self {
        Self {
            client_id: client_id.into(),
            client_secret: secret.map(AccessToken::new),
            token_endpoint_auth_method: None,
        }
    }
}
impl std::fmt::Debug for ClientCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ClientCredentials([REDACTED])")
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct ServerMetadata {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub registration_endpoint: Option<String>,
    pub revocation_endpoint: Option<String>,
    pub code_challenge_methods_supported: Option<Vec<String>>,
}
impl ServerMetadata {
    fn validate(&self) -> Result<(), OAuthError> {
        for endpoint in [
            &self.issuer,
            &self.authorization_endpoint,
            &self.token_endpoint,
        ]
        .into_iter()
        .chain(self.registration_endpoint.as_ref())
        .chain(self.revocation_endpoint.as_ref())
        {
            http::validate_url(endpoint)?;
        }
        if self
            .code_challenge_methods_supported
            .as_ref()
            .is_some_and(|m| !m.iter().any(|m| m == "S256"))
        {
            return Err(OAuthError::Discovery);
        }
        Ok(())
    }
}
/// Validated discovery data. Resource may be an origin prefix of the configured MCP URL.
pub struct Discovery {
    pub metadata: ServerMetadata,
    pub resource: String,
    pub scopes: Vec<String>,
}
#[derive(Debug, thiserror::Error)]
pub enum OAuthError {
    #[error(transparent)]
    Mcp(#[from] McpError),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("invalid OAuth discovery metadata")]
    Discovery,
    #[error("supply OAuth client credentials for this connector")]
    ClientRequired,
    #[error("unsupported OAuth client authentication method")]
    ClientAuthentication,
    #[error("OAuth callback port {0} is unavailable; close the application using it and retry")]
    RedirectPortUnavailable(u16),
    #[error("OAuth callback state mismatch")]
    StateMismatch,
    #[error("invalid OAuth callback")]
    Callback,
    #[error("sign-in was cancelled or denied in the browser")]
    Denied,
    #[error("OAuth callback timed out")]
    Timeout,
    #[error("OAuth token response invalid or authorization expired")]
    Token,
    #[error("no stored authorization; reconnect required")]
    NeedsReauth,
    #[error("secure random generation failed")]
    Random,
}

/// Returned to the caller to open in the system browser; omitted from Debug.
pub struct AuthorizationUrl(Url);
impl AuthorizationUrl {
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}
impl std::fmt::Debug for AuthorizationUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AuthorizationUrl([REDACTED])")
    }
}
/// Owned, single-use handle. Dropping it closes the bound loopback listener.
pub struct PendingAuthorization {
    pub(crate) connector_id: String,
    pub(crate) resource: String,
    resource_indicator: Option<String>,
    callback: CallbackTask,
    redirect_uri: String,
    verifier: AccessToken,
    credentials: ClientCredentials,
    metadata: ServerMetadata,
}
struct CallbackTask(tokio::task::JoinHandle<Result<AccessToken, OAuthError>>);
impl Drop for CallbackTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}
impl std::fmt::Debug for PendingAuthorization {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PendingAuthorization([REDACTED])")
    }
}
#[derive(Clone, Serialize, Deserialize)]
struct StoredToken {
    access_token: AccessToken,
    refresh_token: Option<AccessToken>,
    expires_at_ms: Option<u64>,
    resource: String,
    #[serde(default)]
    oauth_resource: Option<String>,
    #[serde(default = "crate::catalog::default_true")]
    send_resource_indicator: bool,
    metadata: ServerMetadata,
    credentials: ClientCredentials,
}
#[derive(Deserialize)]
struct TokenResponse {
    access_token: AccessToken,
    refresh_token: Option<AccessToken>,
    expires_in: Option<u64>,
    token_type: String,
}
#[derive(Clone)]
pub struct OAuthClient {
    store: Arc<dyn TokenStore>,
    http: reqwest::Client,
}
impl OAuthClient {
    pub fn new(store: Arc<dyn TokenStore>) -> Result<Self, OAuthError> {
        Ok(Self {
            store,
            http: http::client()?,
        })
    }
    /// Discover using a supplied PRM hint, with the MCP origin as the final fallback.
    pub async fn discover(
        &self,
        resource: &str,
        hint: Option<&str>,
    ) -> Result<ServerMetadata, OAuthError> {
        Ok(self.resolve(resource, hint, None).await?.metadata)
    }
    /// Unauthenticated initialize and discovery only. Does not register or authorize.
    pub async fn discover_connector(
        &self,
        connector: &Connector,
        resource: &str,
    ) -> Result<Discovery, OAuthError> {
        self.discover_for(connector, resource, None).await
    }
    async fn discover_for(
        &self,
        connector: &Connector,
        resource: &str,
        hint: Option<&str>,
    ) -> Result<Discovery, OAuthError> {
        let (challenge_hint, scope) = McpClient::probe_auth(resource).await?;
        let mut discovered = self
            .resolve(
                resource,
                hint.or(challenge_hint.as_deref()),
                connector.auth.authorization_server.as_deref(),
            )
            .await?;
        discovered.scopes = if !connector.auth.scopes.is_empty() {
            connector.auth.scopes.clone()
        } else if let Some(scope) = scope {
            scope.split_whitespace().map(str::to_owned).collect()
        } else {
            discovered.scopes
        };
        Ok(discovered)
    }
    async fn optional_metadata<T: serde::de::DeserializeOwned>(
        &self,
        url: Url,
    ) -> Result<Option<T>, OAuthError> {
        let response = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|_| McpError::Transport)?;
        // Check status before parsing: providers often return an HTML 404 page.
        if matches!(response.status().as_u16(), 404 | 405) {
            return Ok(None);
        }
        Ok(Some(http::json(response).await?))
    }
    async fn resolve(
        &self,
        resource: &str,
        hint: Option<&str>,
        catalog_issuer: Option<&str>,
    ) -> Result<Discovery, OAuthError> {
        let resource_url = http::validate_url(resource)?;
        let origin = resource_url.origin().ascii_serialization();
        let mut urls = Vec::new();
        if let Some(hint) = hint {
            urls.push(http::validate_url(hint)?);
        }
        for path in [
            format!(
                "/.well-known/oauth-protected-resource{}",
                resource_url.path()
            ),
            "/.well-known/oauth-protected-resource".into(),
        ] {
            let url = resource_url
                .join(&path)
                .map_err(|_| OAuthError::Discovery)?;
            if !urls.contains(&url) {
                urls.push(url);
            }
        }
        #[derive(Deserialize)]
        struct Resource {
            authorization_servers: Vec<String>,
            resource: Option<String>,
            #[serde(default)]
            scopes_supported: Vec<String>,
        }
        let mut protected = None;
        let mut prm_error = None;
        for url in urls {
            match self.optional_metadata::<Resource>(url).await {
                Ok(Some(metadata)) => {
                    protected = Some(metadata);
                    break;
                }
                Ok(None) => {}
                Err(error) => prm_error = Some(error),
            }
        }
        if protected.is_none() {
            if let Some(error) = prm_error {
                return Err(error);
            }
        }
        let mut oauth_resource = resource.to_owned();
        if let Some(value) = protected.as_ref().and_then(|p| p.resource.as_ref()) {
            let url = http::validate_url(value)?;
            if url.origin() != resource_url.origin() || !resource.starts_with(value) {
                return Err(OAuthError::Discovery);
            }
            oauth_resource = value.clone();
        }
        let issuer = match protected.as_ref() {
            Some(p) => p
                .authorization_servers
                .first()
                .ok_or(OAuthError::Discovery)?
                .as_str(),
            None => catalog_issuer.unwrap_or(&origin),
        };
        let issuer_url = http::validate_url(issuer)?;
        if issuer_url.query().is_some() {
            return Err(OAuthError::Discovery);
        }
        let path = issuer_url.path().trim_end_matches('/');
        let oauth_url = issuer_url
            .join(&format!("/.well-known/oauth-authorization-server{path}"))
            .map_err(|_| OAuthError::Discovery)?;
        let metadata = match self.optional_metadata::<ServerMetadata>(oauth_url).await? {
            Some(metadata) => metadata,
            None => {
                let mut oidc_url = issuer_url.clone();
                oidc_url.set_path(&format!("{path}/.well-known/openid-configuration"));
                self.optional_metadata(oidc_url)
                    .await?
                    .ok_or(OAuthError::Discovery)?
            }
        };
        // URL parsing normalizes the optional slash on an origin issuer. Google's
        // path PRM advertises https://accounts.google.com/ while AS metadata omits it.
        // Issuer paths (including their trailing slash) must still match exactly.
        if http::validate_url(&metadata.issuer)? != issuer_url {
            return Err(OAuthError::Discovery);
        }
        metadata.validate()?;
        Ok(Discovery {
            metadata,
            resource: oauth_resource,
            scopes: protected.map(|p| p.scopes_supported).unwrap_or_default(),
        })
    }
    pub async fn begin(
        &self,
        connector: &Connector,
        resource: &str,
        hint: Option<&str>,
        supplied: Option<ClientCredentials>,
    ) -> Result<(AuthorizationUrl, PendingAuthorization), OAuthError> {
        let discovered = self.discover_for(connector, resource, hint).await?;
        let metadata = discovered.metadata;
        let resource_indicator = connector
            .auth
            .send_resource_indicator
            .then_some(discovered.resource);
        let port = connector.auth.fixed_redirect_port.unwrap_or(0);
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port))
            .await
            .map_err(|_| {
                if port == 0 {
                    OAuthError::Callback
                } else {
                    OAuthError::RedirectPortUnavailable(port)
                }
            })?;
        let redirect_uri = format!(
            "http://127.0.0.1:{}/callback",
            listener
                .local_addr()
                .map_err(|_| OAuthError::Callback)?
                .port()
        );
        let credentials = if let Some(credentials) = supplied {
            credentials
        } else if connector.auth.registration == Registration::Dynamic
            && metadata.registration_endpoint.is_some()
        {
            let endpoint = metadata
                .registration_endpoint
                .as_ref()
                .ok_or(OAuthError::Discovery)?;
            http::json(self.http.post(endpoint).json(&serde_json::json!({
                "client_name":"Rotebot", "redirect_uris":[redirect_uri],
                "grant_types":["authorization_code", "refresh_token"], "response_types":["code"],
                "token_endpoint_auth_method":"none"
            })).send().await.map_err(|_| McpError::Transport)?).await?
        } else {
            self.load::<StoredToken>(&token_key(&connector.id))?
                .filter(|token| token.resource == resource)
                .map(|token| token.credentials)
                .ok_or(OAuthError::ClientRequired)?
        };
        if credentials.client_id.is_empty() {
            return Err(OAuthError::ClientRequired);
        }
        validate_client_auth(&credentials)?;
        let verifier = random_secret()?;
        let state = random_secret()?;
        let challenge = pkce_challenge(verifier.expose());
        let mut url = http::validate_url(&metadata.authorization_endpoint)?;
        url.query_pairs_mut()
            .clear()
            .append_pair("response_type", "code")
            .append_pair("client_id", &credentials.client_id)
            .append_pair("redirect_uri", &redirect_uri)
            .append_pair("state", state.expose())
            .append_pair("code_challenge", &challenge)
            .append_pair("code_challenge_method", "S256");
        if let Some(resource) = &resource_indicator {
            url.query_pairs_mut().append_pair("resource", resource);
        }
        if !discovered.scopes.is_empty() {
            url.query_pairs_mut()
                .append_pair("scope", &discovered.scopes.join(" "));
        }
        url.query_pairs_mut()
            .extend_pairs(&connector.auth.extra_authorize_params);
        let deadline = Instant::now() + Duration::from_secs(300);
        let callback = CallbackTask(tokio::spawn(async move {
            tokio::time::timeout_at(deadline, callback(listener, state))
                .await
                .map_err(|_| OAuthError::Timeout)?
        }));
        Ok((
            AuthorizationUrl(url),
            PendingAuthorization {
                connector_id: connector.id.clone(),
                resource: resource.into(),
                resource_indicator,
                callback,
                redirect_uri,
                verifier,
                credentials,
                metadata,
            },
        ))
    }
    pub async fn complete(
        &self,
        mut pending: PendingAuthorization,
    ) -> Result<AccessToken, OAuthError> {
        let code = (&mut pending.callback.0)
            .await
            .map_err(|_| OAuthError::Callback)??;
        let mut form = vec![
            ("grant_type", "authorization_code"),
            ("code", code.expose()),
            ("redirect_uri", pending.redirect_uri.as_str()),
            ("code_verifier", pending.verifier.expose()),
        ];
        if let Some(resource) = &pending.resource_indicator {
            form.push(("resource", resource.as_str()));
        }
        let response = self
            .token_request(&pending.metadata.token_endpoint, &pending.credentials, form)
            .await?;
        let token = stored(
            response,
            pending.resource,
            pending.resource_indicator,
            pending.metadata,
            pending.credentials,
            None,
        )?;
        self.save(&token_key(&pending.connector_id), &token)?;
        Ok(token.access_token.clone())
    }
    pub async fn access_token(
        &self,
        connector_id: &str,
        resource: &str,
    ) -> Result<AccessToken, OAuthError> {
        let token: StoredToken = self
            .load(&token_key(connector_id))?
            .ok_or(OAuthError::NeedsReauth)?;
        if token.resource != resource {
            return Err(OAuthError::NeedsReauth);
        }
        if token
            .expires_at_ms
            .is_none_or(|expiry| expiry > now_ms().saturating_add(30_000))
        {
            return Ok(token.access_token);
        }
        let refresh = token.refresh_token.clone().ok_or(OAuthError::NeedsReauth)?;
        token.metadata.validate()?;
        let resource_indicator = token.send_resource_indicator.then(|| {
            token
                .oauth_resource
                .clone()
                .unwrap_or_else(|| resource.into())
        });
        let mut form = vec![
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh.expose()),
        ];
        if let Some(resource) = &resource_indicator {
            form.push(("resource", resource.as_str()));
        }
        let response = self
            .token_request(&token.metadata.token_endpoint, &token.credentials, form)
            .await?;
        let token = stored(
            response,
            resource.into(),
            resource_indicator,
            token.metadata,
            token.credentials,
            Some(refresh),
        )?;
        self.save(&token_key(connector_id), &token)?;
        Ok(token.access_token)
    }
    pub async fn disconnect(&self, connector_id: &str) -> Result<(), OAuthError> {
        // Delete locally even if loading corrupt records or remote revocation fails.
        let token = self.load::<StoredToken>(&token_key(connector_id));
        self.store.delete(&token_key(connector_id))?;
        if let Ok(Some(token)) = token {
            if let Some(endpoint) = token.metadata.revocation_endpoint {
                if http::validate_url(&endpoint).is_ok() {
                    let secret = token.refresh_token.as_ref().unwrap_or(&token.access_token);
                    let _ = self
                        .authenticated_post(
                            &endpoint,
                            &token.credentials,
                            vec![("token", secret.expose())],
                        )
                        .send()
                        .await;
                }
            }
        }
        Ok(())
    }
    /// Secrets that are not OAuth tokens (an email app password) use the same private store.
    pub(crate) fn save_secret<T: Serialize>(&self, key: &str, value: &T) -> Result<(), OAuthError> {
        self.save(key, value)
    }
    pub(crate) fn load_secret<T: serde::de::DeserializeOwned>(
        &self,
        key: &str,
    ) -> Result<Option<T>, OAuthError> {
        self.load(key)
    }
    pub(crate) fn delete_secret(&self, key: &str) -> Result<(), OAuthError> {
        self.store.delete(key).map_err(Into::into)
    }
    fn load<T: serde::de::DeserializeOwned>(&self, key: &str) -> Result<Option<T>, OAuthError> {
        self.store
            .get(key)?
            .map(|mut bytes| {
                let result = serde_json::from_slice(&bytes).map_err(|_| OAuthError::Token);
                bytes.zeroize();
                result
            })
            .transpose()
    }
    fn save<T: Serialize>(&self, key: &str, value: &T) -> Result<(), OAuthError> {
        let mut bytes = serde_json::to_vec(value).map_err(|_| OAuthError::Token)?;
        let result = self.store.set(key, &bytes);
        bytes.zeroize();
        result.map_err(Into::into)
    }
    fn authenticated_post<'a>(
        &self,
        endpoint: &str,
        credentials: &'a ClientCredentials,
        mut form: Vec<(&str, &'a str)>,
    ) -> reqwest::RequestBuilder {
        form.push(("client_id", credentials.client_id.as_str()));
        let mut request = self.http.post(endpoint);
        if let Some(secret) = &credentials.client_secret {
            if credentials.token_endpoint_auth_method.as_deref() == Some("client_secret_basic") {
                // OAuth Basic uses form-encoded id and secret before Base64 encoding.
                let encode = |s: &str| {
                    let mut encoded = Zeroizing::new(String::with_capacity(3 * s.len()));
                    for part in url::form_urlencoded::byte_serialize(s.as_bytes()) {
                        encoded.push_str(part);
                    }
                    encoded
                };
                let id = encode(&credentials.client_id);
                let secret = encode(secret.expose());
                request = request.basic_auth(id.as_str(), Some(secret.as_str()));
            } else {
                form.push(("client_secret", secret.expose()));
            }
        }
        // Borrow the AccessToken values throughout; never clone their inner
        // Strings. Reserve the worst-case percent-encoded size so serialization
        // cannot leave secret-bearing allocations behind through reallocation.
        let capacity = form.iter().map(|(k, v)| 3 * (k.len() + v.len()) + 2).sum();
        let mut encoded = url::form_urlencoded::Serializer::new(String::with_capacity(capacity));
        encoded.extend_pairs(form);
        let body = SecretBody(Zeroizing::new(encoded.finish().into_bytes()));
        request
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .body(bytes::Bytes::from_owner(body))
    }
    async fn token_request(
        &self,
        endpoint: &str,
        credentials: &ClientCredentials,
        form: Vec<(&str, &str)>,
    ) -> Result<TokenResponse, OAuthError> {
        http::validate_url(endpoint)?;
        validate_client_auth(credentials)?;
        let response = self
            .authenticated_post(endpoint, credentials, form)
            .send()
            .await
            .map_err(|_| McpError::Transport)?;
        // Do not expose even a reflected token from an OAuth error response.
        if !response.status().is_success() {
            return Err(OAuthError::Token);
        }
        Ok(http::json(response).await?)
    }
}
// Bytes retains this owner across transport clones. Zeroize on success, error,
// or task cancellation, not merely when request construction returns. TLS/socket
// buffers and reqwest's Basic header storage remain owned by those libraries.
struct SecretBody(Zeroizing<Vec<u8>>);
impl AsRef<[u8]> for SecretBody {
    fn as_ref(&self) -> &[u8] {
        self.0.as_slice()
    }
}
impl Drop for SecretBody {
    fn drop(&mut self) {
        self.0.as_mut_slice().zeroize();
        #[cfg(test)]
        {
            assert!(self.0.iter().all(|b| *b == 0));
            REQUEST_BODY_WIPES.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
    }
}

fn validate_client_auth(credentials: &ClientCredentials) -> Result<(), OAuthError> {
    match credentials.token_endpoint_auth_method.as_deref() {
        None | Some("none") => Ok(()),
        Some("client_secret_basic" | "client_secret_post")
            if credentials.client_secret.is_some() =>
        {
            Ok(())
        }
        _ => Err(OAuthError::ClientAuthentication),
    }
}
fn stored(
    response: TokenResponse,
    resource: String,
    resource_indicator: Option<String>,
    metadata: ServerMetadata,
    credentials: ClientCredentials,
    old_refresh: Option<AccessToken>,
) -> Result<StoredToken, OAuthError> {
    if !response.token_type.eq_ignore_ascii_case("bearer")
        || response.access_token.0.is_empty()
        || HeaderToken::invalid(response.access_token.expose())
    {
        return Err(OAuthError::Token);
    }
    Ok(StoredToken {
        access_token: response.access_token,
        refresh_token: response.refresh_token.or(old_refresh),
        expires_at_ms: response
            .expires_in
            .map(|s| now_ms().saturating_add(s.saturating_mul(1000))),
        resource,
        send_resource_indicator: resource_indicator.is_some(),
        oauth_resource: resource_indicator,
        metadata,
        credentials,
    })
}
struct HeaderToken;
impl HeaderToken {
    fn invalid(value: &str) -> bool {
        value.bytes().any(|b| !(0x21..=0x7e).contains(&b))
    }
}
fn token_key(id: &str) -> String {
    format!("mcp-token:{id}")
}
fn random_secret() -> Result<AccessToken, OAuthError> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|_| OAuthError::Random)?;
    let result = AccessToken(URL_SAFE_NO_PAD.encode(bytes));
    bytes.zeroize();
    Ok(result)
}
pub fn pkce_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// Waits for the browser redirect. Accepts connections until a request carries the
/// expected state (the caller's deadline bounds the wait), so a stray local connection,
/// a browser preconnect, or a favicon request cannot consume the one real callback.
async fn callback(listener: TcpListener, state: AccessToken) -> Result<AccessToken, OAuthError> {
    // Bound both concurrent socket state and per-client deadlines. When saturated,
    // evict stalled clients so a fresh browser redirect can still be accepted.
    let mut clients = tokio::task::JoinSet::new();
    loop {
        tokio::select! {
            biased;
            Some(result) = clients.join_next(), if !clients.is_empty() => {
                if let Ok(Some(result)) = result { return result; }
            }
            accepted = listener.accept() => {
                let (stream, _) = accepted.map_err(|_| OAuthError::Callback)?;
                if clients.len() >= 32 {
                    clients.abort_all();
                    while clients.join_next().await.is_some() {}
                }
                let state = state.clone();
                clients.spawn(callback_client(stream, state));
            }
        }
    }
}

async fn callback_client(
    mut stream: tokio::net::TcpStream,
    state: AccessToken,
) -> Option<Result<AccessToken, OAuthError>> {
    let mut request = Zeroizing::new(Vec::new());
    let read = tokio::time::timeout(Duration::from_secs(5), async {
        let mut buffer = Zeroizing::new([0u8; 1024]);
        while !request.windows(4).any(|w| w == b"\r\n\r\n") {
            let n = stream.read(&mut buffer[..]).await.ok()?;
            if n == 0 || request.len() + n > 8192 {
                return None;
            }
            request.extend_from_slice(&buffer[..n]);
        }
        Some(())
    })
    .await;
    if !matches!(read, Ok(Some(()))) {
        request.zeroize();
        return None;
    }
    let result = parse_callback(&request, &state);
    request.zeroize();
    // Entire page is fixed: no query reflection, scripts, links, external resources, or IPC.
    let (status, body) = match &result {
        Ok(_) => ("200 OK", "You can close this tab."),
        Err(OAuthError::Denied) => ("200 OK", "Sign-in was cancelled. You can close this tab."),
        Err(_) => ("404 Not Found", "Not found."),
    };
    let page = format!(
        "<!doctype html><html><head><title>Rotebot</title></head><body>{body}</body></html>"
    );
    let response = format!("HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nContent-Security-Policy: default-src 'none'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'\r\nReferrer-Policy: no-referrer\r\nX-Content-Type-Options: nosniff\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{page}", page.len());
    let _ = tokio::time::timeout(Duration::from_secs(1), async {
        let _ = stream.write_all(response.as_bytes()).await;
        let _ = stream.shutdown().await;
    })
    .await;
    match result {
        Err(OAuthError::Callback | OAuthError::StateMismatch) => None,
        other => Some(other),
    }
}
fn parse_callback(request: &[u8], expected_state: &AccessToken) -> Result<AccessToken, OAuthError> {
    let request = std::str::from_utf8(request).map_err(|_| OAuthError::Callback)?;
    let mut parts = request
        .lines()
        .next()
        .ok_or(OAuthError::Callback)?
        .split_whitespace();
    if parts.next() != Some("GET") {
        return Err(OAuthError::Callback);
    }
    let target = parts.next().ok_or(OAuthError::Callback)?;
    if !target.starts_with("/callback?") {
        return Err(OAuthError::Callback);
    }
    let query: Vec<_> = url::form_urlencoded::parse(&target.as_bytes()["/callback?".len()..])
        .map(|(key, value)| (key, AccessToken::new(value.into_owned())))
        .collect();
    let single = |key: &str| -> Result<AccessToken, OAuthError> {
        let mut values = query.iter().filter(|(k, _)| k == key);
        let value = values.next().ok_or(OAuthError::Callback)?.1.clone();
        if values.next().is_some() {
            return Err(OAuthError::Callback);
        }
        Ok(value)
    };
    let state = single("state")?;
    let expected = expected_state.expose().as_bytes();
    let difference = state
        .expose()
        .as_bytes()
        .iter()
        .zip(expected)
        .fold(0u8, |acc, (a, b)| acc | (a ^ b));
    if state.expose().len() != expected.len() || difference != 0 {
        return Err(OAuthError::StateMismatch);
    }
    if query.iter().any(|(k, _)| k == "error") {
        return Err(OAuthError::Denied);
    }
    let code = single("code")?;
    if code.expose().is_empty() {
        return Err(OAuthError::Callback);
    }
    Ok(code)
}

#[cfg(test)]
static REQUEST_BODY_WIPES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

#[cfg(test)]
mod callback_hardening_tests {
    use super::*;
    use tokio::net::TcpStream;

    #[tokio::test]
    async fn callback_accepts_the_real_redirect_while_many_clients_stall() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let task = tokio::spawn(callback(listener, AccessToken::new("expected")));
        let mut slow = Vec::new();
        for _ in 0..40 {
            let mut s = TcpStream::connect(addr).await.unwrap();
            s.write_all(b"GET /callback?").await.unwrap();
            slow.push(s);
        }
        let mut real = TcpStream::connect(addr).await.unwrap();
        real.write_all(b"GET /callback?state=expected&code=ok HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .await
            .unwrap();
        let result = tokio::time::timeout(Duration::from_secs(1), task).await;
        assert!(result.is_ok(), "slow clients blocked the browser callback");
        assert_eq!(result.unwrap().unwrap().unwrap().expose(), "ok");
    }

    #[tokio::test]
    async fn callback_ignores_stray_requests_and_returns_matching_denial() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let task = tokio::spawn(callback(listener, AccessToken::new("expected")));
        for query in ["/favicon.ico", "/callback?state=wrong&error=denied"] {
            let mut s = TcpStream::connect(addr).await.unwrap();
            s.write_all(format!("GET {query} HTTP/1.1\r\n\r\n").as_bytes())
                .await
                .unwrap();
            let mut response = Vec::new();
            s.read_to_end(&mut response).await.unwrap();
            assert!(response.starts_with(b"HTTP/1.1 404"));
        }
        let mut s = TcpStream::connect(addr).await.unwrap();
        s.write_all(b"GET /callback?state=expected&error=access_denied HTTP/1.1\r\n\r\n")
            .await
            .unwrap();
        assert!(matches!(task.await.unwrap(), Err(OAuthError::Denied)));
    }
}

#[cfg(test)]
mod transient_secret_tests {
    use super::*;
    use std::sync::atomic::Ordering;

    #[tokio::test]
    async fn encoded_oauth_secrets_are_wiped_on_drop_error_and_cancellation() {
        let client = OAuthClient::new(Arc::new(MemoryTokenStore::default())).unwrap();
        let credentials = ClientCredentials::new("client", Some("client-secret-canary".into()));
        let before = REQUEST_BODY_WIPES.load(Ordering::SeqCst);
        let request = client
            .authenticated_post(
                "http://127.0.0.1:9/token",
                &credentials,
                vec![("code_verifier", "verifier-canary")],
            )
            .build()
            .unwrap();
        let body = request.body().unwrap().as_bytes().unwrap();
        assert!(std::str::from_utf8(body)
            .unwrap()
            .contains("verifier-canary"));
        drop(request);
        assert!(
            REQUEST_BODY_WIPES.load(Ordering::SeqCst) > before,
            "encoded OAuth body dropped without zeroization"
        );
        let before = REQUEST_BODY_WIPES.load(Ordering::SeqCst);
        assert!(client
            .token_request(
                "http://127.0.0.1:9/token",
                &credentials,
                vec![("refresh_token", "refresh-canary")]
            )
            .await
            .is_err());
        assert!(REQUEST_BODY_WIPES.load(Ordering::SeqCst) > before);
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/token", listener.local_addr().unwrap());
        let before = REQUEST_BODY_WIPES.load(Ordering::SeqCst);
        let task = tokio::spawn(async move {
            client
                .token_request(&endpoint, &credentials, vec![("token", "revoke-canary")])
                .await
        });
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut bytes = [0; 4096];
        assert!(socket.read(&mut bytes).await.unwrap() > 0);
        task.abort();
        let _ = task.await;
        drop(socket);
        for _ in 0..100 {
            if REQUEST_BODY_WIPES.load(Ordering::SeqCst) > before {
                return;
            }
            tokio::task::yield_now().await;
        }
        panic!("cancellation did not wipe the OAuth request body");
    }
}
