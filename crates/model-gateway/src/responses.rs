use crate::GatewayError;
use macroscope_vault::Secret;
use reqwest::{
    header::{HeaderValue, AUTHORIZATION, RETRY_AFTER},
    StatusCode,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::{Duration, SystemTime};

pub const DEFAULT_BASE_URL: &str = "https://api.meta.ai/v1";
pub const DEFAULT_MODEL: &str = "muse-spark-1.3";
const MAX_RESPONSE: usize = 1024 * 1024;
#[derive(Debug, Clone)]
pub struct MetaResponsesClient {
    pub base_url: String,
    pub http: reqwest::Client,
}
impl Default for MetaResponsesClient {
    fn default() -> Self {
        Self::new(DEFAULT_BASE_URL)
    }
}
impl MetaResponsesClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            http: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .no_proxy()
                .timeout(Duration::from_secs(120))
                .build()
                .expect("rustls client"),
        }
    }
    pub async fn generate_structured(
        &self,
        req: StructuredRequest<'_>,
    ) -> Result<StructuredOutput, GatewayError> {
        let validator =
            jsonschema::validator_for(req.schema).map_err(|_| GatewayError::MalformedOutput)?;
        let body = json!({"model":req.model.unwrap_or(DEFAULT_MODEL), "input":[
            {"role":"system", "content":req.system}, {"role":"user", "content":req.user}],
            "store":false, "max_output_tokens":req.max_output_tokens, "reasoning":{"effort":req.reasoning_effort},
            "text":{"format":{"type":"json_schema", "name":req.schema_name, "schema":req.schema, "strict":true}}});
        let response = self
            .http
            .post(format!("{}/responses", self.base_url.trim_end_matches('/')))
            .header(AUTHORIZATION, bearer(req.key)?)
            .json(&body)
            .send()
            .await
            .map_err(|_| GatewayError::Transient)?;
        classify_status(response.status(), response.headers().get(RETRY_AFTER))?;
        let value = bounded_json(response).await?;
        if value["status"] == "incomplete"
            || value
                .get("incomplete_details")
                .is_some_and(|v| !v.is_null())
        {
            return Err(GatewayError::Truncated);
        }
        let mut text = String::new();
        for item in value["output"]
            .as_array()
            .ok_or(GatewayError::MalformedOutput)?
        {
            if item["type"] == "refusal" {
                return Err(GatewayError::Refused);
            }
            if let Some(parts) = item["content"].as_array() {
                for part in parts {
                    if part["type"] == "refusal" {
                        return Err(GatewayError::Refused);
                    }
                    if part["type"] == "output_text" {
                        text.push_str(part["text"].as_str().ok_or(GatewayError::MalformedOutput)?);
                    }
                }
            }
        }
        if !req.key.expose().is_empty()
            && text
                .as_bytes()
                .windows(req.key.expose().len())
                .any(|w| w == req.key.expose())
        {
            return Err(GatewayError::MalformedOutput);
        }
        let output: Value =
            serde_json::from_str(&text).map_err(|_| GatewayError::MalformedOutput)?;
        if !validator.is_valid(&output) {
            return Err(GatewayError::MalformedOutput);
        }
        Ok(StructuredOutput {
            value: output,
            usage: Usage::from_response(&value["usage"]),
            usage_known: value["usage"]["input_tokens"].is_u64()
                && value["usage"]["output_tokens"].is_u64(),
        })
    }
    pub async fn catalog_check(&self, key: &Secret) -> Result<Catalog, GatewayError> {
        let response = self
            .http
            .get(format!("{}/models", self.base_url.trim_end_matches('/')))
            .header(AUTHORIZATION, bearer(key)?)
            .send()
            .await
            .map_err(|_| GatewayError::Transient)?;
        classify_status(response.status(), response.headers().get(RETRY_AFTER))?;
        let value = bounded_json(response).await?;
        let models = value["data"]
            .as_array()
            .ok_or(GatewayError::MalformedOutput)?
            .iter()
            .map(|v| {
                v["id"]
                    .as_str()
                    .map(String::from)
                    .ok_or(GatewayError::MalformedOutput)
            })
            .collect::<Result<_, _>>()?;
        let catalog = Catalog { models };
        if catalog.models.iter().any(|s| {
            !key.expose().is_empty()
                && s.as_bytes()
                    .windows(key.expose().len())
                    .any(|w| w == key.expose())
        }) {
            return Err(GatewayError::MalformedOutput);
        }
        Ok(catalog)
    }
}
// Only static error summaries leave this module. Remote bodies, URL errors,
// schema diagnostics and header parsing errors can echo secrets and are discarded.
pub(crate) fn bearer(key: &Secret) -> Result<HeaderValue, GatewayError> {
    let mut bytes = zeroize::Zeroizing::new(b"Bearer ".to_vec());
    bytes.extend_from_slice(key.expose());
    let mut header = HeaderValue::from_bytes(&bytes).map_err(|_| GatewayError::AuthRejected)?;
    header.set_sensitive(true);
    Ok(header)
}
pub(crate) async fn bounded_json(response: reqwest::Response) -> Result<Value, GatewayError> {
    bounded_json_count(response).await.map(|(value, _)| value)
}
pub(crate) async fn bounded_json_count(
    mut response: reqwest::Response,
) -> Result<(Value, usize), GatewayError> {
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| GatewayError::Transient)?
    {
        if chunk.len() > MAX_RESPONSE - bytes.len() {
            return Err(GatewayError::PayloadTooLarge);
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes)
        .map(|v| (v, bytes.len()))
        .map_err(|_| GatewayError::MalformedOutput)
}
pub(crate) fn classify_status(
    status: StatusCode,
    retry: Option<&HeaderValue>,
) -> Result<(), GatewayError> {
    if status.is_success() {
        return Ok(());
    }
    Err(match status.as_u16() {
        401 | 403 => GatewayError::AuthRejected,
        402 => GatewayError::BillingRequired,
        429 => GatewayError::Quota {
            retry_after: retry.and_then(|v| v.to_str().ok()).and_then(|s| {
                s.parse::<u64>().map(Duration::from_secs).ok().or_else(|| {
                    httpdate::parse_http_date(s)
                        .ok()
                        .map(|d| d.duration_since(SystemTime::now()).unwrap_or_default())
                })
            }),
        },
        413 => GatewayError::PayloadTooLarge,
        500..=599 => GatewayError::Transient,
        _ => GatewayError::Rejected,
    })
}
#[derive(Debug)]
pub struct StructuredRequest<'a> {
    pub key: &'a Secret,
    pub model: Option<&'a str>,
    pub system: &'a str,
    pub user: &'a str,
    pub max_output_tokens: u32,
    pub reasoning_effort: &'a str,
    pub schema_name: &'a str,
    pub schema: &'a Value,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u64,
    pub cached_tokens: u64,
    pub output_tokens: u64,
    pub reasoning_tokens: u64,
}
impl Usage {
    pub fn from_response(v: &Value) -> Self {
        Self {
            input_tokens: v["input_tokens"].as_u64().unwrap_or(0),
            output_tokens: v["output_tokens"].as_u64().unwrap_or(0),
            cached_tokens: v["input_tokens_details"]["cached_tokens"]
                .as_u64()
                .or_else(|| v["cached_tokens"].as_u64())
                .unwrap_or(0),
            reasoning_tokens: v["output_tokens_details"]["reasoning_tokens"]
                .as_u64()
                .unwrap_or(0),
        }
    }
}
#[derive(Debug, Clone)]
pub struct StructuredOutput {
    pub value: Value,
    pub usage: Usage,
    pub usage_known: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Catalog {
    pub models: Vec<String>,
}

pub const PRICE_TABLE_VERSION: &str = "meta-standard-2026-10-27";
#[derive(Debug, Clone, Copy)]
pub struct CostEstimator {
    /// Responses output_tokens includes reasoning by default. Set false only
    /// for providers whose output counter explicitly excludes reasoning.
    pub reasoning_in_output: bool,
}
impl Default for CostEstimator {
    fn default() -> Self {
        Self {
            reasoning_in_output: true,
        }
    }
}
impl CostEstimator {
    pub fn estimate(&self, model: &str, usage: &Usage) -> Result<u64, GatewayError> {
        // input_tokens includes the cached subset. Charge uncached input and
        // cached input separately. Rates are hundredths of a microdollar per
        // token; round the total UP.
        let (input, cached, output): (u128, u128, u128) = match model {
            "muse-spark-1.3" => (125, 15, 425),
            "muse-wonder-1.0" => (400, 20, 2000),
            _ => return Err(GatewayError::UnsupportedModel),
        };
        if usage.cached_tokens > usage.input_tokens {
            return Err(GatewayError::MalformedOutput);
        }
        let out = u128::from(usage.output_tokens)
            + if self.reasoning_in_output {
                0
            } else {
                u128::from(usage.reasoning_tokens)
            };
        let total = (u128::from(usage.input_tokens - usage.cached_tokens) * input
            + u128::from(usage.cached_tokens) * cached
            + out * output)
            .div_ceil(100);
        u64::try_from(total).map_err(|_| GatewayError::CostOverflow)
    }
}
