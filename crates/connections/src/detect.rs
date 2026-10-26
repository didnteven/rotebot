//! Consume only user-authorized recording signals or deliberate task text.
//! Input is consumed and discarded; no raw text is serializable through this API.
use crate::{
    catalog::{Catalog, TitleMatch},
    state::{ConnectionState, ConnectionStatus},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

pub enum Signal {
    ForegroundApp { bundle_id: String, app_name: String },
    WindowTitle { bundle_id: String, title: String },
    BrowserHost { host: String },
    TaskText { text: String },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalKind {
    ForegroundApp,
    WindowTitle,
    BrowserHost,
    TaskText,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Confidence {
    High,
    Medium,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Detection {
    pub connector_id: String,
    pub signal_kind: SignalKind,
    pub confidence: Confidence,
    pub matched_rule_id: String,
}

pub fn detect(catalog: &Catalog, signal: Signal) -> Vec<Detection> {
    let mut results = Vec::new();
    for connector in &catalog.connectors {
        let signatures = &connector.signatures;
        let matched = match &signal {
            Signal::ForegroundApp { bundle_id, .. } => signatures
                .bundle_ids
                .iter()
                .position(|b| b == bundle_id)
                .map(|i| {
                    (
                        SignalKind::ForegroundApp,
                        Confidence::High,
                        format!("{}-bundle-{i}", connector.id),
                    )
                }),
            Signal::BrowserHost { host } => {
                let host = host.trim_end_matches('.').to_ascii_lowercase();
                signatures
                    .browser_hosts
                    .iter()
                    .position(|pattern| {
                        if let Some(suffix) = pattern.strip_prefix("*.") {
                            host.strip_suffix(suffix)
                                .is_some_and(|prefix| prefix.ends_with('.') && prefix.len() > 1)
                        } else {
                            host == *pattern
                        }
                    })
                    .map(|i| {
                        (
                            SignalKind::BrowserHost,
                            Confidence::High,
                            format!("{}-host-{i}", connector.id),
                        )
                    })
            }
            Signal::WindowTitle { title, .. } => {
                let title = title.to_lowercase();
                signatures
                    .window_titles
                    .iter()
                    .find(|rule| {
                        let value = rule.value.to_lowercase();
                        match rule.kind {
                            TitleMatch::Suffix => title.ends_with(&value),
                            TitleMatch::Contains => title.contains(&value),
                        }
                    })
                    .map(|rule| (SignalKind::WindowTitle, Confidence::High, rule.id.clone()))
            }
            Signal::TaskText { text } => {
                let text = text.to_lowercase();
                signatures
                    .keywords
                    .iter()
                    .position(|keyword| word_match(&text, keyword))
                    .map(|i| {
                        (
                            SignalKind::TaskText,
                            Confidence::Medium,
                            format!("{}-keyword-{i}", connector.id),
                        )
                    })
            }
        };
        if let Some((signal_kind, confidence, matched_rule_id)) = matched {
            results.push(Detection {
                connector_id: connector.id.clone(),
                signal_kind,
                confidence,
                matched_rule_id,
            });
        }
    }
    results
}
fn word_match(text: &str, keyword: &str) -> bool {
    let word = |c: char| c.is_alphanumeric() || c == '_';
    text.match_indices(keyword).any(|(start, _)| {
        !text[..start].chars().next_back().is_some_and(word)
            && !text[start + keyword.len()..]
                .chars()
                .next()
                .is_some_and(word)
    })
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalCount {
    pub count: u64,
    pub first_seen_ms: u64,
    pub last_seen_ms: u64,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DetectionLedger {
    pub counts: BTreeMap<String, BTreeMap<SignalKind, SignalCount>>,
    dismissed: HashSet<String>,
    snoozed: BTreeMap<String, u64>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Suggestion {
    pub connector_id: String,
    pub count: u64,
    pub signal_kind: SignalKind,
    pub message: String,
}
impl DetectionLedger {
    pub fn observe(&mut self, catalog: &Catalog, signal: Signal, at_ms: u64) -> Vec<Detection> {
        let detections = detect(catalog, signal);
        for detection in &detections {
            let count = self
                .counts
                .entry(detection.connector_id.clone())
                .or_default()
                .entry(detection.signal_kind)
                .or_insert(SignalCount {
                    count: 0,
                    first_seen_ms: at_ms,
                    last_seen_ms: at_ms,
                });
            count.count = count.count.saturating_add(1);
            count.first_seen_ms = count.first_seen_ms.min(at_ms);
            count.last_seen_ms = count.last_seen_ms.max(at_ms);
        }
        detections
    }
    pub fn dismiss(&mut self, connector_id: &str) {
        self.dismissed.insert(connector_id.into());
    }
    pub fn snooze(&mut self, connector_id: &str, until_ms: u64) {
        self.snoozed.insert(connector_id.into(), until_ms);
    }
    pub fn suggestions(
        &self,
        statuses: &[ConnectionStatus],
        dismissed: &HashSet<String>,
    ) -> Vec<Suggestion> {
        self.suggestions_at(&Catalog::embedded(), statuses, dismissed, crate::now_ms())
    }
    pub fn suggestions_at(
        &self,
        catalog: &Catalog,
        statuses: &[ConnectionStatus],
        dismissed: &HashSet<String>,
        at_ms: u64,
    ) -> Vec<Suggestion> {
        let mut suggestions = Vec::new();
        for (id, kinds) in &self.counts {
            if self.dismissed.contains(id)
                || dismissed.contains(id)
                || self.snoozed.get(id).is_some_and(|until| *until > at_ms)
                || statuses.iter().any(|s| {
                    s.connector_id == *id
                        && matches!(
                            s.state,
                            ConnectionState::Connected { .. } | ConnectionState::Connecting
                        )
                })
            {
                continue;
            }
            let Some(connector) = catalog.get(id) else {
                continue;
            };
            let Some((kind, _)) = kinds.iter().max_by_key(|(_, count)| count.count) else {
                continue;
            };
            let count = kinds
                .values()
                .fold(0u64, |sum, c| sum.saturating_add(c.count));
            let prefix = if *kind == SignalKind::TaskText {
                format!("You mentioned {} in your task.", connector.name)
            } else {
                format!("You used {} while recording.", connector.name)
            };
            suggestions.push(Suggestion {
                connector_id: id.clone(),
                count,
                signal_kind: *kind,
                message: format!(
                    "{prefix} Connect {} so tools can work with it.",
                    connector.name
                ),
            });
        }
        suggestions.sort_by(|a, b| {
            b.count
                .cmp(&a.count)
                .then_with(|| a.connector_id.cmp(&b.connector_id))
        });
        suggestions
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn detection_rules_and_word_boundaries() {
        let catalog = Catalog::embedded();
        let cases = [
            (
                Signal::WindowTitle {
                    bundle_id: "browser".into(),
                    title: "Inbox - Gmail".into(),
                },
                "gmail",
                SignalKind::WindowTitle,
                Confidence::High,
            ),
            (
                Signal::BrowserHost {
                    host: "MAIL.GOOGLE.COM.".into(),
                },
                "gmail",
                SignalKind::BrowserHost,
                Confidence::High,
            ),
            (
                Signal::ForegroundApp {
                    bundle_id: "com.tinyspeck.slackmacgap".into(),
                    app_name: "Slack".into(),
                },
                "slack",
                SignalKind::ForegroundApp,
                Confidence::High,
            ),
            (
                Signal::TaskText {
                    text: "Send this to SLACK, please".into(),
                },
                "slack",
                SignalKind::TaskText,
                Confidence::Medium,
            ),
            (
                Signal::BrowserHost {
                    host: "team.atlassian.net".into(),
                },
                "jira",
                SignalKind::BrowserHost,
                Confidence::High,
            ),
        ];
        for (signal, id, kind, confidence) in cases {
            let detections = detect(&catalog, signal);
            assert_eq!(detections.len(), 1);
            assert_eq!(detections[0].connector_id, id);
            assert_eq!(detections[0].signal_kind, kind);
            assert_eq!(detections[0].confidence, confidence);
        }
        for signal in [
            Signal::ForegroundApp {
                bundle_id: "com.apple.mail".into(),
                app_name: "Gmail".into(),
            },
            Signal::TaskText {
                text: "gmailish xgmail gmail_thing égmail".into(),
            },
            Signal::BrowserHost {
                host: "mail.google.com.evil.test".into(),
            },
            Signal::BrowserHost {
                host: "evilatlassian.net".into(),
            },
            Signal::WindowTitle {
                bundle_id: "browser".into(),
                title: "Inbox - Gmail impostor".into(),
            },
        ] {
            assert!(detect(&catalog, signal).is_empty());
        }
    }
    #[test]
    fn ledger_privacy_filtering_and_order() {
        let catalog = Catalog::embedded();
        let mut ledger = DetectionLedger::default();
        ledger.observe(
            &catalog,
            Signal::WindowTitle {
                bundle_id: "com.private.browser".into(),
                title: "private.person@gmail.com - Gmail".into(),
            },
            10,
        );
        ledger.observe(
            &catalog,
            Signal::TaskText {
                text: "slack notion linear jira github".into(),
            },
            20,
        );
        ledger.observe(
            &catalog,
            Signal::BrowserHost {
                host: "mail.google.com".into(),
            },
            30,
        );
        let serialized = serde_json::to_string(&ledger).unwrap();
        assert!(!serialized.contains("private.person"));
        assert!(!serialized.contains("@"));
        assert!(!serialized.contains("com.private.browser"));
        let mut ledger: DetectionLedger = serde_json::from_str(&serialized).unwrap();
        let connected = [ConnectionStatus {
            connector_id: "notion".into(),
            state: ConnectionState::Connected {
                since_ms: 0,
                tool_count: 1,
            },
            mcp_url: "".into(),
            last_checked_ms: None,
        }];
        ledger.dismiss("linear");
        ledger.snooze("jira", 100);
        let dismissed = HashSet::from(["github".into()]);
        let suggestions = ledger.suggestions_at(&catalog, &connected, &dismissed, 99);
        assert_eq!(
            suggestions
                .iter()
                .map(|s| s.connector_id.as_str())
                .collect::<Vec<_>>(),
            ["gmail", "slack"]
        );
        assert!(suggestions[0].message.contains("while recording"));
        assert!(suggestions[1].message.contains("mentioned Slack"));
        assert_eq!(suggestions[0].count, 2);
        assert!(ledger
            .suggestions_at(&catalog, &connected, &dismissed, 100)
            .iter()
            .any(|s| s.connector_id == "jira"));
    }
}
