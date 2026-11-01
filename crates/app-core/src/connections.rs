//! Connections (ADR 0001 §2): the core's wrapper over `macroscope_connections`.
//!
//! Tokens and OAuth client secrets live only in the vault (`mcp-token:<id>`,
//! `mcp-client:<id>`); the database keeps non-secret status. Completing a connection
//! waits for the browser for up to five minutes while holding the service, so reads go
//! to a status snapshot and each completion runs as an abortable task that `cancel` or
//! `disconnect` can stop.

use crate::dto::MailServerInput;
use crate::dto::{
    ConnectStart, ConnectionState, ConnectionSuggestion, ConnectorInfo, ConnectorTool,
};
use crate::{AppCore, CoreError, CoreResult};
use macroscope_connections::catalog::{Catalog, Registration, Verification};
use macroscope_connections::oauth::{ClientCredentials, StoreError, TokenStore};
use macroscope_connections::state::{
    ConnectionError, ConnectionState as LibState, ConnectionStatus, ConnectionsService,
    PendingConnect, StateStore, StateStoreError,
};
use macroscope_vault::{Secret, SecretRef, SecretStore};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex};
use tokio::task::AbortHandle;

/// Vault-backed token store; errors are content-free by contract.
struct VaultTokens(Arc<dyn SecretStore>);

impl TokenStore for VaultTokens {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StoreError> {
        let r = SecretRef::new(key).map_err(|_| StoreError)?;
        Ok(self
            .0
            .get(&r)
            .map_err(|_| StoreError)?
            .map(|s| s.expose().to_vec()))
    }
    fn set(&self, key: &str, bytes: &[u8]) -> Result<(), StoreError> {
        let r = SecretRef::new(key).map_err(|_| StoreError)?;
        self.0
            .set(&r, Secret::new(bytes.to_vec()))
            .map_err(|_| StoreError)
    }
    fn delete(&self, key: &str) -> Result<(), StoreError> {
        let r = SecretRef::new(key).map_err(|_| StoreError)?;
        self.0.delete(&r).map_err(|_| StoreError)
    }
}

/// Non-secret status in the core database.
struct DbStates(AppCore);

impl StateStore for DbStates {
    fn load(&self) -> Result<Vec<ConnectionStatus>, StateStoreError> {
        let rows = self
            .0
            .database()
            .connector_states()
            .map_err(|_| StateStoreError)?;
        Ok(rows
            .into_iter()
            .filter_map(|(_, v)| serde_json::from_value(v).ok())
            .collect())
    }
    fn save(&self, statuses: &[ConnectionStatus]) -> Result<(), StateStoreError> {
        for s in statuses {
            let v = serde_json::to_value(s).map_err(|_| StateStoreError)?;
            self.0
                .database()
                .set_connector_state(&s.connector_id, &v)
                .map_err(|_| StateStoreError)?;
        }
        Ok(())
    }
}

struct Pending {
    connector_id: String,
    handle: Option<PendingConnect>,
    task: Option<AbortHandle>,
    authorization_url: Option<String>,
}

#[derive(Clone)]
pub struct Connections {
    core: AppCore,
    vault: Arc<dyn SecretStore>,
    catalog: Arc<Catalog>,
    service: Arc<tokio::sync::Mutex<ConnectionsService>>,
    snapshot: Arc<Mutex<Vec<ConnectionStatus>>>,
    pending: Arc<Mutex<HashMap<String, Pending>>>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

fn user_error(e: ConnectionError) -> CoreError {
    use macroscope_connections::oauth::OAuthError as O;
    let message = match &e {
        ConnectionError::OAuth(O::ClientRequired) => {
            "Add your OAuth client ID for this service first.".to_string()
        }
        ConnectionError::OAuth(O::NeedsReauth | O::Token) => {
            "This connection needs re-authentication. Nothing ran.".into()
        }
        ConnectionError::OAuth(O::Denied) => "Sign-in was cancelled in the browser.".into(),
        ConnectionError::OAuth(O::Timeout) => {
            "Sign-in timed out after 5 minutes. Try again.".into()
        }
        ConnectionError::OAuth(O::RedirectPortUnavailable(p)) => {
            format!("Port {p} is in use, so the sign-in redirect cannot be received. Close the app using it and retry.")
        }
        ConnectionError::OAuth(O::Discovery) => {
            "This server's sign-in settings could not be found.".into()
        }
        ConnectionError::StaleHandle => "That sign-in was replaced or cancelled.".into(),
        ConnectionError::UnknownConnector => "Unknown connection.".into(),
        ConnectionError::Mail(mail) => mail.to_string(),
        other => format!("Connection failed: {other}"),
    };
    CoreError::Invalid(message)
}

// Bind even in-flight credential saves to the exact configured URL. Legacy
// unbound records require re-entry rather than guessing which server owns them.
#[derive(serde::Serialize, serde::Deserialize)]
struct StoredClient {
    resource: String,
    #[serde(flatten)]
    credentials: ClientCredentials,
}

fn client_key(id: &str) -> String {
    format!("mcp-client:{id}")
}

impl Connections {
    pub fn new(core: AppCore, vault: Arc<dyn SecretStore>) -> CoreResult<Connections> {
        let catalog = Catalog::embedded();
        let service = ConnectionsService::with_catalog(
            catalog.clone(),
            Arc::new(VaultTokens(vault.clone())),
            Arc::new(DbStates(core.clone())),
        )
        .map_err(|e| CoreError::Internal(e.to_string()))?;
        let snapshot = service.list();
        Ok(Connections {
            core,
            vault,
            catalog: Arc::new(catalog),
            service: Arc::new(tokio::sync::Mutex::new(service)),
            snapshot: Arc::new(Mutex::new(snapshot)),
            pending: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    fn client_credentials(&self, id: &str, resource: &str) -> Option<ClientCredentials> {
        let saved = SecretRef::new(client_key(id))
            .ok()
            .and_then(|r| self.vault.get(&r).ok().flatten())
            .and_then(|secret| serde_json::from_slice::<StoredClient>(secret.expose()).ok())
            .filter(|stored| stored.resource == resource)
            .map(|stored| stored.credentials);
        saved.or_else(|| {
            // Rotebot's own registered client, when the catalog carries one for this service.
            let auth = &self.catalog.get(id)?.auth;
            let client_id = auth
                .bundled_client_id
                .as_deref()
                .filter(|c| !c.is_empty())?;
            Some(ClientCredentials::new(
                client_id,
                auth.bundled_client_secret.clone(),
            ))
        })
    }

    fn info(&self, status: &ConnectionStatus) -> Option<ConnectorInfo> {
        let c = self.catalog.get(&status.connector_id)?;
        let pending = lock(&self.pending).values().any(|p| p.connector_id == c.id);
        let state = match &status.state {
            _ if pending => ConnectionState::Connecting,
            LibState::NotConnected => ConnectionState::NotConnected,
            LibState::Connecting => ConnectionState::NotConnected, // stale after restart
            LibState::Connected {
                since_ms,
                tool_count,
            } => ConnectionState::Connected {
                since_ms: *since_ms as i64,
                tool_count: *tool_count as u32,
            },
            LibState::NeedsReauth => ConnectionState::NeedsReauth,
            LibState::Error { message, .. } => ConnectionState::Error {
                message: message.chars().take(300).collect(),
            },
        };
        Some(ConnectorInfo {
            id: c.id.clone(),
            name: c.name.clone(),
            category: c.category.clone(),
            mcp_url: status.mcp_url.clone(),
            registration: match c.auth.registration {
                Registration::Dynamic => "dynamic",
                Registration::Preregistered => "preregistered",
            }
            .into(),
            verification: match c.verification {
                Verification::VerifiedLive => "verified-live",
                Verification::VerifiedDocs => "verified-docs",
                Verification::Unverified => "unverified",
            }
            .into(),
            docs_url: (!c.auth.docs_url.is_empty()).then(|| c.auth.docs_url.clone()),
            state,
            has_client_credentials: self.client_credentials(&c.id, &status.mcp_url).is_some(),
            client_setup: c.auth.client_setup.clone(),
            redirect_uri: c
                .auth
                .fixed_redirect_port
                .map(|p| format!("http://127.0.0.1:{p}/callback")),
            service: self.catalog.service_of(&c.id),
            mail: c.mail.as_ref().map(|m| crate::dto::MailInfo {
                help: m.app_password_help.clone(),
                custom_server: m.custom_server,
                imap_host: m.imap_host.clone(),
                imap_port: m.imap_port,
                smtp_host: m.smtp_host.clone(),
                smtp_port: m.smtp_port,
                smtp_starttls: m.smtp_starttls,
                email: self.saved_mail_address(&c.id),
                app_password_url: m.app_password_url.clone(),
            }),
        })
    }

    /// The saved address of a connected mailbox, read from the secret store (never the password).
    fn saved_mail_address(&self, id: &str) -> Option<String> {
        let r = SecretRef::new(format!("mail-account:{id}")).ok()?;
        let secret = self.vault.get(&r).ok()??;
        serde_json::from_slice::<serde_json::Value>(secret.expose())
            .ok()?
            .get("email")?
            .as_str()
            .map(str::to_string)
    }

    fn info_for(&self, id: &str) -> CoreResult<ConnectorInfo> {
        if crate::api::is_api(id) {
            return self.core.api_info(id);
        }
        let snapshot = lock(&self.snapshot).clone();
        let status = snapshot
            .iter()
            .find(|s| s.connector_id == id)
            .ok_or(CoreError::NotFound("connection"))?;
        self.info(status).ok_or(CoreError::NotFound("connection"))
    }

    pub fn list(&self) -> Vec<ConnectorInfo> {
        let snapshot = lock(&self.snapshot).clone();
        let mut list: Vec<_> = snapshot.iter().filter_map(|s| self.info(s)).collect();
        list.extend(self.core.api_infos().unwrap_or_default());
        list
    }

    pub async fn set_url(&self, id: &str, url: &str) -> CoreResult<ConnectorInfo> {
        self.cancel(id);
        let mut service = self.service.lock().await;
        let url = url.trim();
        macroscope_connections::validate_url(url).map_err(|e| user_error(e.into()))?;
        let old = service
            .list()
            .into_iter()
            .find(|s| s.connector_id == id)
            .ok_or(CoreError::NotFound("connection"))?;
        if old.mcp_url != url {
            // Delete before publishing a changed URL; failure leaves admission at
            // the old server. Tokens and pending flows are cleared by set_url.
            let r = SecretRef::new(client_key(id)).map_err(|e| CoreError::Vault(e.to_string()))?;
            self.vault
                .delete(&r)
                .map_err(|e| CoreError::Vault(e.to_string()))?;
        }
        let result = service.set_url(id, url).await;
        *lock(&self.snapshot) = service.list();
        drop(service);
        result.map_err(user_error)?;
        self.core.log_activity(
            "connection",
            &format!("Changed the server address for {id}"),
            None,
        );
        self.info_for(id)
    }

    pub fn set_client_credentials(
        &self,
        id: &str,
        client_id: &str,
        client_secret: Option<&str>,
    ) -> CoreResult<ConnectorInfo> {
        if self.catalog.get(id).is_none() {
            return Err(CoreError::NotFound("connection"));
        }
        let client_id = client_id.trim();
        if client_id.is_empty() || client_id.len() > 500 || client_id.contains(char::is_whitespace)
        {
            return Err(CoreError::Invalid(
                "Enter the OAuth client ID exactly as shown by the provider.".into(),
            ));
        }
        let secret = client_secret
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from);
        let resource = self.info_for(id)?.mcp_url;
        let creds = StoredClient {
            resource,
            credentials: ClientCredentials::new(client_id, secret),
        };
        let bytes = serde_json::to_vec(&creds).map_err(|e| CoreError::Internal(e.to_string()))?;
        let r = SecretRef::new(client_key(id)).map_err(|e| CoreError::Vault(e.to_string()))?;
        self.vault
            .set(&r, Secret::new(bytes))
            .map_err(|e| CoreError::Vault(e.to_string()))?;
        self.core.log_activity(
            "connection",
            &format!("Saved an OAuth client for {id}"),
            None,
        );
        self.info_for(id)
    }

    pub async fn begin_connect(&self, id: &str) -> CoreResult<ConnectStart> {
        let connector = self
            .catalog
            .get(id)
            .ok_or(CoreError::NotFound("connection"))?;
        self.cancel(id);
        let pending_id = format!("pending_{}", uuid::Uuid::new_v4().simple());
        let task = {
            // Register an abort handle before discovery yields, so cancel during
            // begin cannot leave a new browser flow behind after disconnect.
            let mut pending = lock(&self.pending);
            let service = self.service.clone();
            let snapshot = self.snapshot.clone();
            let owner = self.clone();
            let registration = connector.auth.registration;
            let id = id.to_string();
            let task_id = id.clone();
            let task = tokio::spawn(async move {
                let mut service = service.lock().await;
                let resource = service
                    .list()
                    .into_iter()
                    .find(|s| s.connector_id == task_id)
                    .ok_or(CoreError::NotFound("connection"))?
                    .mcp_url;
                let creds = owner.client_credentials(&task_id, &resource);
                if registration == Registration::Preregistered && creds.is_none() {
                    return Err(CoreError::Invalid(
                        "Add your OAuth client ID for this service first.".into(),
                    ));
                }
                let result = service.begin_connect(&task_id, creds).await;
                *lock(&snapshot) = service.list();
                result.map_err(user_error)
            });
            pending.insert(
                pending_id.clone(),
                Pending {
                    connector_id: id,
                    handle: None,
                    task: Some(task.abort_handle()),
                    authorization_url: None,
                },
            );
            task
        };
        let result = task.await;
        let mut pending = lock(&self.pending);
        let Some(slot) = pending.get_mut(&pending_id) else {
            return Err(CoreError::Invalid("Sign-in was cancelled.".into()));
        };
        let (url, handle) = match result {
            Ok(Ok(pair)) => pair,
            other => {
                pending.remove(&pending_id);
                return Err(match other {
                    Ok(Err(e)) => e,
                    _ => CoreError::Invalid("Sign-in was cancelled.".into()),
                });
            }
        };
        let authorization_url = url.as_str().to_string();
        slot.handle = Some(handle);
        slot.task = None;
        slot.authorization_url = Some(authorization_url.clone());
        Ok(ConnectStart {
            connector_id: id.into(),
            authorization_url,
            pending_id,
        })
    }

    /// Waits for the browser redirect, exchanges the code, and verifies the server lists tools.
    pub async fn complete_connect(&self, pending_id: &str) -> CoreResult<ConnectorInfo> {
        let (id, task) = {
            let mut pending = lock(&self.pending);
            let slot = pending
                .get_mut(pending_id)
                .ok_or(CoreError::NotFound("pending sign-in"))?;
            let handle = slot.handle.take().ok_or(CoreError::Invalid(
                "This sign-in is already being completed.".into(),
            ))?;
            let service = self.service.clone();
            let snapshot = self.snapshot.clone();
            let task = tokio::spawn(async move {
                let mut service = service.lock().await;
                let result = service.complete_connect(handle).await;
                *lock(&snapshot) = service.list();
                result
            });
            slot.task = Some(task.abort_handle());
            (slot.connector_id.clone(), task)
        };
        let result = task.await;
        lock(&self.pending).remove(pending_id);
        match result {
            Ok(Ok(status)) => {
                let tools = match status.state {
                    LibState::Connected { tool_count, .. } => tool_count,
                    _ => 0,
                };
                let name = self
                    .catalog
                    .get(&id)
                    .map(|c| c.name.clone())
                    .unwrap_or(id.clone());
                self.core.log_activity(
                    "connection",
                    &format!("Connected {name}"),
                    Some(format!("{tools} tools available")),
                );
                self.info_for(&id)
            }
            Ok(Err(e)) => Err(user_error(e)),
            Err(_) => Err(CoreError::Invalid("Sign-in was cancelled.".into())),
        }
    }

    /// Signs in to a mailbox with an address and app password. Wrong details leave nothing saved.
    pub async fn connect_mail(
        &self,
        id: &str,
        email: &str,
        password: &str,
        server: Option<MailServerInput>,
    ) -> CoreResult<ConnectorInfo> {
        let preset = self
            .catalog
            .get(id)
            .and_then(|c| c.mail.clone())
            .ok_or(CoreError::NotFound("connection"))?;
        let server = server.unwrap_or(MailServerInput {
            imap_host: preset.imap_host.clone(),
            imap_port: preset.imap_port,
            smtp_host: preset.smtp_host.clone(),
            smtp_port: preset.smtp_port,
            smtp_starttls: preset.smtp_starttls,
        });
        // Presets are fixed; only a custom server may name its own hosts.
        let server = if preset.custom_server {
            server
        } else {
            MailServerInput {
                imap_host: preset.imap_host.clone(),
                imap_port: preset.imap_port,
                smtp_host: preset.smtp_host.clone(),
                smtp_port: preset.smtp_port,
                smtp_starttls: preset.smtp_starttls,
            }
        };
        let config = macroscope_connections::mail::MailConfig {
            email: email.trim().to_string(),
            password: password.to_string(),
            imap_host: server.imap_host.trim().to_ascii_lowercase(),
            imap_port: server.imap_port,
            smtp_host: server.smtp_host.trim().to_ascii_lowercase(),
            smtp_port: server.smtp_port,
            smtp_starttls: server.smtp_starttls,
            tls: true,
        };
        let mut service = self.service.lock().await;
        let result = service.connect_mail(id, config).await;
        *lock(&self.snapshot) = service.list();
        drop(service);
        result.map_err(user_error)?;
        let name = self
            .catalog
            .get(id)
            .map(|c| c.name.clone())
            .unwrap_or(id.into());
        self.core
            .log_activity("connection", &format!("Connected {name}"), None);
        self.info_for(id)
    }

    /// Cancels any sign-in in progress for a connector.
    pub fn cancel(&self, id: &str) {
        let mut pending = lock(&self.pending);
        pending.retain(|_, p| {
            if p.connector_id == id {
                if let Some(t) = &p.task {
                    t.abort();
                }
                false
            } else {
                true
            }
        });
    }

    pub async fn test(&self, id: &str) -> CoreResult<ConnectorInfo> {
        if crate::api::is_api(id) {
            return self.core.test_api_connection(id).await;
        }
        let mut service = self.service.lock().await;
        let result = service.test(id).await;
        *lock(&self.snapshot) = service.list();
        drop(service);
        result.map_err(user_error)?;
        self.info_for(id)
    }

    pub async fn disconnect(&self, id: &str) -> CoreResult<ConnectorInfo> {
        if crate::api::is_api(id) {
            return self.core.disconnect_api(id).await;
        }
        self.cancel(id);
        let mut service = self.service.lock().await;
        let result = service.disconnect(id).await;
        *lock(&self.snapshot) = service.list();
        drop(service);
        self.core
            .log_activity("connection", &format!("Disconnected {id}"), None);
        // Local tokens are deleted even when remote revocation fails.
        if let Err(e) = result {
            if !matches!(e, ConnectionError::OAuth(_)) {
                return Err(user_error(e));
            }
        }
        self.info_for(id)
    }

    /// Tools a server lists without signing in (Gmail allows this); empty otherwise.
    pub async fn preview_tools(&self, id: &str) -> CoreResult<Vec<ConnectorTool>> {
        if crate::api::is_api(id) {
            self.core.api_info(id)?;
            return Ok(vec![crate::api::read_tool(&crate::api::api_base_path(&self.core.api_info(id)?.mcp_url), &self.core.known_paths_note(id))]);
        }
        let url = self.info_for(id)?.mcp_url;
        let is_mail = self.catalog.get(id).is_some_and(|c| c.mail.is_some());
        if url.is_empty() && !is_mail {
            return Ok(vec![]);
        }
        let tools = if is_mail {
            macroscope_connections::mail::tools()
        } else if self.connected_ids().contains(id) {
            let mut service = self.service.lock().await;
            let result = service.prepare(id, &url).await;
            *lock(&self.snapshot) = service.list();
            result.map_err(user_error)?.tools
        } else {
            macroscope_connections::preview_tools(&url)
                .await
                .unwrap_or_default()
        };
        Ok(tools
            .into_iter()
            .take(100)
            .map(|t| ConnectorTool {
                input_schema: t.input_schema,
                name: t.name,
                description: t
                    .description
                    .unwrap_or_default()
                    .chars()
                    .take(1000)
                    .collect(),
                read_only: t.annotations.read_only_hint,
                destructive: t.annotations.destructive_hint,
            })
            .collect())
    }

    /// Selection is explicit user intent. Tool metadata remains untrusted data.
    pub async fn select_for_task(
        &self,
        draft_id: &str,
        ids: &[String],
    ) -> CoreResult<crate::dto::TaskDraft> {
        if ids.len() > 5 {
            return Err(CoreError::Invalid(
                "Choose at most five connections.".into(),
            ));
        }
        let mut selected = Vec::new();
        for id in ids {
            if selected
                .iter()
                .any(|c: &crate::dto::SelectedConnection| &c.connector_id == id)
            {
                return Err(CoreError::Invalid("Duplicate connection.".into()));
            }
            let info = self.info_for(id)?;
            if !matches!(info.state, ConnectionState::Connected { .. }) {
                return Err(CoreError::Invalid(
                    "Connect this service before using it.".into(),
                ));
            }
            let tools = self.preview_tools(id).await?;
            if serde_json::to_vec(&tools).map_or(true, |v| v.len() > 65_536)
                || tools
                    .iter()
                    .any(|t| t.input_schema.to_string().len() > 8192)
            {
                return Err(CoreError::Invalid(
                    "Server tool schemas exceed the builder metadata limit".into(),
                ));
            }
            for tool in &tools {
                jsonschema::validator_for(&tool.input_schema).map_err(|_| {
                    CoreError::Invalid("Server supplied an invalid input schema.".into())
                })?;
            }
            selected.push(crate::dto::SelectedConnection {
                connector_id: id.clone(),
                name: info.name,
                resource: info.mcp_url,
                tools,
            });
        }
        let _admission = self.core.admit_local()?;
        let mut draft = self.core.load_draft(draft_id)?;
        if draft.required_connections != selected {
            draft.access_plan = None;
        }
        draft.required_connections = selected;
        if !draft.required_connections.is_empty() {
            draft.kind = crate::dto::TaskKind::Program;
            draft.questions.clear();
        }
        draft.revision += 1;
        crate::drafts::refresh(&mut draft);
        self.core.save_draft(&draft)?;
        Ok(draft)
    }

    pub async fn commit_run(
        &self,
        run_id: &str,
        plan_hash: &str,
        reviewed: bool,
    ) -> CoreResult<crate::dto::CommitResult> {
        let actions = self.core.planned_actions(run_id)?;
        if actions.is_empty() {
            return self.core.commit_run(run_id, plan_hash);
        }
        // Keep the service lock through sends so URL changes/disconnect cannot race approval.
        let mut service = self.service.clone().lock_owned().await;
        let mut prepared = HashMap::new();
        for action in &actions {
            if !prepared.contains_key(&action.call.connector_id) {
                let result = service
                    .prepare(&action.call.connector_id, &action.resource)
                    .await;
                *lock(&self.snapshot) = service.list();
                prepared.insert(
                    action.call.connector_id.clone(),
                    result.map_err(user_error)?,
                );
            }
            let client = &prepared[&action.call.connector_id];
            let mut tools = client
                .tools
                .iter()
                .filter(|t| t.name == action.call.tool_name);
            let tool = tools.next().ok_or_else(|| {
                CoreError::Invalid("The server no longer offers this tool. Stage again.".into())
            })?;
            if tools.next().is_some()
                || tool.annotations.read_only_hint != action.read_only_hint
                || tool.annotations.destructive_hint != action.destructive_hint
                || !jsonschema::validator_for(&tool.input_schema)
                    .is_ok_and(|v| v.is_valid(&action.call.args))
            {
                return Err(CoreError::Invalid(
                    "The server tool changed. Review a fresh plan.".into(),
                ));
            }
        }
        let core = self.core.clone();
        let run_id = run_id.to_string();
        let plan_hash = plan_hash.to_string();
        let runtime = tokio::runtime::Handle::current();
        tokio::task::spawn_blocking(move || {
            let _service_guard = service;
            core.commit_run_actions(&run_id, &plan_hash, reviewed, |action| {
                let result = runtime.block_on(prepared.get_mut(&action.call.connector_id).expect("preflighted").call(&action.call.tool_name, action.call.args.clone()));
                use macroscope_effect_broker::ActionStatus as S;
                match result {
                    Ok(value) => (if value["isError"] == true { S::Failed } else { S::Succeeded }, value.to_string()),
                    Err(macroscope_connections::mcp::McpError::AuthRequired { .. } | macroscope_connections::mcp::McpError::Rpc | macroscope_connections::mcp::McpError::TokenEcho) => (S::Failed, "Server rejected the action. No retry was attempted.".into()),
                    Err(_) => (S::OutcomeUnknown, "The request may have been sent but its response was lost. Check the service; Rotebot will never retry this call.".into()),
                }
            })
        }).await.map_err(|_| CoreError::Internal("Connection commit interrupted; check activity before running again.".into()))?
    }

    fn connected_ids(&self) -> HashSet<String> {
        lock(&self.snapshot)
            .iter()
            .filter(|s| matches!(s.state, LibState::Connected { .. }))
            .map(|s| s.connector_id.clone())
            .collect()
    }

    /// Detected but not connected, not dismissed connectors, most-seen first.
    pub fn suggestions(&self) -> CoreResult<Vec<ConnectionSuggestion>> {
        let now = self.core.now_ms();
        let hidden: HashSet<String> = self
            .core
            .database()
            .hidden_suggestions(now)?
            .into_iter()
            .collect();
        let connected = self.connected_ids();
        let connected_services: HashSet<String> = connected
            .iter()
            .map(|id| self.catalog.service_of(id))
            .collect();
        // connector -> (total count, strongest signal kind); recording signals win over text.
        let mut seen: BTreeMap<String, (i64, String)> = BTreeMap::new();
        for d in self.core.database().detections()? {
            let entry = seen
                .entry(d.connector_id.clone())
                .or_insert((0, d.signal_kind.clone()));
            entry.0 += d.count;
            if entry.1 == "task_text" && d.signal_kind != "task_text" {
                entry.1 = d.signal_kind.clone();
            }
        }
        let mut out: Vec<ConnectionSuggestion> = seen
            .into_iter()
            .filter(|(id, _)| {
                !hidden.contains(id)
                    && !connected.contains(id)
                    && !connected_services.contains(&self.catalog.service_of(id))
            })
            .filter_map(|(id, (count, kind))| {
                let name = self.catalog.get(&id)?.name.clone();
                let message = if kind == "task_text" {
                    format!("You mentioned {name} in your task. Connect {name} so tools can work with it.")
                } else {
                    format!("You used {name} while recording. Connect {name} so tools can work with it.")
                };
                Some(ConnectionSuggestion { connector_id: id, connector_name: name, message, signal_kind: kind, count })
            })
            .collect();
        out.sort_by(|a, b| {
            b.count
                .cmp(&a.count)
                .then(a.connector_id.cmp(&b.connector_id))
        });
        // One suggestion per service: Gmail by Google sign-in and by app password are one ask.
        let mut services = HashSet::new();
        out.retain(|s| services.insert(self.catalog.service_of(&s.connector_id)));
        Ok(out)
    }

    pub fn dismiss(&self, id: &str) -> CoreResult<()> {
        self.core.database().dismiss_suggestion(id, None)?;
        Ok(())
    }

    pub fn snooze(&self, id: &str, days: i64) -> CoreResult<()> {
        let until = self.core.now_ms() + days * 24 * 60 * 60 * 1000;
        self.core.database().dismiss_suggestion(id, Some(until))?;
        Ok(())
    }

    /// The shell may open exactly the authorization URLs this service produced.
    pub fn allow_authorization_url(&self, url: &str) -> bool {
        lock(&self.pending)
            .values()
            .any(|p| p.authorization_url.as_deref() == Some(url))
    }
    pub fn allow_documentation_url(&self, url: &str) -> bool {
        macroscope_connections::rest::validate_url(url, false).is_ok()
            && (self
                .core
                .api_infos()
                .unwrap_or_default()
                .iter()
                .any(|c| c.docs_url.as_deref() == Some(url))
                || self
                    .core
                    .database()
                    .setting("api-research-docs")
                    .ok()
                    .flatten()
                    .and_then(|v| v.as_array().cloned())
                    .is_some_and(|urls| urls.iter().any(|u| u.as_str() == Some(url))))
    }
}

/// Fake connection layer: checks and staging only record and validate, never call a server.
pub(crate) fn validate_calls(
    calls: &[macroscope_domain::PlannedCall],
    selected: &[crate::dto::SelectedConnection],
) -> Result<Vec<macroscope_effect_broker::PlannedAction>, String> {
    if calls.len() > 5 {
        return Err("At most 5 connection calls per run.".into());
    }
    calls
        .iter()
        .map(|call| {
            if crate::api::is_api(&call.connector_id) {
                return Err(
                    "Custom APIs are read sources and cannot request external actions".into(),
                );
            }
            if !call.args.is_object()
                || serde_json::to_vec(&call.args).map_or(true, |b| b.len() > 65_536)
            {
                return Err("Connection arguments must be a JSON object at most 64 KiB.".into());
            }
            let connector = selected
                .iter()
                .find(|c| c.connector_id == call.connector_id)
                .ok_or("Undeclared connector")?;
            let mut tools = connector.tools.iter().filter(|t| t.name == call.tool_name);
            let tool = tools.next().ok_or("Undeclared tool")?;
            if tools.next().is_some() {
                return Err("Ambiguous server tool".into());
            }
            let validator = jsonschema::validator_for(&tool.input_schema)
                .map_err(|_| "Invalid server input schema")?;
            if !validator.is_valid(&call.args) {
                return Err("Connection arguments do not match the tool input schema".into());
            }
            Ok(macroscope_effect_broker::PlannedAction {
                call: call.clone(),
                service: connector.name.clone(),
                resource: connector.resource.clone(),
                read_only_hint: tool.read_only,
                destructive_hint: tool.destructive,
            })
        })
        .collect()
}

impl AppCore {
    pub(crate) fn planned_actions(
        &self,
        run_id: &str,
    ) -> CoreResult<Vec<macroscope_effect_broker::PlannedAction>> {
        Ok(lock(&self.inner.staged)
            .get(run_id)
            .ok_or(CoreError::NotFound("staged run"))?
            .staged
            .plan()
            .planned_calls
            .clone())
    }
    pub(crate) fn validate_connection_authority(
        &self,
        tool_id: &str,
        version: &str,
        selected: &[crate::dto::SelectedConnection],
    ) -> CoreResult<()> {
        let version = self
            .db()
            .version(tool_id, version)?
            .ok_or(CoreError::NotFound("version"))?;
        let grant = self
            .db()
            .active_grant(tool_id, &version.hash)?
            .ok_or_else(|| CoreError::Invalid("Tool permissions revoked.".into()))?;
        let expected: Vec<_> = selected.iter().map(|c| serde_json::json!({"connectorId":c.connector_id,"toolNames":c.tools.iter().map(|t| &t.name).collect::<Vec<_>>()})).collect();
        if !selected.is_empty()
            && (version.manifest["capabilities"]["mcp"] != serde_json::json!(expected)
                || grant.2["mcp"] != serde_json::json!(expected))
        {
            return Err(CoreError::Invalid(
                "Undeclared connection or missing mcp.call grant.".into(),
            ));
        }
        for c in selected {
            if crate::api::is_api(&c.connector_id) {
                self.validate_api_read(
                    &c.connector_id,
                    &c.resource,
                    "read",
                    &serde_json::json!({}),
                )?;
                continue;
            }
            let state = self
                .db()
                .connector_states()?
                .into_iter()
                .find(|(id, _)| id == &c.connector_id)
                .and_then(|(_, v)| serde_json::from_value::<ConnectionStatus>(v).ok());
            if state.is_none_or(|s| {
                s.mcp_url != c.resource || !matches!(s.state, LibState::Connected { .. })
            }) {
                return Err(CoreError::Invalid(format!(
                    "Reconnect {}. Nothing ran.",
                    c.name
                )));
            }
        }
        Ok(())
    }
}

/// Source declarations confer no authority until the exact templates are reviewed.
pub fn validate_sources(
    program: &macroscope_builder::Program,
    selected: &[crate::dto::SelectedConnection],
) -> Result<(), String> {
    macroscope_builder::program_hosts(program, true)?;
    for source in &program.connection_sources {
        let c = selected
            .iter()
            .find(|c| c.connector_id == source.connector_id)
            .ok_or("Unapproved connection source")?;
        let mut tools = c.tools.iter().filter(|t| t.name == source.tool_name);
        let t = tools
            .next()
            .ok_or("Reconnect or rebuild: source tool missing")?;
        if tools.next().is_some() || t.read_only != Some(true) || t.destructive == Some(true) {
            return Err(
                "Connection source must be explicitly read-only and not destructive".into(),
            );
        }
    }
    Ok(())
}

impl Connections {
    pub(crate) async fn read_sources(
        &self,
        sources: &[macroscope_builder::ConnectionSource],
        selected: &[crate::dto::SelectedConnection],
    ) -> CoreResult<BTreeMap<String, String>> {
        let mut service = self.service.lock().await;
        let mut prepared = HashMap::new();
        // All connectors, tools and substituted arguments are checked before any read.
        let mut calls = Vec::new();
        for source in sources {
            let c = selected
                .iter()
                .find(|c| c.connector_id == source.connector_id)
                .ok_or_else(|| CoreError::Invalid("Unapproved connection source".into()))?;
            if crate::api::is_api(&c.connector_id) {
                let args = self.core.source_args(&source.args_json)?;
                self.core.validate_api_read(
                    &c.connector_id,
                    &c.resource,
                    &source.tool_name,
                    &args,
                )?;
                calls.push((source, c, args));
                continue;
            }
            if !prepared.contains_key(&c.connector_id) {
                let result = service.prepare(&c.connector_id, &c.resource).await;
                *lock(&self.snapshot) = service.list();
                prepared.insert(
                    c.connector_id.clone(),
                    result.map_err(|_| {
                        CoreError::Invalid(format!("Reconnect {}. Nothing ran.", c.name))
                    })?,
                );
            }
            let args = self.core.source_args(&source.args_json)?;
            let mut tools = prepared[&c.connector_id]
                .tools
                .iter()
                .filter(|t| t.name == source.tool_name);
            let t = tools.next().ok_or_else(|| {
                CoreError::Invalid("Reconnect or rebuild: source tool missing".into())
            })?;
            if tools.next().is_some()
                || t.annotations.read_only_hint != Some(true)
                || t.annotations.destructive_hint == Some(true)
                || !jsonschema::validator_for(&t.input_schema).is_ok_and(|v| v.is_valid(&args))
            {
                return Err(CoreError::Invalid(
                    "Reconnect or rebuild: source tool or schema changed".into(),
                ));
            }
            calls.push((source, c, args));
        }
        let mut results = BTreeMap::new();
        for (source, c, args) in calls {
            if crate::api::is_api(&c.connector_id) {
                let target = self.core.validate_api_read(
                    &c.connector_id,
                    &c.resource,
                    &source.tool_name,
                    &args,
                )?;
                let text = self
                    .core
                    .api_read_at(&c.connector_id, &target, false, target != c.resource)
                    .await?;
                self.core.remember_api_read(&c.connector_id, &target, &text, "Worked in a tool")?;
                results.insert(source.name.clone(), text);
                continue;
            }
            self.core.db().insert_outbound(&macroscope_storage::db::OutboundRow {
                id: crate::new_id("out"), at_ms: self.core.now(), purpose: "Connection read".into(), destination: c.name.clone(),
                route: "connection".into(), synthetic_only: false, cost_micros: None,
                payload_text: Some(serde_json::json!({"tool":source.tool_name,"argsTemplate":serde_json::from_str::<serde_json::Value>(&source.args_json).unwrap()}).to_string()),
            })?;
            let result = prepared
                .get_mut(&c.connector_id)
                .unwrap()
                .read(&source.tool_name, args)
                .await
                .map_err(|e| {
                    if matches!(
                        e,
                        macroscope_connections::mcp::McpError::AuthRequired { .. }
                    ) {
                        CoreError::Invalid(format!("Reconnect {}. Nothing ran.", c.name))
                    } else if matches!(e, macroscope_connections::mcp::McpError::ToolNotReadOnly) {
                        CoreError::Invalid("Reconnect or rebuild: source tool changed".into())
                    } else {
                        CoreError::Invalid(format!("Connection read failed: {e}"))
                    }
                })?;
            results.insert(
                source.name.clone(),
                source_text(&result).map_err(CoreError::Invalid)?,
            );
        }
        Ok(results)
    }
}

pub fn source_text(result: &serde_json::Value) -> Result<String, String> {
    if result["isError"] == true {
        // The service says why (a bad argument, a missing permission). Show a short, plain version
        // of it; the text is untrusted, so only printable characters and a bounded length are kept.
        let why: String = result["content"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|p| p["type"] == "text")
            .filter_map(|p| p["text"].as_str())
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .filter(|c| !c.is_control())
            .take(300)
            .collect();
        return Err(if why.trim().is_empty() {
            "Connection read was rejected by the service".into()
        } else {
            format!(
                "Connection read was rejected by the service: {}",
                why.trim()
            )
        });
    }
    let text = if let Some(v) = result.get("structuredContent") {
        v.to_string()
    } else {
        result["content"]
            .as_array()
            .ok_or("Invalid connection read content")?
            .iter()
            .filter(|p| p["type"] == "text")
            .map(|p| p["text"].as_str().ok_or("Invalid connection read text"))
            .collect::<Result<Vec<_>, _>>()?
            .join("\n")
    };
    if text.len() > 2 * 1024 * 1024 {
        return Err("Connection read exceeds 2 MB".into());
    }
    Ok(text)
}

#[cfg(test)]
mod action_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_rejected_read_shows_the_services_reason_trimmed_and_without_control_characters() {
        let result = json!({"isError": true, "content": [{"type": "text", "text": "Invalid argument:\n pageSize must be <= 50"}]});
        assert_eq!(
            source_text(&result).unwrap_err(),
            "Connection read was rejected by the service: Invalid argument: pageSize must be <= 50"
        );
        let long = json!({"isError": true, "content": [{"type": "text", "text": "x".repeat(900)}]});
        assert!(source_text(&long).unwrap_err().len() < 400);
        assert_eq!(
            source_text(&json!({"isError": true})).unwrap_err(),
            "Connection read was rejected by the service"
        );
    }

    #[test]
    fn undeclared_connectors_tools_invalid_schemas_and_excess_calls_fail_closed() {
        let selected = vec![crate::dto::SelectedConnection {
            connector_id: "gmail".into(),
            name: "Gmail".into(),
            resource: "https://example.invalid/mcp".into(),
            tools: vec![ConnectorTool {
                name: "create_draft".into(),
                description: "untrusted instructions".into(),
                input_schema: json!({"type":"object","additionalProperties":false,"properties":{"to":{"type":"string"}},"required":["to"]}),
                read_only: Some(false),
                destructive: None,
            }],
        }];
        let good = macroscope_domain::PlannedCall {
            connector_id: "gmail".into(),
            tool_name: "create_draft".into(),
            args: json!({"to":"user@example.invalid"}),
        };
        assert!(validate_calls(std::slice::from_ref(&good), &selected).is_ok());
        for bad in [
            macroscope_domain::PlannedCall {
                connector_id: "slack".into(),
                ..good.clone()
            },
            macroscope_domain::PlannedCall {
                tool_name: "send_email".into(),
                ..good.clone()
            },
            macroscope_domain::PlannedCall {
                args: json!({"to":3}),
                ..good.clone()
            },
            macroscope_domain::PlannedCall {
                args: json!({"to":"x","extra":true}),
                ..good.clone()
            },
        ] {
            assert!(validate_calls(&[bad], &selected).is_err());
        }
        assert!(validate_calls(&vec![good; 6], &selected).is_err());
    }
}

impl AppCore {
    /// Stages a no-input tool for its connection actions alone: no output folder,
    /// no output file. The private folder only holds the staging area.
    pub async fn stage_actions_no_input(&self, tool_id: &str) -> CoreResult<crate::StageResult> {
        let detail = self.get_tool(tool_id)?;
        let program = detail
            .program
            .as_ref()
            .ok_or_else(|| CoreError::Invalid("Only program tools can send actions.".into()))?;
        let private = self.inner.cfg.data_dir.join("digest");
        std::fs::create_dir_all(&private).map_err(|e| CoreError::Internal(e.to_string()))?;
        let dir = self.register_picked_dir(&private)?;
        self.stage_inner_files(tool_id, None, &dir.handle_id, &program.output_name, None, false)
            .await
    }
}

/// Scheduled results go out through the mailbox the person approved with the schedule. The
/// same checks as an approved connection action apply: the tool must still be the plain
/// send tool with the expected shape, and the send is tried once.
impl crate::schedules::ResultMailer for Connections {
    fn send<'a>(
        &'a self,
        connector_id: &'a str,
        to: &'a str,
        subject: &'a str,
        body: &'a str,
    ) -> crate::schedules::MailFuture<'a> {
        Box::pin(async move {
            let resource = lock(&self.snapshot)
                .iter()
                .find(|s| s.connector_id == connector_id)
                .map(|s| s.mcp_url.clone())
                .ok_or("that mailbox is no longer connected")?;
            let args = serde_json::json!({"to": to, "subject": subject, "body": body});
            let mut service = self.service.clone().lock_owned().await;
            let prepared = service.prepare(connector_id, &resource).await;
            *lock(&self.snapshot) = service.list();
            let mut prepared =
                prepared.map_err(|_| "the mailbox needs to be connected again".to_string())?;
            let mut tools = prepared.tools.iter().filter(|t| t.name == "send_message");
            let tool = tools.next().ok_or("this connection cannot send email")?;
            if tools.next().is_some()
                || tool.annotations.read_only_hint == Some(true)
                || tool.annotations.destructive_hint == Some(true)
                || !jsonschema::validator_for(&tool.input_schema).is_ok_and(|v| v.is_valid(&args))
            {
                return Err("this connection's send tool is not the one that was approved".into());
            }
            match prepared.call("send_message", args).await {
                Ok(value) if value["isError"] == true => Err(value["content"][0]["text"]
                    .as_str()
                    .map(|t| t.chars().take(160).collect())
                    .unwrap_or_else(|| "the mail server refused it".into())),
                Ok(_) => Ok(()),
                Err(_) => Err("the mail server did not confirm it; check your Sent folder, it will not be retried".into()),
            }
        })
    }
}
