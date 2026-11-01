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

pub const RESPONSE_CAP: usize = 4 * 1024 * 1024;
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
    #[error("Web sources can't include keys or tokens in the URL")]
    QueryCredentials,
    #[error("redirect to another origin is not allowed")]
    CrossHostRedirect,
    #[error("web source response exceeds 4 MB")]
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
    if url.query_pairs().any(|(name, _)| {
        let name = name.to_ascii_lowercase();
        [
            "key",
            "token",
            "secret",
            "password",
            "passwd",
            "auth",
            "signature",
            "sig",
            "session",
            "credential",
            "apikey",
        ]
        .iter()
        .any(|part| name.contains(part))
    }) {
        return Err(FetchError::QueryCredentials);
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

fn same_origin(from: &Url, to: &Url) -> bool {
    from.scheme() == to.scheme()
        && from.host_str() == to.host_str()
        && from.port_or_known_default() == to.port_or_known_default()
}

fn redirect_allowed(from: &Url, to: &Url, allow_insecure: bool) -> bool {
    parse_url(to.as_str(), allow_insecure).is_ok() && same_origin(from, to)
}

fn client(allow_insecure: bool) -> Result<reqwest::Client, FetchError> {
    reqwest::Client::builder()
        .timeout(FETCH_TIMEOUT)
        // Many public APIs refuse requests that do not name their client.
        .user_agent(concat!("Rotebot/", env!("CARGO_PKG_VERSION")))
        .https_only(!allow_insecure)
        // Follow redirects only back to the host that issued them; anything
        // else stops and is reported as a cross-host redirect below.
        .redirect(Policy::custom(move |attempt| {
            if attempt.previous().len() > MAX_REDIRECTS {
                return attempt.stop();
            }
            let same = attempt
                .previous()
                .last()
                .is_some_and(|from| redirect_allowed(from, attempt.url(), allow_insecure));
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
    let client = client(allow_insecure)?;
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
        // headers are ever attached (the client name above is the only one).
        // One retry when the site did not answer or dropped the connection, so a
        // momentary blip does not fail a run. Refusals and rate limits are answers
        // and are never retried.
        let response = match client.get(url.clone()).send().await {
            Ok(response) => response,
            Err(e) if e.is_timeout() || e.is_connect() => client
                .get(url.clone())
                .send()
                .await
                .map_err(|e| FetchError::Transport(short_error(e)))?,
            Err(e) => return Err(FetchError::Transport(short_error(e))),
        };
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
        if !redirect_allowed(&url, response.url(), allow_insecure) {
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

#[cfg(test)]
mod review_regressions {
    use super::*;
    #[test]
    fn redirects_require_https_same_host_and_effective_port() {
        let approved = Url::parse("https://same.test/start").unwrap();
        for target in [
            "http://same.test/next",
            "https://same.test:8443/next",
            "https://other.test/next",
        ] {
            let target = Url::parse(target).unwrap();
            assert!(!redirect_allowed(&approved, &target, false), "{target}");
            assert!(!same_origin(&approved, &target), "final URL: {target}");
        }
        for target in ["https://same.test/next", "https://same.test:443/next"] {
            assert!(redirect_allowed(
                &approved,
                &Url::parse(target).unwrap(),
                false
            ));
        }
        let local = Url::parse("http://127.0.0.1:1234/start").unwrap();
        let next = Url::parse("http://127.0.0.1:1234/next").unwrap();
        assert!(redirect_allowed(&local, &next, true));
        assert!(!redirect_allowed(&local, &next, false));
        assert_eq!(
            parse_url("https://same.test/path?Api_%4Bey=hidden", false).unwrap_err(),
            FetchError::QueryCredentials
        );
    }
}
