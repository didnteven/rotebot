use serde::{Deserialize, Serialize};
use std::collections::HashSet;

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
    pub scopes: Vec<String>,
    pub docs_url: String,
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
