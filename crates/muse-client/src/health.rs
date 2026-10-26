use crate::{diagnostic, MuseError, SAFE_PATH};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::process::Command;

pub const PINNED_FINGERPRINT: &str =
    "sha256:f282ad16e21520d0cda68cb190149b86e2b062eef1dd282749360c05aa3d1082";
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetupState {
    Missing,
    UnsupportedVersion,
    FingerprintMismatch,
    Ready,
}
#[derive(Debug, Clone)]
pub struct BinaryInfo {
    pub path: PathBuf,
    pub version: String,
}
/// Inspect only installation files; never run the updating bash launcher to
/// resolve a build executable. The launcher on this machine uses sibling
/// muse-bin-<version> files; older installations may use runtime subdirectories.
pub fn resolve_native(home: &Path) -> Result<PathBuf, MuseError> {
    let mut found = Vec::new();
    fn scan(root: &Path, depth: usize, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(root) else {
            return;
        };
        for e in entries.flatten() {
            let path = e.path();
            let name = e.file_name();
            let name = name.to_string_lossy();
            if path.is_file() && name.starts_with("muse-bin-") {
                out.push(path);
            } else if depth > 0 && path.is_dir() && !e.file_type().is_ok_and(|t| t.is_symlink()) {
                scan(&path, depth - 1, out);
            }
        }
    }
    scan(&home.join(".local/bin"), 0, &mut found);
    scan(&home.join(".local/share/muse/runtime"), 3, &mut found);
    found.retain(|p| {
        use std::io::Read;
        let mut bytes = [0; 4];
        if std::fs::File::open(p)
            .and_then(|mut f| f.read_exact(&mut bytes))
            .is_err()
        {
            return false;
        }
        bytes == [0xcf, 0xfa, 0xed, 0xfe]
            || bytes == [0xca, 0xfe, 0xba, 0xbe]
            || bytes == [0x7f, b'E', b'L', b'F']
    });
    found.sort_by_key(|p| version_numbers(p.file_name().unwrap().to_string_lossy().as_ref()));
    found.pop().ok_or(MuseError::Missing)
}
fn version_numbers(s: &str) -> Vec<u64> {
    s.split(|c: char| !c.is_ascii_digit())
        .filter_map(|s| s.parse().ok())
        .collect()
}
fn minimal(cmd: &mut Command, home: &Path) {
    cmd.env_clear().env("HOME", home).env("PATH", SAFE_PATH);
    for n in ["USER", "LANG", "TMPDIR"] {
        if let Some(v) = std::env::var_os(n) {
            cmd.env(n, v);
        }
    }
    cmd.kill_on_drop(true);
}
impl BinaryInfo {
    pub async fn detect(home: &Path) -> Result<Self, MuseError> {
        let path = resolve_native(home)?;
        let mut cmd = Command::new(&path);
        minimal(&mut cmd, home);
        cmd.arg("--version");
        let o = tokio::time::timeout(Duration::from_secs(20), cmd.output())
            .await
            .map_err(|_| MuseError::Timeout)??;
        if !o.status.success() {
            return Err(MuseError::UnsupportedVersion);
        }
        let version = String::from_utf8_lossy(&o.stdout).trim().to_string();
        if !version_numbers(&version).starts_with(&[1, 4]) {
            return Err(MuseError::UnsupportedVersion);
        }
        Ok(Self { path, version })
    }
    pub fn setup_state(&self, initialize: &Value) -> SetupState {
        if !version_numbers(&self.version).starts_with(&[1, 4]) {
            SetupState::UnsupportedVersion
        } else if initialize["schema"]["version"] != 1
            || initialize["schema"]["fingerprint"] != PINNED_FINGERPRINT
        {
            SetupState::FingerprintMismatch
        } else {
            SetupState::Ready
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub enum PlanStatus {
    SignedIn { summary: Value },
    NotSignedIn,
    MuseMissing,
    Unknown(String),
}
pub fn parse_plan_status(exit: Option<i32>, stdout: &str, stderr: &str) -> PlanStatus {
    if exit == Some(0) {
        if let Ok(summary) = serde_json::from_str(stdout) {
            return PlanStatus::SignedIn { summary };
        }
    }
    if stderr.to_lowercase().contains("not logged in") {
        PlanStatus::NotSignedIn
    } else {
        PlanStatus::Unknown(diagnostic(stderr))
    }
}
pub async fn plan_status(home: &Path) -> PlanStatus {
    let Ok(path) = resolve_native(home) else {
        return PlanStatus::MuseMissing;
    };
    let mut cmd = Command::new(path);
    minimal(&mut cmd, home);
    cmd.args(["usage", "--json"]);
    match tokio::time::timeout(Duration::from_secs(20), cmd.output()).await {
        Ok(Ok(o)) => parse_plan_status(
            o.status.code(),
            &String::from_utf8_lossy(&o.stdout),
            &String::from_utf8_lossy(&o.stderr),
        ),
        _ => PlanStatus::Unknown("Muse usage check unavailable or timed out".into()),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn plan_sign_in_detection() {
        assert_eq!(
            parse_plan_status(
                Some(1),
                "",
                "muse usage: not logged in: run `muse login` first"
            ),
            PlanStatus::NotSignedIn
        );
        assert!(matches!(
            parse_plan_status(Some(0), r#"{"plan":"pro","allowance":42}"#, ""),
            PlanStatus::SignedIn { .. }
        ));
        assert!(matches!(
            parse_plan_status(Some(0), "bad json", "bogus-key-canary"),
            PlanStatus::Unknown(_)
        ));
        assert!(
            !format!("{:?}", parse_plan_status(Some(1), "", "bogus-key-canary"))
                .contains("bogus-key-canary")
        );
    }
    #[test]
    fn fingerprint_and_version_gates() {
        let b = BinaryInfo {
            path: "native".into(),
            version: "muse 1.4.13".into(),
        };
        assert_eq!(
            b.setup_state(
                &serde_json::json!({"schema":{"version":1,"fingerprint":PINNED_FINGERPRINT}})
            ),
            SetupState::Ready
        );
        assert_eq!(
            b.setup_state(&serde_json::json!({"schema":{"version":1,"fingerprint":"changed"}})),
            SetupState::FingerprintMismatch
        );
    }
}

/// Setup diagnostics may execute the launcher *only* with --version when the
/// native binary is absent. The launcher can never become a build executable.
#[derive(Debug, Clone)]
pub struct InstallationHealth {
    pub state: SetupState,
    pub native_path: Option<PathBuf>,
    pub version: Option<String>,
}
pub async fn installation_health(home: &Path, initialize: &Value) -> InstallationHealth {
    match BinaryInfo::detect(home).await {
        Ok(binary) => InstallationHealth {
            state: binary.setup_state(initialize),
            native_path: Some(binary.path),
            version: Some(binary.version),
        },
        Err(MuseError::Missing) => {
            let launcher = home.join(".local/bin/muse");
            let version = if launcher.is_file() {
                let mut cmd = Command::new(launcher);
                minimal(&mut cmd, home);
                cmd.env("MUSE_NO_AUTO_UPDATE", "1")
                    .env("MUSE_LOGIN", "0")
                    .arg("--version");
                match tokio::time::timeout(Duration::from_secs(20), cmd.output()).await {
                    Ok(Ok(o)) if o.status.success() => {
                        Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
                    }
                    _ => None,
                }
            } else {
                None
            };
            InstallationHealth {
                state: SetupState::Missing,
                native_path: None,
                version,
            }
        }
        _ => InstallationHealth {
            state: SetupState::UnsupportedVersion,
            native_path: resolve_native(home).ok(),
            version: None,
        },
    }
}
