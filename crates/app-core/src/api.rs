//! Immutable, host-owned custom API connections. The worker sees a read tool,
//! never a URL override, authentication header, or credential.
use crate::*;
use macroscope_connections::rest::{self, ReadError};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiAuth {
    None,
    Bearer,
    Header,
    /// The key travels in the web address (`?access_token=…`); `header_name` is its parameter name.
    Query,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApiConfig {
    pub name: String,
    pub url: String,
    pub auth: ApiAuth,
    pub header_name: String,
    pub docs_url: Option<String>,
    /// Further key headers the API also needs (an app id beside the access token). Names only:
    /// each value is entered in the native dialog and kept in the host vault.
    #[serde(default)]
    pub extra_header_names: Vec<String>,
}
pub const MAX_EXTRA_HEADERS: usize = 2;

/// What people paste around a key: quotes, a leading `Authorization:` or `Bearer` (Macroscope adds
/// that itself, so a pasted one is sent twice), and stray spaces or line breaks. Keys never contain spaces.
pub fn clean_key(raw: &str) -> String {
    let mut s = raw.trim().to_string();
    loop {
        let before = s.clone();
        s = s.trim().to_string();
        for q in ['"', '\'', '`'] {
            if s.len() > 1 && s.starts_with(q) && s.ends_with(q) {
                s = s[1..s.len() - 1].to_string();
            }
        }
        let lower = s.to_ascii_lowercase();
        if lower.starts_with("authorization:") {
            s = s["authorization:".len()..].to_string();
        } else if lower.starts_with("bearer ") || lower.starts_with("bearer:") {
            s = s["bearer ".len()..].to_string();
        }
        if s == before {
            break;
        }
    }
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SavedApi {
    id: String,
    config: ApiConfig,
    state: ConnectionState,
}

const APIS: &str = "custom_api_connections_v1";
/// Dropping a reqwest future closes dispatch. Unlike a child process it needs no
/// asynchronous reap, so acknowledge even if an enclosing caller is cancelled.
pub(crate) struct HttpEpochGuard(pub Option<macroscope_policy::EpochGuard>);
impl Drop for HttpEpochGuard {
    fn drop(&mut self) {
        if let Some(guard) = self.0.take() {
            guard.acknowledge(true);
        }
    }
}
pub(crate) fn is_api(id: &str) -> bool {
    id.starts_with("api_")
}

/// The version prefix every path on a service shares, taken from the saved address:
/// `/v1` for `https://api.akahu.io/v1/me`, `/api/v2` for `…/api/v2/users`, empty when there is none.
pub fn api_base_path(address: &str) -> String {
    let Ok(url) = url::Url::parse(address) else { return String::new() };
    let segments: Vec<&str> = url.path().split('/').filter(|s| !s.is_empty()).collect();
    let version = |s: &str| s == "api" || s == "rest" || {
        let s = s.strip_prefix(['v', 'V']).unwrap_or("");
        !s.is_empty() && s.chars().all(|c| c.is_ascii_digit() || c == '.')
    };
    match segments.iter().rposition(|s| version(s)) {
        Some(last) => format!("/{}", segments[..=last].join("/")),
        None => String::new(),
    }
}

/// The array a list page keeps its entries in.
fn list_field(v: &Value) -> Option<&'static str> {
    ["items", "data", "results", "transactions", "accounts", "records"].into_iter().find(|k| v[*k].is_array())
}

/// The next page of a list the host can follow: a cursor (`cursor.next`, sent back as `?cursor=`)
/// or a next-page address on the same service. `None` when the list is complete.
fn next_page(v: &Value, current: &str) -> Result<Option<String>, ReadError> {
    let mut url = url::Url::parse(current).map_err(|_| ReadError::Url)?;
    if let Some(c) = v["cursor"]["next"].as_str().filter(|c| !c.is_empty()) {
        let kept: Vec<(String, String)> = url.query_pairs().filter(|(k, _)| k != "cursor").map(|(k, v)| (k.into_owned(), v.into_owned())).collect();
        url.query_pairs_mut().clear().extend_pairs(kept).append_pair("cursor", c);
        return Ok(Some(url.into()));
    }
    for link in [&v["next"], &v["links"]["next"]] {
        if let Some(s) = link.as_str().filter(|s| !s.is_empty()) {
            let next = url.join(s).map_err(|_| ReadError::Paginated)?;
            if next.origin() != url.origin() {
                return Err(ReadError::Paginated);
            }
            return Ok(Some(next.into()));
        }
    }
    if v["offset"].is_string() || v["has_more"] == true || v["next_cursor"].is_string() || v["nextPageToken"].is_string() {
        return Err(ReadError::Paginated);
    }
    Ok(None)
}

pub(crate) fn read_tool(base: &str, known: &str) -> ConnectorTool {
    let under = if base.is_empty() { String::new() } else { format!(" Paths are on this service under {base}: \"/accounts\" reads {base}/accounts.") };
    ConnectorTool {
        name: "read".into(),
        description: format!("Read one path on this API's service with a single GET, using the saved keys. Pass {{\"path\": \"/the/path\"}} (starts with /, no query string, same service only); with no path it reads the connection's default address.{under} Lists that come in pages (a cursor.next value or a next-page link) are read in full, up to 10 pages, and joined into one list. No URL, method or header overrides.{known}"),
        input_schema: json!({"type":"object","properties":{"path":{"type":"string","pattern":"^/[^?#]*$","maxLength":300,"description":"Path on this service, for example /accounts"}},"additionalProperties":false}),
        read_only: Some(true), destructive: Some(false),
    }
}

impl AppCore {
    fn saved_apis(&self) -> CoreResult<Vec<SavedApi>> {
        serde_json::from_value(self.db().setting(APIS)?.unwrap_or(json!([])))
            .map_err(|_| CoreError::Invalid("Saved API connection settings are invalid".into()))
    }
    /// The settings of a saved API (never its keys), so the setup form can be reopened for editing.
    pub fn api_config(&self, id: &str) -> CoreResult<ApiConfig> {
        self.saved_apis()?
            .into_iter()
            .find(|a| a.id == id)
            .map(|a| a.config)
            .ok_or(CoreError::NotFound("API connection"))
    }
    /// A copy of a saved key for an edit that leaves it blank: the main key (`None`) or extra key `n`.
    pub fn api_kept_secret(&self, id: &str, extra: Option<usize>) -> CoreResult<Secret> {
        let reference = match extra {
            None => Self::api_secret(id)?,
            Some(n) => Self::api_extra_secret(id, n)?,
        };
        self.inner
            .cfg
            .vault
            .get(&reference)
            .map_err(|_| CoreError::Vault("API credential unavailable".into()))?
            .map(|s| Secret::new(s.expose().to_vec()))
            .ok_or_else(|| CoreError::Invalid("The saved key is unavailable; enter it again".into()))
    }
    pub(crate) fn api_secret(id: &str) -> CoreResult<SecretRef> {
        SecretRef::new(format!("api-credential:{id}"))
            .map_err(|_| CoreError::Invalid("Invalid API connection".into()))
    }
    pub(crate) fn api_extra_secret(id: &str, n: usize) -> CoreResult<SecretRef> {
        SecretRef::new(format!("api-credential:{id}:{n}"))
            .map_err(|_| CoreError::Invalid("Invalid API connection".into()))
    }
    pub fn validate_api_config(&self, config: &ApiConfig) -> CoreResult<()> {
        let invalid = |s: &str| CoreError::Invalid(s.into());
        if config.name.trim().is_empty()
            || config.name.chars().count() > 80
            || config.name.chars().any(char::is_control)
        {
            return Err(invalid("Name this API connection (up to 80 characters)"));
        }
        rest::validate_url(&config.url, self.allow_insecure_web())
            .map_err(|e| invalid(&e.to_string()))?;
        let in_query = config.auth == ApiAuth::Query;
        let check_name = |name: &str| {
            if in_query { rest::validate_param(name) } else { rest::validate_header(name) }
        };
        if config.auth == ApiAuth::Header || in_query {
            check_name(&config.header_name).map_err(|e| invalid(&e.to_string()))?;
        }
        if in_query && url::Url::parse(&config.url).is_ok_and(|u| u.query().is_some()) {
            return Err(invalid("Leave the key out of the API URL; it is added from the secure box"));
        }
        if config.extra_header_names.len() > MAX_EXTRA_HEADERS
            || (!config.extra_header_names.is_empty() && config.auth == ApiAuth::None)
        {
            return Err(invalid("This API needs a credential before it can use extra key headers"));
        }
        let mut seen = std::collections::HashSet::new();
        if config.auth == ApiAuth::Bearer {
            seen.insert("authorization".to_string());
        } else if config.auth == ApiAuth::Header || in_query {
            seen.insert(config.header_name.to_ascii_lowercase());
        }
        for name in &config.extra_header_names {
            check_name(name).map_err(|e| invalid(&e.to_string()))?;
            if !seen.insert(name.to_ascii_lowercase()) {
                return Err(invalid("Each key header can only be used once"));
            }
        }
        if let Some(url) = &config.docs_url {
            rest::validate_url(url, self.allow_insecure_web())
                .map_err(|e| invalid(&e.to_string()))?;
        }
        Ok(())
    }
    /// Saves locally only; a separate explicit test issues the first GET.
    /// Configurations are immutable. A replacement gets a new id and needs new grants.
    pub async fn save_api_connection(
        &self,
        config: ApiConfig,
        credential: Option<Secret>,
    ) -> CoreResult<ConnectorInfo> {
        self.save_api_connection_with_extras(config, credential, Vec::new())
            .await
    }
    /// `extras` holds one value per `extra_header_names` entry, in the same order.
    pub async fn save_api_connection_with_extras(
        &self,
        mut config: ApiConfig,
        credential: Option<Secret>,
        extras: Vec<Secret>,
    ) -> CoreResult<ConnectorInfo> {
        self.validate_api_config(&config)?;
        let unusable = |s: &Secret| {
            s.expose().is_empty()
                || s.expose().len() > 8192
                || s.expose().iter().any(|b| *b == b'\r' || *b == b'\n')
        };
        if extras.len() != config.extra_header_names.len() || extras.iter().any(unusable) {
            return Err(CoreError::Invalid(
                "Enter every API credential without line breaks".into(),
            ));
        }
        if config.auth != ApiAuth::None && credential.as_ref().is_none_or(unusable) {
            return Err(CoreError::Invalid(
                "Enter an API credential without line breaks".into(),
            ));
        }
        let _io = self.inner.api_io.lock().await;
        let _local = self.admit_local()?;
        let mut apis = self.saved_apis()?;
        if apis.len() >= 20 {
            return Err(CoreError::Invalid(
                "Remove an unused API connection before adding another".into(),
            ));
        }
        config.name = config.name.trim().to_string();
        let id = new_id("api");
        let reference = Self::api_secret(&id)?;
        if config.auth != ApiAuth::None {
            self.inner
                .cfg
                .vault
                .set(&reference, credential.unwrap())
                .map_err(|_| CoreError::Vault("API credential could not be saved".into()))?;
        }
        let forget = |upto: usize| {
            let _ = self.inner.cfg.vault.delete(&reference);
            for n in 0..upto {
                if let Ok(r) = Self::api_extra_secret(&id, n) {
                    let _ = self.inner.cfg.vault.delete(&r);
                }
            }
        };
        for (n, secret) in extras.into_iter().enumerate() {
            let saved = Self::api_extra_secret(&id, n)
                .map_err(|e| e.to_string())
                .and_then(|r| self.inner.cfg.vault.set(&r, secret).map_err(|e| e.to_string()));
            if saved.is_err() {
                forget(n);
                return Err(CoreError::Vault("API credential could not be saved".into()));
            }
        }
        let extra_count = config.extra_header_names.len();
        apis.push(SavedApi {
            id: id.clone(),
            config,
            state: ConnectionState::NotConnected,
        });
        if let Err(e) = self.db().set_setting(APIS, &json!(apis)) {
            forget(extra_count);
            return Err(e.into());
        }
        self.api_info(&id)
    }
    pub(crate) fn api_infos(&self) -> CoreResult<Vec<ConnectorInfo>> {
        Ok(self.saved_apis()?.into_iter().map(|a| ConnectorInfo {
            service: a.id.clone(), id: a.id, name: a.config.name, category: "APIs".into(),
            mcp_url: a.config.url, registration: "api".into(), verification: "unverified".into(),
            docs_url: a.config.docs_url, state: a.state, has_client_credentials: false,
            client_setup: Some("One approved GET read. Your credential stays in the host vault. Test access before selecting this connection.".into()),
            redirect_uri: None, mail: None,
        }).collect())
    }
    pub(crate) fn api_info(&self, id: &str) -> CoreResult<ConnectorInfo> {
        self.api_infos()?
            .into_iter()
            .find(|a| a.id == id)
            .ok_or(CoreError::NotFound("API connection"))
    }
    /// Checks one approved read and returns the exact address to read: the connection's own address, or
    /// `path` on the same service (never another host, a query string or a fragment).
    pub fn validate_api_read(
        &self,
        id: &str,
        resource: &str,
        tool: &str,
        args: &Value,
    ) -> CoreResult<String> {
        let changed = || CoreError::Invalid("API access changed; reconnect and rebuild this tool".into());
        let a = self.api_info(id)?;
        let object = args.as_object().ok_or_else(changed)?;
        if a.mcp_url != resource || !matches!(a.state, ConnectionState::Connected { .. }) || tool != "read" || object.keys().any(|k| k != "path") {
            return Err(changed());
        }
        match object.get("path") {
            None => Ok(a.mcp_url),
            Some(path) => {
                let path = path.as_str().ok_or_else(changed)?;
                if !path.starts_with('/') || path.starts_with("//") || path.contains(['?', '#', '{', '}', '\\']) || path.len() > 300 {
                    return Err(CoreError::Invalid("A path starts with / and has no query string".into()));
                }
                let mut url = url::Url::parse(&a.mcp_url).map_err(|_| changed())?;
                let base = api_base_path(&a.mcp_url);
                if base.is_empty() || path == base || path.starts_with(&format!("{base}/")) {
                    url.set_path(path);
                } else {
                    // "/accounts" on an API at /v1/me means /v1/accounts: services put every path under their version.
                    url.set_path(&format!("{base}{path}"));
                }
                url.set_query(None);
                Ok(url.to_string())
            }
        }
    }
    pub(crate) fn api_origin_key(url: &str) -> CoreResult<String> {
        let u = url::Url::parse(url).map_err(|_| CoreError::Invalid("Invalid API URL".into()))?;
        Ok(format!(
            "api-circuit:{}",
            hash_hex(u.origin().ascii_serialization().as_bytes())
        ))
    }
    pub(crate) async fn api_read(
        &self,
        id: &str,
        resource: &str,
        testing: bool,
    ) -> CoreResult<String> {
        self.api_read_at(id, resource, testing, false).await
    }
    /// `example` reads another address on the saved API's own service with its keys, and
    /// leaves the connection's state alone: a failing example must not break a working API.
    pub(crate) async fn api_read_at(
        &self,
        id: &str,
        resource: &str,
        testing: bool,
        example: bool,
    ) -> CoreResult<String> {
        let _io = self.inner.api_io.lock().await;
        self.ensure_active()?;
        if let Some(reason) = self.web_fetch_blocked() {
            return Err(CoreError::Invalid(reason));
        }
        let mut apis = self.saved_apis()?;
        let index = apis
            .iter()
            .position(|a| a.id == id)
            .ok_or(CoreError::NotFound("API connection"))?;
        let a = &apis[index];
        if (!example && a.config.url != resource)
            || (!testing && !matches!(a.state, ConnectionState::Connected { .. }))
        {
            return Err(CoreError::Invalid(
                "Reconnect the API and rebuild this tool".into(),
            ));
        }
        if example
            && url::Url::parse(resource).map(|u| u.origin()) != url::Url::parse(&a.config.url).map(|u| u.origin())
        {
            return Err(CoreError::Invalid("An example can only read the API's own service".into()));
        }
        let circuit = Self::api_origin_key(resource)?;
        if let Some(v) = self.db().setting(&circuit)? {
            if v["retryAtMs"].as_i64().is_some_and(|at| at > self.now()) {
                return Err(CoreError::Invalid("This service is rate limited. Wait until its retry time before testing or running any connection to it.".into()));
            }
            if !testing && v["blocked"].as_bool() == Some(true) {
                return Err(CoreError::Invalid("This API refused access. Check its permissions and test the connection before running again.".into()));
            }
        }
        let token = self
            .inner
            .cfg
            .vault
            .get(&Self::api_secret(id)?)
            .map_err(|_| CoreError::Vault("API credential unavailable".into()))?;
        if a.config.auth != ApiAuth::None && token.is_none() {
            return Err(CoreError::Invalid(
                "Reconnect this API; its credential is unavailable".into(),
            ));
        }
        let mut extra_values = Vec::new();
        for n in 0..a.config.extra_header_names.len() {
            let value = self
                .inner
                .cfg
                .vault
                .get(&Self::api_extra_secret(id, n)?)
                .map_err(|_| CoreError::Vault("API credential unavailable".into()))?
                .ok_or_else(|| {
                    CoreError::Invalid("Reconnect this API; its credential is unavailable".into())
                })?;
            extra_values.push(value);
        }
        let mut bytes = Vec::new();
        let header = match a.config.auth {
            ApiAuth::None => None,
            ApiAuth::Bearer => {
                bytes.extend_from_slice(b"Bearer ");
                bytes.extend_from_slice(token.as_ref().unwrap().expose());
                Some("authorization")
            }
            ApiAuth::Header => {
                bytes.extend_from_slice(token.as_ref().unwrap().expose());
                Some(a.config.header_name.as_str())
            }
            ApiAuth::Query => None,
        };
        let mut headers: Vec<(&str, &[u8])> = Vec::new();
        // Query keys are added to the address only for this one request; the saved
        // address, the activity log and the receipts never contain them.
        let mut query: Vec<(&str, &[u8])> = Vec::new();
        if a.config.auth == ApiAuth::Query {
            query.push((a.config.header_name.as_str(), token.as_ref().unwrap().expose()));
            for (name, value) in a.config.extra_header_names.iter().zip(&extra_values) {
                query.push((name.as_str(), value.expose()));
            }
        } else {
            if let Some(h) = header {
                headers.push((h, bytes.as_slice()));
            }
            for (name, value) in a.config.extra_header_names.iter().zip(&extra_values) {
                headers.push((name.as_str(), value.expose()));
            }
        }
        // A list that comes in pages is read page by page (at most 10) and joined, so a program
        // never sees a partial list; paging the host cannot follow fails instead.
        let mut page_url = resource.to_string();
        let mut merged: Option<Value> = None;
        let mut pages = 0;
        let text: Result<String, ReadError> = loop {
            self.db().insert_outbound(&db::OutboundRow {
                id: new_id("api_read"),
                at_ms: self.now(),
                purpose: if testing { "API connection test" } else { "API read" }.into(),
                destination: web::host_of_url(&page_url),
                route: "api_connection".into(),
                synthetic_only: false,
                cost_micros: None,
                payload_text: Some(json!({"connectionId":id,"method":"GET","page":pages + 1}).to_string()),
            })?;
            let admission = self
                .inner
                .policy
                .admit(macroscope_policy::Job { epoch: self.inner.policy.epoch() })
                .map_err(|_| CoreError::Invalid("Local only or Emergency stop blocks API reads".into()))?;
            let epoch = admission.epoch;
            let guard = HttpEpochGuard(Some(admission.guard));
            let result = tokio::select! { biased;
                _ = self.inner.policy.revoked(epoch) => Err(ReadError::Transport),
                r = rest::get_with_keys(&page_url, &headers, &query, self.allow_insecure_web(), 4 * 1024 * 1024, std::time::Duration::from_secs(10)) => r,
            };
            drop(guard);
            let page = result.and_then(|text| {
                if token.iter().chain(extra_values.iter()).any(|t| {
                    !t.expose().is_empty()
                        && text.as_bytes().windows(t.expose().len()).any(|w| w == t.expose())
                }) {
                    return Err(ReadError::InvalidResponse);
                }
                serde_json::from_str::<Value>(&text).map_err(|_| ReadError::InvalidResponse)
            });
            let value = match page { Ok(v) => v, Err(e) => break Err(e) };
            let next = match next_page(&value, &page_url) { Ok(n) => n, Err(e) => break Err(e) };
            match merged.as_mut() {
                None => merged = Some(value),
                Some(all) => {
                    let Some(field) = list_field(all) else { break Err(ReadError::Paginated) };
                    let (Some(list), Some(more)) = (all[field].as_array_mut(), value[field].as_array()) else { break Err(ReadError::Paginated) };
                    list.extend(more.iter().cloned());
                }
            }
            pages += 1;
            let Some(next) = next else {
                let mut all = merged.take().unwrap_or(Value::Null);
                if pages > 1 {
                    for k in ["cursor", "next", "links"] {
                        if let Some(o) = all.as_object_mut() { o.remove(k); }
                    }
                }
                let text = all.to_string();
                break if text.len() > 4 * 1024 * 1024 { Err(ReadError::TooLarge) } else { Ok(text) };
            };
            if pages >= 10 || merged.as_ref().is_none_or(|m| list_field(m).is_none()) {
                break Err(ReadError::TooLarge);
            }
            page_url = next;
        };
        let _local = self.admit_local()?;
        if example && testing && text.is_ok() && !matches!(apis[index].state, ConnectionState::Connected { .. }) {
            // The keys were accepted by the service at a tested path, so the connection is usable
            // even if its default address was wrong.
            apis[index].state = ConnectionState::Connected { since_ms: self.now(), tool_count: 1 };
        }
        if !example { match &text {
            Ok(_) => {
                apis[index].state = ConnectionState::Connected {
                    since_ms: self.now(),
                    tool_count: 1,
                };
                if testing {
                    self.db().set_setting(&circuit, &Value::Null)?;
                }
            }
            Err(e) => {
                apis[index].state = match e { ReadError::Unauthorized => ConnectionState::NeedsReauth,
                    _ => ConnectionState::Error { message: format!("{e}.") } };
                match e {
                    ReadError::RateLimited { retry_at_ms } => self
                        .db()
                        .set_setting(&circuit, &json!({"retryAtMs":retry_at_ms}))?,
                    ReadError::Unauthorized | ReadError::Forbidden => {
                        self.db().set_setting(&circuit, &json!({"blocked":true}))?
                    }
                    _ => {}
                }
            }
        } }
        self.db().set_setting(APIS, &json!(apis))?;
        if let Some(blocked) = self.web_fetch_blocked() {
            return Err(CoreError::Invalid(blocked));
        }
        text.map_err(|e| CoreError::Invalid(format!("{e}. Nothing ran.")))
    }
    pub async fn test_api_connection(&self, id: &str) -> CoreResult<ConnectorInfo> {
        let info = self.api_info(id)?;
        self.api_read(id, &info.mcp_url, true).await?;
        self.api_info(id)
    }
    pub async fn disconnect_api(&self, id: &str) -> CoreResult<ConnectorInfo> {
        let _io = self.inner.api_io.lock().await;
        let _local = self.admit_local()?;
        let mut apis = self.saved_apis()?;
        let index = apis
            .iter()
            .position(|a| a.id == id)
            .ok_or(CoreError::NotFound("API connection"))?;
        self.inner
            .cfg
            .vault
            .delete(&Self::api_secret(id)?)
            .map_err(|_| CoreError::Vault("API credential could not be removed".into()))?;
        for n in 0..apis[index].config.extra_header_names.len() {
            self.inner
                .cfg
                .vault
                .delete(&Self::api_extra_secret(id, n)?)
                .map_err(|_| CoreError::Vault("API credential could not be removed".into()))?;
        }
        let mut info = self.api_info(id)?;
        info.state = ConnectionState::NotConnected;
        apis.remove(index);
        self.db().set_setting(APIS, &json!(apis))?;
        self.db().set_setting(&format!("api-examples:{id}"), &Value::Null)?;
        Ok(info)
    }
}
