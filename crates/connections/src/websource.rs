//! Host-owned web source fetches (ADR 0004). The sandbox never gets network
//! access: the host issues one plain HTTPS GET per declared source and passes
//! the response text to the program as data through `macroscope.source(name)`.
//!
//! Rules enforced here: GET only, no cookies, credentials or custom headers,
//! 10 s timeout, 2 MB response cap, redirects only to the same host, and the
//! URL host must be in the tool's approved host list. Query parameters may use
//! `{today}`; the caller substitutes the host's local YYYY-MM-DD date before
//! calling. `http://127.0.0.1` is accepted only when the caller passes the
//! test-only flag for a local stand-in API; release paths always pass false.
use reqwest::redirect::Policy;
use std::collections::BTreeMap;
use std::time::Duration;
use url::Url;

pub const RESPONSE_CAP: usize = 2 * 1024 * 1024;
pub const FETCH_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_REDIRECTS: usize = 5;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum FetchError {
    #[error("web source URL must use https")]
    NotHttps,
    #[error("web source URL host could not be parsed")]
    UnparseableHost,
    #[error("web source host is not approved for this tool")]
    UnapprovedHost,
    #[error("web source URL must not contain credentials")]
    Credentials,
    #[error("redirect to another host is not allowed")]
    CrossHostRedirect,
    #[error("web source response exceeds 2 MB")]
    TooLarge,
    #[error("Local only is on: web sources cannot be fetched")]
    LocalOnly,
    #[error("web source fetch failed: {0}")]
    Transport(String),
}

fn parse_url(raw: &str, allow_insecure: bool) -> Result<Url, FetchError> {
    let url = Url::parse(raw).map_err(|_| FetchError::UnparseableHost)?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err(FetchError::Credentials);
    }
    let insecure_local =
        allow_insecure && url.scheme() == "http" && matches!(url.host_str(), Some("127.0.0.1"));
    if url.scheme() != "https" && !insecure_local {
        return Err(FetchError::NotHttps);
    }
    if url.host_str().is_none() {
        return Err(FetchError::UnparseableHost);
    }
    Ok(url)
}

fn client() -> Result<reqwest::Client, FetchError> {
    reqwest::Client::builder()
        .timeout(FETCH_TIMEOUT)
        // Follow redirects only back to the host that issued them; anything
        // else stops and is reported as a cross-host redirect below.
        .redirect(Policy::custom(|attempt| {
            if attempt.previous().len() > MAX_REDIRECTS {
                return attempt.stop();
            }
            let same = attempt
                .previous()
                .last()
                .and_then(|u| u.host_str().map(str::to_ascii_lowercase))
                == attempt.url().host_str().map(str::to_ascii_lowercase);
            if same {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }))
        .build()
        .map_err(|e| FetchError::Transport(e.to_string()))
}

/// Fetches every declared source after substituting nothing: URLs arrive with
/// `{today}` already replaced. Returns response texts keyed by source name.
/// Names must be unique; every URL host must be approved (case-insensitive).
pub async fn fetch_all(
    sources: &[(String, String)],
    approved_hosts: &[String],
    allow_insecure: bool,
) -> Result<BTreeMap<String, String>, FetchError> {
    let approved: Vec<String> = approved_hosts
        .iter()
        .map(|h| h.to_ascii_lowercase())
        .collect();
    let client = client()?;
    let mut out = BTreeMap::new();
    for (name, raw_url) in sources {
        if name.trim().is_empty() || out.contains_key(name) {
            return Err(FetchError::Transport("duplicate web source name".into()));
        }
        let url = parse_url(raw_url, allow_insecure)?;
        let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
        if !approved.contains(&host) {
            return Err(FetchError::UnapprovedHost);
        }
        // The host issues a bare GET: no cookies, credentials or custom
        // headers are ever attached.
        let response = client
            .get(url.clone())
            .send()
            .await
            .map_err(|e| FetchError::Transport(short_error(e)))?;
        if response.status().is_redirection() {
            return Err(FetchError::CrossHostRedirect);
        }
        if !response.status().is_success() {
            return Err(FetchError::Transport(format!(
                "HTTP {}",
                response.status().as_u16()
            )));
        }
        if response
            .content_length()
            .is_some_and(|n| n > RESPONSE_CAP as u64)
        {
            return Err(FetchError::TooLarge);
        }
        let final_host = response
            .url()
            .host_str()
            .unwrap_or_default()
            .to_ascii_lowercase();
        if final_host != host {
            return Err(FetchError::CrossHostRedirect);
        }
        let mut bytes = Vec::new();
        let mut response = response;
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| FetchError::Transport(short_error(e)))?
        {
            if bytes.len() + chunk.len() > RESPONSE_CAP {
                return Err(FetchError::TooLarge);
            }
            bytes.extend_from_slice(&chunk);
        }
        let text = String::from_utf8(bytes)
            .map_err(|_| FetchError::Transport("response is not UTF-8".into()))?;
        out.insert(name.clone(), text);
    }
    Ok(out)
}

fn short_error(e: reqwest::Error) -> String {
    if e.is_timeout() {
        "timed out after 10 s".into()
    } else if e.is_redirect() {
        "too many redirects".into()
    } else {
        "connection failed".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insecure_http_is_rejected_without_the_test_flag() {
        assert_eq!(
            parse_url("http://127.0.0.1:9/x", false).unwrap_err(),
            FetchError::NotHttps
        );
        assert!(parse_url("http://127.0.0.1:9/x", true).is_ok());
        assert_eq!(
            parse_url("http://example.com/x", true).unwrap_err(),
            FetchError::NotHttps
        );
        assert_eq!(
            parse_url("https://user@example.com/x", false).unwrap_err(),
            FetchError::Credentials
        );
        assert_eq!(
            parse_url("https://", false).unwrap_err(),
            FetchError::UnparseableHost
        );
    }
}
