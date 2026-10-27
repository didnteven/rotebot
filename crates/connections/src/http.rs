use crate::mcp::McpError;
use reqwest::{Client, Response};
use serde::de::DeserializeOwned;
use std::time::Duration;
use url::Url;

pub const RESPONSE_LIMIT: usize = 1024 * 1024;

pub fn validate_url(value: &str) -> Result<Url, McpError> {
    let url = Url::parse(value).map_err(|_| McpError::InvalidUrl)?;
    let local = matches!(url.host_str(), Some("127.0.0.1" | "localhost"));
    if (url.scheme() != "https" && !(url.scheme() == "http" && local))
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(McpError::InvalidUrl);
    }
    Ok(url)
}
pub fn client() -> Result<Client, McpError> {
    Client::builder()
        .timeout(Duration::from_secs(30))
        // Never forward credentials or silently downgrade through a redirect.
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| McpError::Transport)
}
pub fn check_size(response: &Response) -> Result<(), McpError> {
    check_size_limited(response, RESPONSE_LIMIT)
}
pub fn check_size_limited(response: &Response, limit: usize) -> Result<(), McpError> {
    if response.content_length().is_some_and(|n| n > limit as u64) {
        return Err(McpError::ResponseTooLarge);
    }
    Ok(())
}
pub async fn bytes(response: Response) -> Result<Vec<u8>, McpError> {
    check_size(&response)?;
    bytes_limited(response, RESPONSE_LIMIT).await
}
pub async fn bytes_limited(mut response: Response, limit: usize) -> Result<Vec<u8>, McpError> {
    check_size_limited(&response, limit)?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| McpError::Transport)? {
        if bytes.len() + chunk.len() > limit {
            return Err(McpError::ResponseTooLarge);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}
pub async fn json<T: DeserializeOwned>(response: Response) -> Result<T, McpError> {
    if !response.status().is_success() {
        return Err(McpError::Http(response.status().as_u16()));
    }
    serde_json::from_slice(&bytes(response).await?).map_err(|_| McpError::InvalidResponse)
}
