//! Host-owned web source fetching (ADR 0004). The sandbox never gets network
//! access: the host issues one plain HTTPS GET per declared source and passes
//! the response text to the program as data through `macroscope.source(name)`.

use crate::dto::ModelRoute;
use macroscope_storage::db::{self, Db};
use std::collections::BTreeMap;

/// These host transport errors require access/cooldown review, not model retries.
pub(crate) fn access_failure(message: &str) -> bool {
    ["HTTP 401", "HTTP 403", "HTTP 429"]
        .iter()
        .any(|status| message.contains(status))
}

/// Local YYYY-MM-DD date for `{today}` query parameters, from the host clock.
pub fn today_local() -> String {
    // `date` prints the host's local calendar date on macOS and Linux; the
    // UTC fallback keeps staging working if it is ever unavailable.
    if let Ok(output) = std::process::Command::new("date")
        .arg("+%F")
        .env_clear()
        .output()
    {
        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if text.len() == 10
                && text.as_bytes()[4] == b'-'
                && text.as_bytes()[7] == b'-'
                && text
                    .bytes()
                    .enumerate()
                    .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
            {
                return text;
            }
        }
    }
    let now = time::OffsetDateTime::now_utc();
    format!(
        "{:04}-{:02}-{:02}",
        now.year(),
        u8::from(now.month()),
        now.day()
    )
}

/// Substitutes the host's local date into a declared source URL template.
pub fn substitute_today(template: &str, today: &str) -> String {
    template.replace("{today}", today)
}

/// Only the origin and path enter outbound history or Privacy. Query values
/// are data even when the parameter name passed validation.
pub fn log_url(url: &str) -> String {
    url.split(['?', '#']).next().unwrap_or(url).to_string()
}

/// Lowercase host (without port) of a substituted URL, for the outbound log.
pub fn host_of_url(url: &str) -> String {
    url.split("://")
        .nth(1)
        .unwrap_or_default()
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .split('@')
        .next_back()
        .unwrap_or_default()
        .split(':')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
}

/// Empty stand-in snapshot for tools with no input file. The broker still
/// stages through its file snapshot machinery; this constant empty file keeps
/// that path intact while the run plan hash binds the source URLs and
/// fetched-response digests instead of input bytes.
pub fn no_input_sentinel(data_dir: &std::path::Path) -> std::io::Result<std::path::PathBuf> {
    let path = data_dir.join("no-input.empty");
    if !path.exists() {
        std::fs::write(&path, b"")?;
    }
    Ok(path)
}

/// Hosts the tool version was approved to fetch from (the tool's grant).
pub fn approved_hosts(scope: &serde_json::Value) -> Vec<String> {
    scope
        .get("webHosts")
        .and_then(|v| serde_json::from_value::<Vec<String>>(v.clone()).ok())
        .unwrap_or_default()
}

/// One recorded host fetch for the outbound log and Privacy screen.
pub fn record_fetch(
    db: &Db,
    id: &str,
    at_ms: i64,
    host: &str,
    url: &str,
) -> Result<(), db::DbError> {
    db.insert_outbound(&db::OutboundRow {
        id: id.into(),
        at_ms,
        purpose: "Web source".into(),
        destination: host.into(),
        route: ModelRoute::OfflineDemo.as_str().into(),
        synthetic_only: false,
        cost_micros: None,
        payload_text: Some(log_url(url)),
    })
}

/// Fetches one declared source and returns its response text. Only GET, no
/// cookies, credentials or custom headers, 10 s timeout, 2 MB cap, redirects
/// only to the same host.
pub async fn fetch_one(
    url: &str,
    approved_hosts: &[String],
    allow_insecure: bool,
    name: &str,
) -> Result<String, String> {
    let mut map = macroscope_connections::websource::fetch_all(
        &[(name.to_string(), url.to_string())],
        approved_hosts,
        allow_insecure,
    )
    .await
    .map_err(|e| format!("Web source '{name}' could not be fetched: {e}."))?;
    map.remove(name)
        .ok_or_else(|| format!("Web source '{name}' could not be fetched."))
}

/// Sorted digests of fetched response bodies for the run plan binding.
pub fn digests(fetched: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    fetched
        .iter()
        .map(|(k, v)| {
            (
                k.clone(),
                format!("{:x}", <sha2::Sha256 as sha2::Digest>::digest(v.as_bytes())),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn today_substitution_and_log_shape() {
        assert_eq!(
            substitute_today("https://x.test/v1?start={today}&end={today}", "2026-10-27"),
            "https://x.test/v1?start=2026-10-27&end=2026-10-27"
        );
        assert_eq!(
            host_of_url("https://API.Open-Meteo.com:443/v1?x=1"),
            "api.open-meteo.com"
        );
        assert_eq!(host_of_url("http://127.0.0.1:9/forecast"), "127.0.0.1");
        assert_eq!(log_url("https://x.test/v1?a=1#frag"), "https://x.test/v1");
        let today = today_local();
        assert_eq!(today.len(), 10);
        assert!(today.as_bytes()[4] == b'-' && today.as_bytes()[7] == b'-');
    }

    #[test]
    fn grant_hosts_and_digests() {
        let scope = serde_json::json!({"webHosts": ["a.test", "b.test"]});
        assert_eq!(approved_hosts(&scope), ["a.test", "b.test"]);
        assert!(approved_hosts(&serde_json::json!({})).is_empty());
        let first = digests(&BTreeMap::from([("s".to_string(), "body".to_string())]));
        let second = digests(&BTreeMap::from([("s".to_string(), "other".to_string())]));
        assert_eq!(first.len(), 1);
        assert_ne!(first["s"], second["s"]);
    }
}
