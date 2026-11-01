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

/// The plain sentence for a build that failed only because a site did not answer
/// (a timeout or refused connection), naming the site. None for other failures.
pub(crate) fn unreachable_site_message(failures: &[String]) -> Option<String> {
    let detail = failures.iter().find(|d| {
        d.contains("could not be fetched")
            && (d.contains("timed out") || d.contains("connection failed") || d.contains("did not answer"))
    })?;
    let host = detail
        .split("could not be fetched from ")
        .nth(1)
        .and_then(|rest| rest.split(':').next())
        .or_else(|| detail.split(": ").nth(1).and_then(|rest| rest.split(" did not answer").next()))
        .filter(|h| !h.is_empty() && !h.contains(' '))
        .unwrap_or("The site");
    Some(format!(
        "{host} didn't answer, so the tool couldn't get its data. Many sites refuse automated requests: try again later, or name a different site in the request."
    ))
}

/// Whether the person's request (or the API plan made for it) names the site
/// this URL is on. A refusal from such a site stops the build; a site the model
/// picked on its own is just replaced with another public one.
pub(crate) fn chosen_up_front(request: &str, plan: Option<&str>, url: &str) -> bool {
    let host = host_of_url(url);
    let labels: Vec<&str> = host.split('.').collect();
    let site = if labels.len() > 2 && labels.iter().any(|l| l.parse::<u8>().is_err()) {
        labels[labels.len() - 2..].join(".")
    } else {
        host.clone()
    };
    request.to_ascii_lowercase().contains(&site) || plan.is_some_and(|p| p.contains(&host))
}

/// What an ordinary web page holds, the way `macroscope.html` reads it: its
/// title, the first rows of each table, the start of its visible text and the
/// JSON it embeds. A repair round sees this instead of the page's first
/// characters, which are mostly markup. A JSON response whose largest string is
/// a page (as many APIs return) is previewed the same way, with that string's
/// JSON path. None when the text holds no HTML.
pub(crate) fn page_preview(text: &str) -> Option<serde_json::Value> {
    use regex::Regex;
    use std::sync::LazyLock;
    static HIDDEN: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?is)<!--.*?-->|<script\b.*?</script\s*>|<style\b.*?</style\s*>|<noscript\b.*?</noscript\s*>|<svg\b.*?</svg\s*>|<template\b.*?</template\s*>|<head\b.*?</head\s*>").unwrap()
    });
    static TABLE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?is)<table\b.*?</table\s*>").unwrap());
    static ROW: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?is)<tr\b.*?(?:</tr\s*>|$)").unwrap());
    static CELL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?is)<t([hd])\b[^>]*>(.*?)(?:</t[hd]\s*>|$)").unwrap());
    static BLOCK: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)<br\s*/?>|</?(?:p|div|li|ul|ol|tr|table|h[1-6]|section|article|header|footer|main|nav|dt|dd)\b[^>]*>").unwrap()
    });
    static TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<[^>]*>").unwrap());
    static HEADING: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?is)<h([1-6])\b[^>]*>(.*?)</h[1-6]\s*>").unwrap());
    static TITLE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?is)<title\b[^>]*>(.*?)</title\s*>").unwrap());
    static JSON: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"(?is)<script\b[^>]*(?:type\s*=\s*["']?[^"'>]*json|id\s*=\s*["']?__NEXT_DATA__)[^>]*>(.*?)</script\s*>"#).unwrap()
    });
    static MARKUP: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)<(?:html|body|p|a|div|table|span|li|h[1-6])\b").unwrap());
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(text) {
        fn largest<'a>(v: &'a serde_json::Value, path: String, depth: usize) -> Option<(String, &'a str)> {
            match v {
                serde_json::Value::String(s) => Some((path, s.as_str())),
                serde_json::Value::Array(items) if depth < 8 => items
                    .iter()
                    .enumerate()
                    .filter_map(|(i, x)| largest(x, format!("{path}/{i}"), depth + 1))
                    .max_by_key(|(_, s)| s.len()),
                serde_json::Value::Object(map) if depth < 8 => map
                    .iter()
                    .filter_map(|(k, x)| largest(x, format!("{path}/{k}"), depth + 1))
                    .max_by_key(|(_, s)| s.len()),
                _ => None,
            }
        }
        let (path, inner) = largest(&value, String::new(), 0)?;
        if inner.len() < 200 || MARKUP.find_iter(inner).take(3).count() < 3 {
            return None;
        }
        let mut preview = page_preview(inner)?;
        preview["json_path"] = serde_json::json!(path);
        return Some(preview);
    }
    if MARKUP.find_iter(text).take(3).count() < 3 {
        return None;
    }
    static NUMERIC: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"&#(?:[xX]([0-9a-fA-F]{1,6})|([0-9]{1,7}));").unwrap());
    let decode = |s: &str| {
        NUMERIC
            .replace_all(s, |c: &regex::Captures| {
                c.get(1)
                    .and_then(|h| u32::from_str_radix(h.as_str(), 16).ok())
                    .or_else(|| c.get(2).and_then(|d| d.as_str().parse().ok()))
                    .and_then(char::from_u32)
                    .map(String::from)
                    .unwrap_or_default()
            })
            .replace("&nbsp;", " ")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&#39;", "'")
            .replace("&amp;", "&")
    };
    let flat = |s: &str| decode(&TAG.replace_all(s, " ")).split_whitespace().collect::<Vec<_>>().join(" ");
    let visible = HIDDEN.replace_all(text, " ");
    let tables: Vec<serde_json::Value> = TABLE
        .find_iter(&visible)
        .take(6)
        .map(|t| {
            let mut headers = Vec::new();
            let mut rows = Vec::new();
            for row in ROW.find_iter(t.as_str()) {
                let cells: Vec<(bool, String)> = CELL
                    .captures_iter(row.as_str())
                    .map(|c| (c[1].eq_ignore_ascii_case("h"), flat(&c[2])))
                    .collect();
                if cells.iter().all(|(_, c)| c.is_empty()) {
                    continue;
                }
                if headers.is_empty() && rows.is_empty() && cells.iter().any(|(th, _)| *th) {
                    headers = cells.into_iter().map(|(_, c)| c).collect();
                } else {
                    rows.push(cells.into_iter().map(|(_, c)| c).collect::<Vec<_>>());
                }
            }
            serde_json::json!({"headers": headers, "row_count": rows.len(), "first_rows": rows.into_iter().take(3).collect::<Vec<_>>()})
        })
        .collect();
    let lines: Vec<String> = decode(&TAG.replace_all(&BLOCK.replace_all(&visible, "\n"), " "))
        .lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|l| !l.is_empty())
        .collect();
    let embedded: Vec<String> = JSON
        .captures_iter(text)
        .take(3)
        .map(|c| c[1].trim().chars().take(1200).collect())
        .collect();
    Some(serde_json::json!({
        "read_with": "macroscope.html.headings / tables / text / links / json / title",
        "title": TITLE.captures(text).map(|c| flat(&c[1])).unwrap_or_default(),
        "headings": HEADING
            .captures_iter(&visible)
            .map(|c| format!("h{}: {}", &c[1], flat(&c[2])))
            .filter(|h| h.len() > 4)
            .take(40)
            .collect::<Vec<_>>(),
        "tables": tables,
        "visible_text_start": lines.join("\n").chars().take(2500).collect::<String>(),
        "embedded_json_start": embedded,
    }))
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

/// Why a fetched page is handed to the source search: it answered but did not hold the data.
/// Its site is not ruled out, since another page there may hold it.
pub const PAGE_LACKED_DATA: &str = "The page was fetched but did not hold the data the task needs.";

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
    .map_err(|e| {
        format!(
            "Web source '{name}' could not be fetched from {}: {e}.",
            host_of_url(url)
        )
    })?;
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
    fn a_site_that_does_not_answer_is_named_plainly() {
        let timed_out = vec!["Web source 'stocks' could not be fetched from api.nasdaq.com: web source fetch failed: timed out after 10 s.".to_string()];
        assert!(unreachable_site_message(&timed_out).unwrap().starts_with("api.nasdaq.com didn't answer"));
        let repeated = vec!["Web source 'stocks' could not be fetched: api.nasdaq.com did not answer earlier in this build, so use a different site.".to_string()];
        assert!(unreachable_site_message(&repeated).unwrap().starts_with("api.nasdaq.com didn't answer"));
        assert!(unreachable_site_message(&["Web source 'a' could not be fetched from a.test: web source fetch failed: HTTP 404.".to_string()]).is_none());
    }

    #[test]
    fn only_sites_the_person_named_count_as_chosen() {
        let url = "https://api.nasdaq.com/api/screener/stocks";
        assert!(chosen_up_front("Top movers from Nasdaq.com", None, url));
        assert!(!chosen_up_front("The most fearful stock right now", None, url));
        assert!(chosen_up_front("x", Some(r#"{"baseUrl":"https://api.nasdaq.com"}"#), url));
        assert!(chosen_up_front("read http://127.0.0.1:9/denied", None, "http://127.0.0.1:9/denied"));
    }

    #[test]
    fn a_page_preview_shows_tables_and_text_not_markup() {
        let page = "<!doctype html><html><head><title>Movers</title><script>var x='<table>';</script></head><body><h1>Top losers &#x27;today&#39;</h1><table><tr><th>Symbol</th><th>Change</th></tr><tr><td><a href='/abos'>ABOS</a></td><td>-90.83%</td></tr></table><script type=\"application/json\">{\"a\":1}</script></body></html>";
        let preview = page_preview(page).unwrap();
        assert_eq!(preview["title"], "Movers");
        assert_eq!(preview["headings"][0], "h1: Top losers 'today'");
        assert_eq!(preview["tables"][0]["headers"], serde_json::json!(["Symbol", "Change"]));
        assert_eq!(preview["tables"][0]["first_rows"][0], serde_json::json!(["ABOS", "-90.83%"]));
        assert_eq!(preview["tables"][0]["row_count"], 1);
        assert!(preview["visible_text_start"].as_str().unwrap().starts_with("Top losers 'today'"));
        assert_eq!(preview["embedded_json_start"][0], "{\"a\":1}");
        assert!(page_preview("{\"a\": 1}").is_none());
        let wrapped = serde_json::json!({"parse": {"title": "x", "text": {"*": format!("<style>.a{{}}</style><div><p>Today's article: <b><a href='/wiki/Moa'>Moa</a></b></p>{}</div>", " ".repeat(200))}}}).to_string();
        let inner = page_preview(&wrapped).unwrap();
        assert_eq!(inner["json_path"], "/parse/text/*");
        assert!(inner["visible_text_start"].as_str().unwrap().starts_with("Today's article: Moa"));
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
