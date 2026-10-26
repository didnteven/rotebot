//! Opt-in probes: no credentials, authorization requests, DCR, or token requests.
use macroscope_connections::{
    catalog::{Catalog, Registration, Verification},
    oauth::{MemoryTokenStore, OAuthClient},
};
use std::sync::Arc;

#[tokio::test]
#[ignore = "requires MACROSCOPE_LIVE_MCP=1 and network access"]
async fn live_discovery_matches_catalog() {
    if std::env::var("MACROSCOPE_LIVE_MCP").as_deref() != Ok("1") {
        eprintln!("skipped: set MACROSCOPE_LIVE_MCP=1 to probe real MCP servers");
        return;
    }
    let client = OAuthClient::new(Arc::new(MemoryTokenStore::default())).unwrap();
    let mut probes = tokio::task::JoinSet::new();
    for connector in Catalog::embedded().connectors {
        if connector.verification != Verification::VerifiedLive {
            continue;
        }
        let client = client.clone();
        probes.spawn(async move {
            let expected_issuer = match connector.id.as_str() {
                "gmail" | "google-calendar" | "google-drive" => {
                    connector.auth.authorization_server.as_deref().unwrap()
                }
                "slack" => "https://mcp.slack.com",
                "notion" => "https://mcp.notion.com",
                "github" => "https://github.com/login/oauth",
                "linear" => "https://mcp.linear.app",
                "jira" => "https://mcp.atlassian.com",
                id => panic!("missing issuer expectation for {id}"),
            };
            match client
                .discover_connector(&connector, &connector.mcp_url)
                .await
            {
                Ok(discovered) => {
                    let dcr = discovered.metadata.registration_endpoint.is_some();
                    let expected_dcr = connector.auth.registration == Registration::Dynamic;
                    let report = format!(
                        "{}: issuer={}, DCR={} (expected issuer={}, DCR={})",
                        connector.id,
                        discovered.metadata.issuer,
                        dcr,
                        expected_issuer,
                        expected_dcr
                    );
                    let matches =
                        discovered.metadata.issuer == expected_issuer && dcr == expected_dcr;
                    (matches, report)
                }
                Err(error) => (
                    false,
                    format!("{}: discovery failed: {error}", connector.id),
                ),
            }
        });
    }
    let mut reports = Vec::new();
    let mut passed = true;
    while let Some(result) = probes.join_next().await {
        let (matches, report) = result.unwrap();
        passed &= matches;
        reports.push(report);
    }
    reports.sort();
    assert_eq!(reports.len(), 8);
    let report = reports.join("\n");
    println!("{report}");
    assert!(
        passed,
        "live discovery did not match the 2026-10-27 catalog:\n{report}"
    );
}
