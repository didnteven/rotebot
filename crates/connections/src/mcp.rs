use crate::{http, oauth::AccessToken};
use reqwest::{header::HeaderValue, Client, Response};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;

/// Errors deliberately omit remote bodies, RPC messages, tokens, and request URLs.
#[derive(thiserror::Error)]
pub enum McpError {
    #[error("MCP authorization required")]
    AuthRequired {
        resource_metadata_url: Option<String>,
        scope_hint: Option<String>,
    },
    #[error("URL must use HTTPS (HTTP is allowed only for localhost or 127.0.0.1)")]
    InvalidUrl,
    #[error("connection transport failed or timed out")]
    Transport,
    #[error("response exceeds 1 MiB")]
    ResponseTooLarge,
    #[error("invalid server response")]
    InvalidResponse,
    #[error("unsupported MCP protocol version")]
    UnsupportedVersion,
    #[error("HTTP request failed ({0})")]
    Http(u16),
    #[error("server returned a JSON-RPC error")]
    Rpc,
    #[error("tool is not explicitly read-only")]
    ToolNotReadOnly,
}
impl std::fmt::Debug for McpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

/// All strings and schemas in this structure are untrusted remote data.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub input_schema: Value,
    #[serde(default)]
    pub annotations: ToolAnnotations,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolAnnotations {
    pub read_only_hint: Option<bool>,
    pub destructive_hint: Option<bool>,
}

/// List capabilities before connecting, if the server permits anonymous access.
/// Server annotations are preserved as claims, never promoted to permissions.
pub async fn preview_tools(url: &str) -> Result<Vec<Tool>, McpError> {
    McpClient::connect(url, None).await?.list_tools().await
}

pub struct McpClient {
    http: Client,
    url: String,
    auth: Option<AccessToken>,
    session: Option<HeaderValue>,
    protocol: Option<String>,
    next_id: u64,
}
impl std::fmt::Debug for McpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpClient")
            .field("protocol", &self.protocol)
            .finish_non_exhaustive()
    }
}
impl McpClient {
    fn new(url: &str, auth: Option<AccessToken>) -> Result<Self, McpError> {
        http::validate_url(url)?;
        Ok(Self {
            http: http::client()?,
            url: url.into(),
            auth,
            session: None,
            protocol: None,
            next_id: 1,
        })
    }
    /// Probe initialize only, without credentials or any authorization request.
    pub(crate) async fn probe_auth(
        url: &str,
    ) -> Result<(Option<String>, Option<String>), McpError> {
        match Self::new(url, None)?.initialize().await {
            Ok(_) => Ok((None, None)),
            Err(McpError::AuthRequired {
                resource_metadata_url,
                scope_hint,
            }) => Ok((resource_metadata_url, scope_hint)),
            Err(error) => Err(error),
        }
    }
    async fn initialize(&mut self) -> Result<Value, McpError> {
        self.request(
            "initialize",
            json!({
                "protocolVersion": "2025-06-18", "capabilities": {},
                "clientInfo": {"name": "macroscope", "version": env!("CARGO_PKG_VERSION")}
            }),
        )
        .await
    }
    pub async fn connect(url: &str, auth: Option<AccessToken>) -> Result<Self, McpError> {
        let mut client = Self::new(url, auth)?;
        let result = client.initialize().await?;
        let version = result["protocolVersion"]
            .as_str()
            .ok_or(McpError::InvalidResponse)?;
        if !matches!(version, "2025-06-18" | "2025-03-26") {
            return Err(McpError::UnsupportedVersion);
        }
        client.protocol = Some(version.into());
        let response = client
            .send(json!({"jsonrpc":"2.0", "method":"notifications/initialized"}))
            .await?;
        if !matches!(response.status().as_u16(), 200 | 202 | 204) {
            return Err(McpError::InvalidResponse);
        }
        Ok(client)
    }
    pub fn protocol_version(&self) -> &str {
        self.protocol.as_deref().unwrap_or_default()
    }
    async fn send(&mut self, body: Value) -> Result<Response, McpError> {
        let mut request = self
            .http
            .post(&self.url)
            .header("Accept", "application/json, text/event-stream")
            .json(&body);
        if let Some(auth) = &self.auth {
            request = request.bearer_auth(auth.expose());
        }
        if let Some(session) = &self.session {
            request = request.header("Mcp-Session-Id", session);
        }
        if let Some(protocol) = &self.protocol {
            request = request.header("MCP-Protocol-Version", protocol);
        }
        let response = request.send().await.map_err(|_| McpError::Transport)?;
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            let metadata = response
                .headers()
                .get_all("WWW-Authenticate")
                .iter()
                .filter_map(|v| v.to_str().ok())
                .find_map(|h| bearer_parameter(h, "resource_metadata"));
            let scope_hint = response
                .headers()
                .get_all("WWW-Authenticate")
                .iter()
                .filter_map(|v| v.to_str().ok())
                .find_map(|h| bearer_parameter(h, "scope"));
            return Err(McpError::AuthRequired {
                resource_metadata_url: metadata,
                scope_hint,
            });
        }
        if !response.status().is_success() {
            return Err(McpError::Http(response.status().as_u16()));
        }
        if let Some(session) = response.headers().get("Mcp-Session-Id") {
            if session.as_bytes().len() > 1024
                || session
                    .as_bytes()
                    .iter()
                    .any(|c| !(0x21..=0x7e).contains(c))
            {
                return Err(McpError::InvalidResponse);
            }
            self.session = Some(session.clone());
        }
        http::check_size(&response)?;
        Ok(response)
    }
    async fn request(&mut self, method: &str, params: Value) -> Result<Value, McpError> {
        let id = self.next_id;
        self.next_id += 1;
        let response = self
            .send(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))
            .await?;
        let content_type = response
            .headers()
            .get("Content-Type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let content_type = content_type.split(';').next().unwrap_or("").trim();
        let value = match content_type {
            "application/json" => http::json::<Value>(response).await?,
            "text/event-stream" => sse_response(response, id).await?,
            _ => return Err(McpError::InvalidResponse),
        };
        if value["jsonrpc"] != "2.0" || value["id"] != id {
            return Err(McpError::InvalidResponse);
        }
        if value.get("error").is_some() {
            return Err(McpError::Rpc);
        }
        value
            .get("result")
            .cloned()
            .ok_or(McpError::InvalidResponse)
    }
    pub async fn list_tools(&mut self) -> Result<Vec<Tool>, McpError> {
        let mut tools = Vec::new();
        let mut cursor = None;
        let mut seen = HashSet::new();
        // Bound aggregate work as well as each individual response.
        for _ in 0..100 {
            let params = cursor.as_ref().map_or(json!({}), |c| json!({"cursor": c}));
            let result = self.request("tools/list", params).await?;
            let page: Vec<Tool> = serde_json::from_value(result["tools"].clone())
                .map_err(|_| McpError::InvalidResponse)?;
            for mut tool in page {
                // Never silently rename a tool and then call a different wire name.
                if tool.name.is_empty() || clean(&tool.name, 256) != tool.name {
                    return Err(McpError::InvalidResponse);
                }
                tool.description = tool.description.map(|s| clean(&s, 1000));
                tools.push(tool);
            }
            if tools.len() > 10_000 {
                return Err(McpError::ResponseTooLarge);
            }
            match result.get("nextCursor") {
                None | Some(Value::Null) => return Ok(tools),
                Some(Value::String(next)) if !next.is_empty() && seen.insert(next.clone()) => {
                    cursor = Some(next.clone())
                }
                _ => return Err(McpError::InvalidResponse),
            }
        }
        Err(McpError::ResponseTooLarge)
    }
    /// Trusted host only, after the broker has durably journaled an approved call.
    pub(crate) async fn call_approved_tool(
        &mut self,
        name: &str,
        args: Value,
    ) -> Result<Value, McpError> {
        self.request("tools/call", json!({"name": name, "arguments": args}))
            .await
    }
    /// Host diagnostics only. Hints are untrusted claims, not a broker permission.
    /// The ConnectionsService verifies using initialize/list only; no generated-code API.
    #[allow(dead_code)]
    pub(crate) async fn call_tool(&mut self, name: &str, args: Value) -> Result<Value, McpError> {
        let tools = self.list_tools().await?;
        let mut matches = tools.iter().filter(|t| t.name == name);
        let safe = matches.next().is_some_and(|t| {
            t.annotations.read_only_hint == Some(true)
                && t.annotations.destructive_hint != Some(true)
        });
        if !safe || matches.next().is_some() {
            return Err(McpError::ToolNotReadOnly);
        }
        self.request("tools/call", json!({"name":name,"arguments":args}))
            .await
    }
}
fn clean(value: &str, limit: usize) -> String {
    value
        .chars()
        .filter(|c| !c.is_control())
        .take(limit)
        .collect()
}
fn bearer_parameter(header: &str, parameter: &str) -> Option<String> {
    let (scheme, params) = header.split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }
    // Parse quoted auth-params without splitting commas inside quoted URLs.
    let mut rest = params.trim();
    while !rest.is_empty() {
        let (key, value) = rest.split_once('=')?;
        let value = value.trim_start();
        let (value, tail) = if let Some(quoted) = value.strip_prefix('"') {
            let end = quoted.find('"')?;
            (&quoted[..end], &quoted[end + 1..])
        } else {
            value.split_once(',').unwrap_or((value, ""))
        };
        if key.trim().eq_ignore_ascii_case(parameter) {
            return Some(value.to_owned());
        }
        rest = tail.trim_start_matches([',', ' ']);
    }
    None
}
async fn sse_response(mut response: Response, id: u64) -> Result<Value, McpError> {
    let mut total = 0;
    let mut line = Vec::new();
    let mut data = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| McpError::Transport)? {
        total += chunk.len();
        if total > http::RESPONSE_LIMIT {
            return Err(McpError::ResponseTooLarge);
        }
        for byte in chunk {
            if byte != b'\n' {
                line.push(byte);
                continue;
            }
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            if line.is_empty() {
                if !data.is_empty() {
                    let event: Value =
                        serde_json::from_slice(&data).map_err(|_| McpError::InvalidResponse)?;
                    if event["id"] == id {
                        return Ok(event);
                    }
                    data.clear();
                }
            } else if let Some(value) = line.strip_prefix(b"data:") {
                if !data.is_empty() {
                    data.push(b'\n');
                }
                data.extend_from_slice(value.strip_prefix(b" ").unwrap_or(value));
            }
            line.clear();
        }
    }
    Err(McpError::InvalidResponse)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{routing::post, Json, Router};

    #[tokio::test]
    async fn host_diagnostics_require_explicit_read_only_and_unique_names() {
        let app = Router::new().route("/mcp", post(|Json(body): Json<Value>| async move {
            let result = match body["method"].as_str().unwrap() {
                "initialize" => json!({"protocolVersion":"2025-06-18"}),
                "notifications/initialized" => return (reqwest::StatusCode::ACCEPTED, Json(Value::Null)),
                "tools/list" => json!({"tools":[
                    {"name":"read","inputSchema":{},"annotations":{"readOnlyHint":true}},
                    {"name":"unknown","inputSchema":{}},
                    {"name":"destructive","inputSchema":{},"annotations":{"readOnlyHint":true,"destructiveHint":true}},
                    {"name":"duplicate","inputSchema":{},"annotations":{"readOnlyHint":true}},
                    {"name":"duplicate","inputSchema":{},"annotations":{"destructiveHint":true}}
                ]}),
                "tools/call" => { assert_eq!(body["params"]["name"],"read"); json!({"content":[]}) },
                _ => panic!("unexpected request"),
            };
            (reqwest::StatusCode::OK, Json(json!({"jsonrpc":"2.0","id":body["id"],"result":result})))
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/mcp", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let mut client = McpClient::connect(&url, None).await.unwrap();
        for name in ["unknown", "destructive", "missing", "duplicate"] {
            assert!(matches!(
                client.call_tool(name, json!({})).await,
                Err(McpError::ToolNotReadOnly)
            ));
        }
        assert_eq!(
            client.call_tool("read", json!({})).await.unwrap(),
            json!({"content":[]})
        );
        server.abort();
    }
}
