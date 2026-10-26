//! Bounded Muse process supervisor. No account mutation surface is exposed.
mod conformance;
mod environment;
mod exec;
mod health;
mod reducer;
mod supervisor;
pub use conformance::*;
pub use environment::*;
pub use exec::*;
pub use health::*;
pub use reducer::*;
pub use supervisor::*;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MuseError {
    #[error("Muse native binary missing")]
    Missing,
    #[error("Muse version unsupported")]
    UnsupportedVersion,
    #[error("MSP schema major version unsupported")]
    SchemaMajorMismatch,
    #[error("MSP echo conformance required for {fingerprint}: {step}")]
    ConformanceRequired { fingerprint: String, step: String },
    #[error("Muse binary disappeared or changed during the job")]
    BinaryChanged,
    #[error("Muse conformance acceptance store unavailable")]
    AcceptanceStore,
    #[error("Muse permission profile unavailable")]
    PermissionProfileUnavailable,
    #[error("Muse sandbox unavailable")]
    SandboxUnavailable,
    #[error("credential unavailable")]
    KeyUnavailable,
    #[error("Muse transport failed")]
    Transport,
    #[error("Muse deadline exceeded")]
    Timeout,
    #[error("Muse protocol rejected the request")]
    Protocol,
    #[error("Muse output framing invalid")]
    Framing,
    #[error("required exec safety flag unavailable")]
    UnsupportedExecFlags,
    #[error("Muse auth required")]
    AuthRequired,
    #[error("Muse turn failed")]
    TurnFailed,
}
impl From<std::io::Error> for MuseError {
    fn from(_: std::io::Error) -> Self {
        Self::Transport
    }
}

/// Deliberately return a fixed diagnostic rather than an untrusted error body.
/// This prevents credentials, paths and provider payloads escaping via errors.
pub fn diagnostic(raw: &str) -> String {
    let lower = raw.to_ascii_lowercase();
    if lower.contains("not logged in") {
        "Muse is not signed in"
    } else if lower.contains("rejected") && (lower.contains("key") || lower.contains("credential"))
    {
        "Muse credential rejected"
    } else if lower.contains("permission") && lower.contains("unavailable") {
        "Muse permission profile unavailable"
    } else if raw.is_empty() {
        "Muse returned no diagnostic"
    } else {
        "Muse diagnostic omitted"
    }
    .into()
}
