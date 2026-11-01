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
        let mut messages = vec![json!({"role":"system","content":req.system}), json!({"role":"user","content":req.user})];
        let mut total = Usage::default();
        let mut usage_known = true;
        // Self-hosted models often ignore strict schemas or wrap JSON in prose; one corrective
        // round with the exact problem fixes most of these.
        for attempt in 0..2 {
            let (text, usage, known) = self.complete(&req, &messages).await?;
            total.input_tokens += usage.input_tokens;
            total.output_tokens += usage.output_tokens;
            total.reasoning_tokens += usage.reasoning_tokens;
            usage_known &= known;
            if !req.key.expose().is_empty()
                && text.as_bytes().windows(req.key.expose().len()).any(|w| w == req.key.expose())
            {
                return Err(GatewayError::MalformedOutput);
            }
            let problem = match extract_json(&text) {
                None => "The answer was not a JSON object.".to_string(),
                Some(output) => {
                    if contains_secret(&output, req.key.expose()) {
                        return Err(GatewayError::MalformedOutput);
                    }
                    let first = validator.iter_errors(&output).next().map(|e| format!("It does not match the schema: {} (at {}).", e, e.instance_path()));
                    match first {
                        None => return Ok(StructuredOutput { value: output, usage: total, usage_known }),
                        Some(problem) => problem,
                    }
                }
            };
            if attempt == 0 {
                messages.push(json!({"role":"assistant","content":text.chars().take(8000).collect::<String>()}));
                messages.push(json!({"role":"user","content":format!("{problem} Return only one JSON object that matches the requested schema exactly: every required field, no extra fields, no prose and no code fences.")}));
            }
        }
        Err(GatewayError::MalformedOutput)
    }

    /// One chat completion: the answer text, its usage, and whether the usage was reported.
    async fn complete(&self, req: &StructuredRequest<'_>, messages: &[Value]) -> Result<(String, Usage, bool), GatewayError> {
        // Reasoning models spend part of max_tokens thinking; leave room for the answer itself.
        let body = json!({"model":self.model,"messages":messages,
            "response_format":{"type":"json_schema","json_schema":{"name":req.schema_name,"strict":true,"schema":req.schema}},
            "temperature":0.2,"max_tokens":req.max_output_tokens.max(8000),"reasoning_effort":"medium"});
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
            return Err(GatewayError::Truncated);
        }
        let text = choice["message"]["content"]
            .as_str()
            .ok_or(GatewayError::MalformedOutput)?
            .to_string();
        let known = value["usage"]["prompt_tokens"].is_u64() && value["usage"]["completion_tokens"].is_u64();
        Ok((text, usage, known))
    }
}

/// The JSON object in a model's answer, also when it is wrapped in a code fence or a sentence.
fn extract_json(text: &str) -> Option<Value> {
    if let Ok(v @ Value::Object(_)) = serde_json::from_str::<Value>(text.trim()) {
        return Some(v);
    }
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    serde_json::from_str::<Value>(text.get(start..=end)?).ok().filter(Value::is_object)
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
