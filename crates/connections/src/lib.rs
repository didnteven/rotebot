//! Host-owned connection setup. Server strings are untrusted data, never instructions.
pub mod catalog;
pub mod detect;
mod http;
pub mod mcp;
pub mod oauth;
pub mod state;

pub use http::validate_url;
pub use mcp::preview_tools;
pub use state::ConnectionsService;

/// Wall-clock timestamps for persisted state (callback deadlines use monotonic time).
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}
