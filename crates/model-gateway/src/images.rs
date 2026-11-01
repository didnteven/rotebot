//! Dedicated, one-image calls. No automatic retries: a lost response may be billed.
use crate::{bearer, classify_status, GatewayError};
use base64::{engine::general_purpose::STANDARD, Engine};
use macroscope_vault::Secret;
use serde_json::{json, Value};
use std::time::Duration;

pub const BOT_IMAGE_MODEL: &str = "muse-image-1.0";
pub const BOT_IMAGE_MICROS: i64 = 10_000;
const MAX_IMAGE_RESPONSE: usize = 12 * 1024 * 1024;

pub async fn generate_bot_png(
    base_url: &str,
    key: &Secret,
    prompt: &str,
    reference: Option<&[u8]>,
) -> Result<Vec<u8>, GatewayError> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .timeout(Duration::from_secs(180))
        .build()
        .map_err(|_| GatewayError::Unavailable)?;
    let mut body = json!({"model": BOT_IMAGE_MODEL, "prompt": prompt, "n": 1,
        "size": "1024x1024", "output_format": "png", "response_format": "b64_json"});
    let endpoint = if let Some(reference) = reference {
        body["images"] =
            json!([{"image_url": format!("data:image/png;base64,{}", STANDARD.encode(reference))}]);
        "images/edits"
    } else {
        "images/generations"
    };
    let mut response = client
        .post(format!("{}/{endpoint}", base_url.trim_end_matches('/')))
        .header(reqwest::header::AUTHORIZATION, bearer(key)?)
        .json(&body)
        .send()
        .await
        .map_err(|_| GatewayError::Transient)?;
    classify_status(
        response.status(),
        response.headers().get(reqwest::header::RETRY_AFTER),
    )?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| GatewayError::Transient)?
    {
        if chunk.len() > MAX_IMAGE_RESPONSE - bytes.len() {
            return Err(GatewayError::PayloadTooLarge);
        }
        bytes.extend_from_slice(&chunk);
    }
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| GatewayError::MalformedOutput)?;
    let images = value["data"]
        .as_array()
        .filter(|a| a.len() == 1)
        .ok_or(GatewayError::MalformedOutput)?;
    let encoded = images[0]["b64_json"]
        .as_str()
        .ok_or(GatewayError::MalformedOutput)?;
    let png = STANDARD
        .decode(encoded)
        .map_err(|_| GatewayError::MalformedOutput)?;
    if png.len() > 8 * 1024 * 1024 || !png.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err(GatewayError::MalformedOutput);
    }
    Ok(png)
}
