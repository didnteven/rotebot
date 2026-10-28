//! OpenAI-compatible structured generation. Remote diagnostics never cross this boundary.
use crate::{
    responses::{bearer, bounded_json_count, classify_status},
    GatewayError, MetaResponsesClient, StructuredOutput, StructuredRequest, Usage,
};
use reqwest::{
    header::{AUTHORIZATION, RETRY_AFTER},
    Url,
};
use serde_json::{json, Value};
use std::sync::Arc;

/// Host-owned durable call admission and metadata-only outbound accounting.
pub trait CustomCallObserver: Send + Sync {
    fn dispatch(&self, request_bytes: usize) -> Result<String, GatewayError>;
    fn completed(&self, id: &str, response_bytes: usize, usage: &Usage)
        -> Result<(), GatewayError>;
}

pub fn validate_custom_url(raw: &str, allow_insecure: bool) -> Result<Url, GatewayError> {
    let url = Url::parse(raw).map_err(|_| GatewayError::Rejected)?;
    let loopback = allow_insecure && url.scheme() == "http" && url.host_str() == Some("127.0.0.1");
    if (url.scheme() != "https" && !loopback)
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || raw
            .split("://")
            .nth(1)
            .is_some_and(|s| s.split('/').next().unwrap_or("").contains('@'))
    {
        return Err(GatewayError::Rejected);
    }
    Ok(url)
}

#[derive(Clone)]
pub struct CustomEndpointClient {
    base_url: String,
    model: String,
    http: reqwest::Client,
    observer: Option<Arc<dyn CustomCallObserver>>,
}
impl CustomEndpointClient {
    pub fn new(base_url: &str, model: &str, allow_insecure: bool) -> Result<Self, GatewayError> {
        let url = validate_custom_url(base_url, allow_insecure)?;
        if model.trim().is_empty() || model.len() > 256 || model.chars().any(char::is_control) {
            return Err(GatewayError::Rejected);
        }
        Ok(Self {
            base_url: url.to_string(),
            model: model.into(),
            http: MetaResponsesClient::default().http,
            observer: None,
        })
    }
    pub fn with_observer(mut self, observer: Arc<dyn CustomCallObserver>) -> Self {
        self.observer = Some(observer);
        self
    }
    pub async fn generate_structured(
        &self,
        req: StructuredRequest<'_>,
    ) -> Result<StructuredOutput, GatewayError> {
        let validator =
            jsonschema::validator_for(req.schema).map_err(|_| GatewayError::MalformedOutput)?;
        let body = json!({"model":self.model,"messages":[{"role":"system","content":req.system},{"role":"user","content":req.user}],
            "response_format":{"type":"json_schema","json_schema":{"name":req.schema_name,"strict":true,"schema":req.schema}},
            "temperature":0.2,"max_tokens":req.max_output_tokens,"reasoning_effort":"medium"});
        let bytes = serde_json::to_vec(&body).map_err(|_| GatewayError::Rejected)?;
        let header = bearer(req.key)?;
        let id = self
            .observer
            .as_ref()
            .map(|o| o.dispatch(bytes.len()))
            .transpose()?;
        let response = self
            .http
            .post(format!(
                "{}/chat/completions",
                self.base_url.trim_end_matches('/')
            ))
            .header(AUTHORIZATION, header)
            .header("content-type", "application/json")
            .body(bytes)
            .send()
            .await
            .map_err(|_| GatewayError::Transient)?;
        classify_status(response.status(), response.headers().get(RETRY_AFTER))?;
        let (value, response_bytes) = bounded_json_count(response).await?;
        let usage = Usage {
            input_tokens: value["usage"]["prompt_tokens"].as_u64().unwrap_or(0),
            output_tokens: value["usage"]["completion_tokens"].as_u64().unwrap_or(0),
            reasoning_tokens: value["usage"]["reasoning_tokens"].as_u64().unwrap_or(0),
            ..Usage::default()
        };
        if let (Some(observer), Some(id)) = (&self.observer, id) {
            observer.completed(&id, response_bytes, &usage)?;
        }
        let choice = &value["choices"][0];
        if choice["finish_reason"] == "length" {
            return Err(GatewayError::MalformedOutput);
        }
        let text = choice["message"]["content"]
            .as_str()
            .ok_or(GatewayError::MalformedOutput)?;
        if !req.key.expose().is_empty()
            && text
                .as_bytes()
                .windows(req.key.expose().len())
                .any(|w| w == req.key.expose())
        {
            return Err(GatewayError::MalformedOutput);
        }
        let output: Value =
            serde_json::from_str(text).map_err(|_| GatewayError::MalformedOutput)?;
        if contains_secret(&output, req.key.expose()) || !validator.is_valid(&output) {
            return Err(GatewayError::MalformedOutput);
        }
        Ok(StructuredOutput {
            value: output,
            usage,
            usage_known: value["usage"]["prompt_tokens"].is_u64()
                && value["usage"]["completion_tokens"].is_u64(),
        })
    }
}

fn contains_secret(value: &Value, secret: &[u8]) -> bool {
    fn matches(s: &str, secret: &[u8]) -> bool {
        !secret.is_empty() && s.as_bytes().windows(secret.len()).any(|w| w == secret)
    }
    match value {
        Value::String(s) => matches(s, secret),
        Value::Array(items) => items.iter().any(|v| contains_secret(v, secret)),
        Value::Object(fields) => fields
            .iter()
            .any(|(k, v)| matches(k, secret) || contains_secret(v, secret)),
        _ => false,
    }
}
