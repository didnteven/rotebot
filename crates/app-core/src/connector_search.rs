//! Looks up remote MCP servers for a service in the public MCP registry. Only the service name
//! leaves the Mac, only when the person asks, and results are unverified suggestions: nothing is
//! connected until the person signs in and approves what the tool may read.
use crate::*;

const REGISTRY_HOST: &str = "registry.modelcontextprotocol.io";

impl AppCore {
    pub async fn find_connectors(&self, query: &str) -> CoreResult<Vec<ConnectorCandidate>> {
        if let Some(blocked) = self.web_fetch_blocked() {
            return Err(CoreError::Invalid(blocked.replace(
                "web sources cannot be fetched. Turn off Local only to run this tool.",
                "the connector search is off. Turn off Local only to search.",
            )));
        }
        let name: String = query
            .chars()
            .filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '.' | '_'))
            .take(40)
            .collect();
        let name = name.trim();
        if name.is_empty() {
            return Err(CoreError::Invalid("Name the service to search for.".into()));
        }
        let encoded: String = name
            .bytes()
            .map(|b| match b {
                b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'.' | b'_' => {
                    (b as char).to_string()
                }
                _ => format!("%{b:02X}"),
            })
            .collect();
        let url = format!("https://{REGISTRY_HOST}/v0/servers?search={encoded}&limit=30");
        self.db().insert_outbound(&db::OutboundRow {
            id: new_id("connector_search"),
            at_ms: self.now(),
            purpose: "Connector search".into(),
            destination: REGISTRY_HOST.into(),
            route: ModelRoute::OfflineDemo.as_str().into(),
            synthetic_only: false,
            cost_micros: None,
            payload_text: Some(format!("search: {name}")),
        })?;
        let body = web::fetch_one(
            &url,
            &[REGISTRY_HOST.to_string()],
            self.allow_insecure_web(),
            "registry",
        )
        .await
        .map_err(|_| {
            CoreError::Invalid("Couldn't reach the connector registry. Try again later.".into())
        })?;
        Ok(parse_registry(&body))
    }
}

/// Keeps https streamable-http remotes only, newest entry per address, at most eight.
pub(crate) fn parse_registry(body: &str) -> Vec<ConnectorCandidate> {
    let Ok(value) = serde_json::from_str::<Value>(body) else {
        return vec![];
    };
    let mut out: Vec<ConnectorCandidate> = vec![];
    for item in value["servers"].as_array().into_iter().flatten() {
        let server = &item["server"];
        let (Some(name), Some(remotes)) = (server["name"].as_str(), server["remotes"].as_array())
        else {
            continue;
        };
        let Some(url) = remotes.iter().find_map(|r| {
            (r["type"] == "streamable-http")
                .then(|| r["url"].as_str())
                .flatten()
                .filter(|u| u.starts_with("https://") && !u.contains(['{', '}', ' ']))
        }) else {
            continue;
        };
        if out.iter().any(|c| c.url == url) {
            continue;
        }
        let clip = |s: &str, n: usize| {
            s.chars()
                .filter(|c| !c.is_control())
                .take(n)
                .collect::<String>()
        };
        out.push(ConnectorCandidate {
            name: clip(name, 80),
            description: clip(server["description"].as_str().unwrap_or_default(), 160),
            url: url.to_string(),
            // Registry names are reverse-DNS ("com.airtable/mcp"): the owner of that domain published it.
            publisher: name
                .split('/')
                .next()
                .unwrap_or_default()
                .split('.')
                .rev()
                .collect::<Vec<_>>()
                .join("."),
        });
        if out.len() == 8 {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_results_keep_only_https_streamable_http_servers() {
        let body = r#"{"servers":[
          {"server":{"name":"com.airtable/mcp","description":"Official","remotes":[{"type":"streamable-http","url":"https://mcp.airtable.com/mcp"}]}},
          {"server":{"name":"x.y/sse","description":"d","remotes":[{"type":"sse","url":"https://x.y/sse"}]}},
          {"server":{"name":"a.b/local","description":"d","remotes":[]}},
          {"server":{"name":"a.b/plain","description":"d","remotes":[{"type":"streamable-http","url":"http://a.b/mcp"}]}},
          {"server":{"name":"com.airtable/mcp","description":"dup","remotes":[{"type":"streamable-http","url":"https://mcp.airtable.com/mcp"}]}}
        ]}"#;
        let found = parse_registry(body);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].url, "https://mcp.airtable.com/mcp");
        assert_eq!(found[0].publisher, "airtable.com");
        assert!(parse_registry("not json").is_empty());
    }
}
