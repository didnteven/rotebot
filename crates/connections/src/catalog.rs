use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Catalog {
    pub schema: String,
    pub version: u32,
    pub connectors: Vec<Connector>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connector {
    pub id: String,
    pub name: String,
    pub category: String,
    pub mcp_url: String,
    pub transport: String,
    pub auth: Auth,
    pub verification: Verification,
    pub source: String,
    pub notes: String,
    pub signatures: Signatures,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Auth {
    pub kind: String,
    pub registration: Registration,
    #[serde(default)]
    pub scopes: Vec<String>,
    pub docs_url: String,
    #[serde(default)]
    pub authorization_server: Option<String>,
    #[serde(default = "default_true")]
    pub send_resource_indicator: bool,
    #[serde(default)]
    pub extra_authorize_params: BTreeMap<String, String>,
    #[serde(default)]
    pub client_setup: Option<String>,
    /// An OAuth client registered for Macroscope itself. When present the person goes straight to
    /// the service's sign-in; their own client (saved in Connections) still takes priority.
    #[serde(default)]
    pub bundled_client_id: Option<String>,
    #[serde(default)]
    pub bundled_client_secret: Option<String>,
    #[serde(default)]
    pub fixed_redirect_port: Option<u16>,
}
pub(crate) fn default_true() -> bool {
    true
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Registration {
    Dynamic,
    Preregistered,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Verification {
    VerifiedLive,
    VerifiedDocs,
    Unverified,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Signatures {
    pub bundle_ids: Vec<String>,
    pub browser_hosts: Vec<String>,
    pub window_titles: Vec<TitleRule>,
    pub keywords: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TitleRule {
    pub id: String,
    pub kind: TitleMatch,
    pub value: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TitleMatch {
    Suffix,
    Contains,
}
#[derive(Debug, thiserror::Error)]
#[error("invalid connector catalog")]
pub struct CatalogError;

impl Catalog {
    pub fn embedded() -> Self {
        Self::from_json(include_str!("../../../contracts/connectors.json"))
            .expect("embedded connector catalog must be valid")
    }
    pub fn from_json(json: &str) -> Result<Self, CatalogError> {
        let catalog: Self = serde_json::from_str(json).map_err(|_| CatalogError)?;
        let mut ids = HashSet::new();
        if catalog.schema != "macroscope.connectors/v1"
            || catalog.version != 1
            || catalog.connectors.iter().any(|c| {
                c.id.is_empty()
                    || !ids.insert(&c.id)
                    || c.auth.kind != "oauth"
                    || c.transport != "streamable-http"
                    || c.auth.fixed_redirect_port == Some(0)
                    || c.auth
                        .authorization_server
                        .as_ref()
                        .is_some_and(|url| crate::http::validate_url(url).is_err())
                    || c.auth.extra_authorize_params.keys().any(|key| {
                        matches!(
                            key.as_str(),
                            "response_type"
                                | "client_id"
                                | "redirect_uri"
                                | "state"
                                | "code_challenge"
                                | "code_challenge_method"
                                | "resource"
                                | "scope"
                        )
                    })
                    || (!c.mcp_url.is_empty() && crate::http::validate_url(&c.mcp_url).is_err())
            })
        {
            return Err(CatalogError);
        }
        Ok(catalog)
    }
    pub fn get(&self, id: &str) -> Option<&Connector> {
        self.connectors.iter().find(|c| c.id == id)
    }
}

#[cfg(test)]
mod bundled_client_tests {
    use super::*;

    #[test]
    fn a_catalog_entry_can_carry_a_bundled_client_and_the_embedded_one_has_none_yet() {
        let mut value: serde_json::Value =
            serde_json::from_str(include_str!("../../../contracts/connectors.json")).unwrap();
        value["connectors"][0]["auth"]["bundled_client_id"] = "app-client.example".into();
        let catalog = Catalog::from_json(&value.to_string()).unwrap();
        assert_eq!(catalog.connectors[0].auth.bundled_client_id.as_deref(), Some("app-client.example"));
        assert!(Catalog::embedded().connectors.iter().all(|c| c.auth.bundled_client_id.is_none()));
    }
}
