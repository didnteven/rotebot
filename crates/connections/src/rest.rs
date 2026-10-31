//! Bounded host-owned GET transport for API reads and documentation research.
//! Pin public DNS answers, disable proxies/redirects, and never expose remote errors.
use reqwest::header::{HeaderName, HeaderValue};
use std::net::{IpAddr, Ipv4Addr};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReadError {
    #[error("Use an HTTPS URL without credentials, fragments or secret query parameters")]
    Url,
    #[error("Private or reserved network destinations are not allowed")]
    PrivateNetwork,
    #[error("Invalid API credential or header")]
    Credential,
    #[error("The API redirected; review its final address before connecting")]
    Redirect,
    #[error("API authentication failed; check the saved credential")]
    Unauthorized,
    #[error("The service refused access; check its permissions and access policy")]
    Forbidden,
    #[error("The service is rate limited; wait before trying again")]
    RateLimited { retry_at_ms: u64 },
    #[error("The response exceeded its size limit")]
    TooLarge,
    #[error("The API returned an invalid or unsafe response")]
    InvalidResponse,
    #[error("The request failed or timed out; no retry was attempted")]
    Transport,
    #[error("The service returned HTTP {0}; no retry was attempted")]
    Http(u16),
}

pub fn validate_url(raw: &str, allow_test: bool) -> Result<Url, ReadError> {
    let url = Url::parse(raw).map_err(|_| ReadError::Url)?;
    let local = allow_test && url.scheme() == "http" && url.host_str() == Some("127.0.0.1");
    if (url.scheme() != "https" && !local)
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || url.as_str().len() > 4096
        || url.query_pairs().any(|(k, _)| {
            [
                "key",
                "token",
                "secret",
                "password",
                "credential",
                "signature",
                "auth",
            ]
            .iter()
            .any(|s| k.to_ascii_lowercase().contains(s))
        })
    {
        return Err(ReadError::Url);
    }
    if let Some(ip) = url
        .host_str()
        .and_then(|s| s.trim_matches(['[', ']']).parse::<IpAddr>().ok())
    {
        if !public_ip(ip) && !local {
            return Err(ReadError::PrivateNetwork);
        }
    }
    Ok(url)
}

fn public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !ip.is_private()
        && !ip.is_loopback()
        && !ip.is_link_local()
        && !ip.is_broadcast()
        && !ip.is_documentation()
        && !ip.is_unspecified()
        && !ip.is_multicast()
        && a != 0
        && a < 224
        && !(a == 100 && (64..=127).contains(&b))
        && !(a == 198 && (b == 18 || b == 19))
        && !(a == 192 && b == 0 && c == 0)
        && !(a == 192 && b == 88 && c == 99)
}
pub fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => public_v4(v),
        IpAddr::V6(v) => {
            let s = v.segments();
            // Global unicast only; exclude documentation, Teredo and 6to4.
            s[0] & 0xe000 == 0x2000
                && !(s[0] == 0x2001 && (s[1] < 0x0200 || s[1] == 0x0db8))
                && s[0] != 0x2002
                && s[0] != 0x3fff
        }
    }
}

pub fn validate_header(raw: &str) -> Result<(), ReadError> {
    let name = raw.to_ascii_lowercase();
    if name != "authorization" && !name.starts_with("x-") && name != "api-key" {
        return Err(ReadError::Credential);
    }
    HeaderName::from_bytes(raw.as_bytes()).map_err(|_| ReadError::Credential)?;
    Ok(())
}

pub async fn get(
    raw: &str,
    credential: Option<(&str, &[u8])>,
    allow_test: bool,
    max_bytes: usize,
    timeout: Duration,
) -> Result<String, ReadError> {
    let url = validate_url(raw, allow_test)?;
    let host = url.host_str().ok_or(ReadError::Url)?;
    let local = allow_test && host == "127.0.0.1" && url.scheme() == "http";
    let addresses: Vec<_> = tokio::time::timeout(
        Duration::from_secs(5),
        tokio::net::lookup_host((host, url.port_or_known_default().ok_or(ReadError::Url)?)),
    )
    .await
    .map_err(|_| ReadError::Transport)?
    .map_err(|_| ReadError::Transport)?
    .collect();
    if addresses.is_empty() || (!local && addresses.iter().any(|a| !public_ip(a.ip()))) {
        return Err(ReadError::PrivateNetwork);
    }
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .resolve_to_addrs(host, &addresses)
        .timeout(timeout)
        .user_agent(concat!("Macroscope/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|_| ReadError::Transport)?;
    let mut request = client.get(url.clone());
    if let Some((name, value)) = credential {
        validate_header(name)?;
        let mut header = HeaderValue::from_bytes(value).map_err(|_| ReadError::Credential)?;
        header.set_sensitive(true);
        request = request.header(name, header);
    }
    let mut response = request.send().await.map_err(|_| ReadError::Transport)?;
    let status = response.status();
    match status.as_u16() {
        200..=299 => {}
        300..=399 => return Err(ReadError::Redirect),
        401 => return Err(ReadError::Unauthorized),
        403 => return Err(ReadError::Forbidden),
        429 => {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            let raw = response
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok());
            let retry_at_ms = raw
                .and_then(|s| s.parse::<u64>().ok())
                .map(|s| now.saturating_add(s.saturating_mul(1000)))
                .or_else(|| {
                    raw.and_then(|s| httpdate::parse_http_date(s).ok())
                        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                        .map(|d| d.as_millis() as u64)
                })
                .unwrap_or(now.saturating_add(60_000))
                .max(now.saturating_add(1000))
                .min(i64::MAX as u64);
            return Err(ReadError::RateLimited { retry_at_ms });
        }
        code => return Err(ReadError::Http(code)),
    }
    if response
        .content_length()
        .is_some_and(|n| n > max_bytes as u64)
    {
        return Err(ReadError::TooLarge);
    }
    let mut bytes = Vec::new();
    while let Some(part) = response.chunk().await.map_err(|_| ReadError::Transport)? {
        if part.len() > max_bytes.saturating_sub(bytes.len()) {
            return Err(ReadError::TooLarge);
        }
        bytes.extend_from_slice(&part);
    }
    if credential.is_some_and(|(_, key)| {
        let key = key.strip_prefix(b"Bearer ").unwrap_or(key);
        !key.is_empty() && bytes.windows(key.len()).any(|w| w == key)
    }) {
        return Err(ReadError::InvalidResponse);
    }
    String::from_utf8(bytes).map_err(|_| ReadError::InvalidResponse)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn private_and_ambiguous_destinations_and_credentials_are_rejected() {
        for url in [
            "http://example.com",
            "https://user:pass@example.com",
            "https://127.0.0.1",
            "https://[::1]",
            "https://169.254.169.254/latest",
            "https://10.0.0.1",
            "https://100.64.0.1",
            "https://example.com?api_key=x",
            "https://example.com#x",
        ] {
            assert!(validate_url(url, false).is_err(), "{url}");
        }
        assert!(validate_url("https://api.example.com/v1?date=2026-10-31", false).is_ok());
        assert!(validate_url("http://127.0.0.1:1234/api", true).is_ok());
        for value in [
            "127.0.0.1",
            "192.168.1.1",
            "::ffff:127.0.0.1",
            "2001:db8::1",
            "2002:7f00:1::1",
        ] {
            assert!(!public_ip(value.parse().unwrap()), "{value}");
        }
        assert!(public_ip("1.1.1.1".parse().unwrap()));
        assert!(public_ip("2606:4700:4700::1111".parse().unwrap()));
        for name in [
            "host",
            "cookie",
            "proxy-authorization",
            "connection",
            "x-bad\r\nname",
        ] {
            assert!(validate_header(name).is_err());
        }
    }
}
