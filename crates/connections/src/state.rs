use crate::{
    catalog::Catalog,
    http,
    mcp::{McpClient, McpError},
    now_ms,
    oauth::{
        AuthorizationUrl, ClientCredentials, OAuthClient, OAuthError, PendingAuthorization,
        TokenStore,
    },
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    io::Write,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ConnectionState {
    NotConnected,
    Connecting,
    Connected { since_ms: u64, tool_count: usize },
    NeedsReauth,
    Error { code: String, message: String },
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConnectionStatus {
    pub connector_id: String,
    pub state: ConnectionState,
    pub mcp_url: String,
    pub last_checked_ms: Option<u64>,
}
#[derive(Debug, thiserror::Error)]
#[error("connection state store unavailable or invalid")]
pub struct StateStoreError;
pub trait StateStore: Send + Sync {
    fn load(&self) -> Result<Vec<ConnectionStatus>, StateStoreError>;
    fn save(&self, statuses: &[ConnectionStatus]) -> Result<(), StateStoreError>;
}
#[derive(Default)]
pub struct MemoryStateStore(Mutex<Vec<ConnectionStatus>>);
impl StateStore for MemoryStateStore {
    fn load(&self) -> Result<Vec<ConnectionStatus>, StateStoreError> {
        Ok(self.0.lock().map_err(|_| StateStoreError)?.clone())
    }
    fn save(&self, statuses: &[ConnectionStatus]) -> Result<(), StateStoreError> {
        *self.0.lock().map_err(|_| StateStoreError)? = statuses.to_vec();
        Ok(())
    }
}
pub struct JsonFileStateStore {
    path: PathBuf,
}
impl JsonFileStateStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}
impl StateStore for JsonFileStateStore {
    fn load(&self) -> Result<Vec<ConnectionStatus>, StateStoreError> {
        match std::fs::read(&self.path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|_| StateStoreError),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(_) => Err(StateStoreError),
        }
    }
    fn save(&self, statuses: &[ConnectionStatus]) -> Result<(), StateStoreError> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let bytes = serde_json::to_vec_pretty(statuses).map_err(|_| StateStoreError)?;
        let temporary = self.path.with_extension(format!(
            "tmp-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary).map_err(|_| StateStoreError)?;
        let result = (|| {
            file.write_all(&bytes)?;
            file.sync_all()?;
            std::fs::rename(&temporary, &self.path)?;
            if let Some(parent) = self.path.parent().filter(|p| !p.as_os_str().is_empty()) {
                std::fs::File::open(parent)?.sync_all()?;
            }
            Ok::<_, std::io::Error>(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        result.map_err(|_| StateStoreError)
    }
}
#[derive(Debug, thiserror::Error)]
pub enum ConnectionError {
    #[error("unknown connector")]
    UnknownConnector,
    #[error("pending connection was cancelled or replaced")]
    StaleHandle,
    #[error(transparent)]
    Mcp(#[from] McpError),
    #[error(transparent)]
    OAuth(#[from] OAuthError),
    #[error(transparent)]
    Store(#[from] StateStoreError),
    #[error(transparent)]
    Mail(#[from] crate::mail::MailError),
}
pub struct PendingConnect {
    owner: Arc<()>,
    generation: u64,
    pending: PendingAuthorization,
}
impl std::fmt::Debug for PendingConnect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PendingConnect([REDACTED])")
    }
}
/// Host-owned coordinator. Callers must never expose this service through generated-code IPC.
pub struct ConnectionsService {
    catalog: Catalog,
    statuses: Vec<ConnectionStatus>,
    oauth: OAuthClient,
    store: Arc<dyn StateStore>,
    owner: Arc<()>,
    generation: u64,
    active: HashMap<String, u64>,
}
/// Contains host credentials; never serialize or pass to the runner.
pub struct PreparedConnection {
    client: McpClient,
    pub tools: Vec<crate::mcp::Tool>,
}
impl PreparedConnection {
    /// Trusted host only, after exact source-template approval. Rechecks live hints.
    pub async fn read(
        &mut self,
        name: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, McpError> {
        self.client.call_tool(name, args).await
    }

    pub async fn call(
        &mut self,
        name: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, McpError> {
        self.client.call_approved_tool(name, args).await
    }
}
impl ConnectionsService {
    pub fn new(
        tokens: Arc<dyn TokenStore>,
        state: Arc<dyn StateStore>,
    ) -> Result<Self, ConnectionError> {
        Self::with_catalog(Catalog::embedded(), tokens, state)
    }
    pub fn with_catalog(
        catalog: Catalog,
        tokens: Arc<dyn TokenStore>,
        store: Arc<dyn StateStore>,
    ) -> Result<Self, ConnectionError> {
        let saved = store.load()?;
        let mut statuses = Vec::new();
        for connector in &catalog.connectors {
            let mut status = saved
                .iter()
                .find(|s| s.connector_id == connector.id)
                .cloned()
                .unwrap_or(ConnectionStatus {
                    connector_id: connector.id.clone(),
                    state: ConnectionState::NotConnected,
                    mcp_url: connector.mcp_url.clone(),
                    last_checked_ms: None,
                });
            if !status.mcp_url.is_empty() {
                http::validate_url(&status.mcp_url)?;
            }
            if status.state == ConnectionState::Connecting {
                status.state = ConnectionState::NotConnected;
            }
            statuses.push(status);
        }
        Ok(Self {
            catalog,
            statuses,
            oauth: OAuthClient::new(tokens)?,
            store,
            owner: Arc::new(()),
            generation: 0,
            active: HashMap::new(),
        })
    }
    pub fn list(&self) -> Vec<ConnectionStatus> {
        self.statuses.clone()
    }
    fn index(&self, id: &str) -> Result<usize, ConnectionError> {
        self.statuses
            .iter()
            .position(|s| s.connector_id == id)
            .ok_or(ConnectionError::UnknownConnector)
    }
    fn persist(&self) -> Result<(), ConnectionError> {
        self.store.save(&self.statuses).map_err(Into::into)
    }
    fn is_mail(&self, id: &str) -> bool {
        self.catalog.get(id).is_some_and(|c| c.mail.is_some())
    }
    fn mail_config(&self, id: &str) -> Result<crate::mail::MailConfig, ConnectionError> {
        self.oauth
            .load_secret::<crate::mail::MailConfig>(&mail_key(id))?
            .ok_or(OAuthError::NeedsReauth.into())
    }
    /// Signs in to the mailbox once to prove the settings work, then keeps them in the secret store.
    pub async fn connect_mail(
        &mut self,
        id: &str,
        config: crate::mail::MailConfig,
    ) -> Result<ConnectionStatus, ConnectionError> {
        let index = self.index(id)?;
        if !self.is_mail(id) {
            return Err(ConnectionError::UnknownConnector);
        }
        let result = async {
            crate::mail::verify(&config).await?;
            self.oauth.save_secret(&mail_key(id), &config)?;
            Ok::<_, ConnectionError>(crate::mail::tools().len())
        }
        .await;
        match result {
            Ok(tool_count) => {
                self.statuses[index].state = ConnectionState::Connected {
                    since_ms: now_ms(),
                    tool_count,
                };
                self.statuses[index].last_checked_ms = Some(now_ms());
                self.persist()?;
                Ok(self.statuses[index].clone())
            }
            Err(error) => {
                // A wrong password is something the person fixes on the form, not a broken connection.
                if !matches!(error, ConnectionError::Mail(_)) {
                    self.failed(index, &error)?;
                }
                Err(error)
            }
        }
    }
    /// The saved address and server names (never the password), for showing on the connection.
    pub fn mail_summary(&self, id: &str) -> Option<(String, String, String)> {
        let c = self.mail_config(id).ok()?;
        Some((c.email.clone(), c.imap_host.clone(), c.smtp_host.clone()))
    }
    pub async fn begin_connect(
        &mut self,
        id: &str,
        client_credentials: Option<ClientCredentials>,
    ) -> Result<(AuthorizationUrl, PendingConnect), ConnectionError> {
        if self.is_mail(id) {
            return Err(ConnectionError::UnknownConnector);
        }
        let index = self.index(id)?;
        let url = self.statuses[index].mcp_url.clone();
        http::validate_url(&url)?;
        self.active.remove(id);
        self.statuses[index].state = ConnectionState::Connecting;
        self.persist()?;
        let result = async {
            Ok(self
                .oauth
                .begin(
                    self.catalog
                        .get(id)
                        .ok_or(ConnectionError::UnknownConnector)?,
                    &url,
                    None,
                    client_credentials,
                )
                .await?)
        }
        .await;
        match result {
            Ok((url, pending)) => {
                self.generation += 1;
                self.active.insert(id.into(), self.generation);
                Ok((
                    url,
                    PendingConnect {
                        owner: self.owner.clone(),
                        generation: self.generation,
                        pending,
                    },
                ))
            }
            Err(error) => {
                self.failed(index, &error)?;
                Err(error)
            }
        }
    }
    pub async fn complete_connect(
        &mut self,
        handle: PendingConnect,
    ) -> Result<ConnectionStatus, ConnectionError> {
        let id = handle.pending.connector_id.clone();
        let index = self.index(&id)?;
        if !Arc::ptr_eq(&self.owner, &handle.owner)
            || self.active.get(&id) != Some(&handle.generation)
            || self.statuses[index].mcp_url != handle.pending.resource
        {
            return Err(ConnectionError::StaleHandle);
        }
        self.active.remove(&id);
        let result = async {
            let token = self.oauth.complete(handle.pending).await?;
            let mut client = McpClient::connect(&self.statuses[index].mcp_url, Some(token)).await?;
            Ok::<_, ConnectionError>(client.list_tools().await?.len())
        }
        .await;
        self.finish(index, result)
    }
    /// Preflight all connectors before executing ANY call. Never falls back to anonymous.
    pub async fn prepare(
        &mut self,
        id: &str,
        resource: &str,
    ) -> Result<PreparedConnection, ConnectionError> {
        let index = self.index(id)?;
        if self.statuses[index].mcp_url != resource
            || !matches!(
                self.statuses[index].state,
                ConnectionState::Connected { .. }
            )
        {
            return Err(OAuthError::NeedsReauth.into());
        }
        if self.is_mail(id) {
            let config = self.mail_config(id)?;
            return Ok(PreparedConnection {
                client: McpClient::for_mail(config),
                tools: crate::mail::tools(),
            });
        }
        let result = async {
            let token = self.oauth.access_token(id, resource).await?;
            let mut client = McpClient::connect(resource, Some(token)).await?;
            let tools = client.list_tools().await?;
            Ok::<_, ConnectionError>(PreparedConnection { client, tools })
        }
        .await;
        if let Err(error) = &result {
            self.failed(index, error)?;
        }
        result
    }
    pub async fn test(&mut self, id: &str) -> Result<ConnectionStatus, ConnectionError> {
        let index = self.index(id)?;
        if self.is_mail(id) {
            let result = async {
                crate::mail::verify(&self.mail_config(id)?).await?;
                Ok::<_, ConnectionError>(crate::mail::tools().len())
            }
            .await;
            return self.finish(index, result);
        }
        let url = self.statuses[index].mcp_url.clone();
        let result = async {
            let token = match self.oauth.access_token(id, &url).await {
                Ok(token) => Some(token),
                Err(OAuthError::NeedsReauth) => None,
                Err(e) => return Err(ConnectionError::OAuth(e)),
            };
            let mut client = McpClient::connect(&url, token).await?;
            Ok(client.list_tools().await?.len())
        }
        .await;
        self.finish(index, result)
    }
    fn finish(
        &mut self,
        index: usize,
        result: Result<usize, ConnectionError>,
    ) -> Result<ConnectionStatus, ConnectionError> {
        match result {
            Ok(tool_count) => {
                let since_ms = match self.statuses[index].state {
                    ConnectionState::Connected { since_ms, .. } => since_ms,
                    _ => now_ms(),
                };
                self.statuses[index].state = ConnectionState::Connected {
                    since_ms,
                    tool_count,
                };
                self.statuses[index].last_checked_ms = Some(now_ms());
                self.persist()?;
                Ok(self.statuses[index].clone())
            }
            Err(error) => {
                self.failed(index, &error)?;
                Err(error)
            }
        }
    }
    fn failed(&mut self, index: usize, error: &ConnectionError) -> Result<(), ConnectionError> {
        self.statuses[index].last_checked_ms = Some(now_ms());
        self.statuses[index].state = match error {
            ConnectionError::Mcp(McpError::AuthRequired { .. })
            | ConnectionError::OAuth(OAuthError::NeedsReauth | OAuthError::Token) => {
                ConnectionState::NeedsReauth
            }
            _ => ConnectionState::Error {
                code: "connection_failed".into(),
                message: error.to_string(),
            },
        };
        self.persist()
    }
    pub async fn disconnect(&mut self, id: &str) -> Result<(), ConnectionError> {
        let index = self.index(id)?;
        self.active.remove(id);
        if self.is_mail(id) {
            self.oauth.delete_secret(&mail_key(id))?;
            self.statuses[index].state = ConnectionState::NotConnected;
            return self.persist();
        }
        let result = self.oauth.disconnect(id).await;
        self.statuses[index].state = ConnectionState::NotConnected;
        self.persist()?;
        result.map_err(Into::into)
    }
    /// The same transport policy applies everywhere, including explicit local servers.
    /// Changing a URL invalidates tokens and pending flows; credentials never cross servers.
    pub async fn set_url(&mut self, id: &str, url: &str) -> Result<(), ConnectionError> {
        if self.is_mail(id) {
            return Err(ConnectionError::UnknownConnector);
        }
        http::validate_url(url)?;
        let index = self.index(id)?;
        if self.statuses[index].mcp_url != url {
            self.disconnect(id).await?;
            self.statuses[index].mcp_url = url.into();
            self.statuses[index].last_checked_ms = None;
            self.persist()?;
        }
        Ok(())
    }
}

fn mail_key(id: &str) -> String {
    format!("mail-account:{id}")
}
