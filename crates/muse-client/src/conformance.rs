use crate::{BinaryInfo, BuildEvent, MspClient, MuseError, SetupState, Verification, SAFE_PATH};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    io::Write,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConformanceResult {
    Passed,
    Failed { step: String },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcceptanceRecord {
    pub binary_sha256: String,
    pub version: String,
    pub fingerprint: String,
    pub checked_at_ms: u64,
    pub result: ConformanceResult,
}
pub trait AcceptanceStore: Send + Sync {
    fn load(&self, binary_sha256: &str) -> Result<Option<AcceptanceRecord>, MuseError>;
    fn save(&self, record: &AcceptanceRecord) -> Result<(), MuseError>;
}
/// App-owned Muse state directory; never the Muse CLI's configuration directory.
pub struct JsonAcceptanceStore {
    directory: PathBuf,
}
impl JsonAcceptanceStore {
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
        }
    }
    fn path(&self, digest: &str) -> Result<PathBuf, MuseError> {
        let digest = digest
            .strip_prefix("sha256:")
            .ok_or(MuseError::AcceptanceStore)?;
        if digest.len() != 64 || !digest.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(MuseError::AcceptanceStore);
        }
        Ok(self.directory.join(format!("{digest}.json")))
    }
}
impl AcceptanceStore for JsonAcceptanceStore {
    fn load(&self, digest: &str) -> Result<Option<AcceptanceRecord>, MuseError> {
        let path = self.path(digest)?;
        match std::fs::read(path) {
            Ok(bytes) if bytes.len() <= 16 * 1024 => serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|_| MuseError::AcceptanceStore),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            _ => Err(MuseError::AcceptanceStore),
        }
    }
    fn save(&self, record: &AcceptanceRecord) -> Result<(), MuseError> {
        let path = self.path(&record.binary_sha256)?;
        let write = || -> Result<(), Box<dyn std::error::Error>> {
            std::fs::create_dir_all(&self.directory)?;
            let mut temp = tempfile::NamedTempFile::new_in(&self.directory)?;
            serde_json::to_writer(&mut temp, record)?;
            temp.write_all(b"\n")?;
            temp.as_file().sync_all()?;
            temp.persist(path)?;
            std::fs::File::open(&self.directory)?.sync_all()?;
            Ok(())
        };
        write().map_err(|_| MuseError::AcceptanceStore)
    }
}

pub async fn ensure_conformance(
    binary: &BinaryInfo,
    initialize: &Value,
    store: &dyn AcceptanceStore,
    scratch: &Path,
) -> Result<SetupState, MuseError> {
    binary.validate()?;
    let state = binary.setup_state(initialize);
    let SetupState::ConformanceRequired { fingerprint, .. } = state else {
        return Ok(state);
    };
    if let Some(record) = store.load(&binary.binary_sha256)? {
        if record.binary_sha256 == binary.binary_sha256
            && record.version == binary.version
            && record.fingerprint == fingerprint
            && record.result == ConformanceResult::Passed
        {
            return Ok(SetupState::Ready {
                fingerprint,
                verified: Verification::Conformance,
            });
        }
    }
    let report = run_echo_conformance(binary, &fingerprint, scratch).await;
    persist_conformance(binary, store, &report)?;
    Ok(report.setup_state())
}

#[derive(Debug)]
pub struct ConformanceReport {
    pub fingerprint: String,
    pub result: ConformanceResult,
    /// Scrubbed wire frames in the same format as the historical replay fixtures.
    pub frames: Vec<Value>,
}
impl ConformanceReport {
    pub fn setup_state(&self) -> SetupState {
        match &self.result {
            ConformanceResult::Passed => SetupState::Ready {
                fingerprint: self.fingerprint.clone(),
                verified: Verification::Conformance,
            },
            ConformanceResult::Failed { step } => SetupState::ConformanceRequired {
                fingerprint: self.fingerprint.clone(),
                failing_step: Some(step.clone()),
            },
        }
    }
}
pub fn persist_conformance(
    binary: &BinaryInfo,
    store: &dyn AcceptanceStore,
    report: &ConformanceReport,
) -> Result<(), MuseError> {
    binary.validate()?;
    store.save(&AcceptanceRecord {
        binary_sha256: binary.binary_sha256.clone(),
        version: binary.version.clone(),
        fingerprint: report.fingerprint.clone(),
        checked_at_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| MuseError::AcceptanceStore)?
            .as_millis()
            .try_into()
            .map_err(|_| MuseError::AcceptanceStore)?,
        result: report.result.clone(),
    })
}

/// Always uses the same pinned native executable with provider echo. There is
/// deliberately no provider argument and no unchecked public transport API.
pub async fn run_echo_conformance(
    binary: &BinaryInfo,
    fingerprint: &str,
    scratch: &Path,
) -> ConformanceReport {
    let mut report = ConformanceReport {
        fingerprint: fingerprint.into(),
        result: ConformanceResult::Passed,
        frames: Vec::new(),
    };
    let mut step = "private workspace";
    let mut client = None;
    let mut root = None;
    let result = async {
        binary.validate()?;
        let temp = tempfile::tempdir_in(scratch)?;
        let ws = temp.path().join("workspace");
        std::fs::create_dir(&ws)?;
        // No inherited git configuration, credential helpers or real HOME.
        let status = tokio::process::Command::new("/usr/bin/git")
            .env_clear()
            .env("PATH", SAFE_PATH)
            .env("HOME", temp.path())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .args(["init", "--quiet"])
            .arg(&ws)
            .status()
            .await?;
        if !status.success() {
            return Err(MuseError::Transport);
        }
        root = Some(temp);
        step = "initialize / initialized";
        let (probe, init) = MspClient::spawn_echo_probe(binary, &ws, true).await?;
        client = Some(probe);
        let probe = client.as_mut().unwrap();
        if init["schema"]["fingerprint"] != fingerprint {
            return Err(MuseError::Protocol);
        }
        step = "model/list";
        if !probe.model_list().await?["models"].is_array() {
            return Err(MuseError::Protocol);
        }
        step = "session/start";
        let session = probe.session_start(&ws).await?;
        step = "turn/start";
        probe.turn_start(&session, "hello from macroscope").await?;
        step = "turn/completed echo text";
        if probe.wait_turn().await? != "echo: hello from macroscope"
            || probe.reducer.terminal.as_deref() != Some("completed")
        {
            return Err(MuseError::Protocol);
        }
        step = "second turn/start";
        probe
            .turn_start(&session, "interrupt conformance turn")
            .await?;
        step = "turn/interrupt";
        probe.interrupt().await?;
        step = "interrupted turn terminal";
        tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                if let Some(BuildEvent::Completed { terminal }) = probe.next_event().await? {
                    // Echo may complete before the interrupt is processed. An
                    // accepted interrupt and a terminal event are both required.
                    return if terminal == "cancelled" || terminal == "completed" {
                        Ok(())
                    } else {
                        Err(MuseError::TurnFailed)
                    };
                }
            }
        })
        .await
        .map_err(|_| MuseError::Timeout)??;
        step = "unknown method JSON-RPC error";
        probe.unknown_method_probe().await?;
        step = "clean shutdown";
        probe.shutdown_inner().await?;
        step = "binary identity";
        binary.validate()?;
        Ok::<(), MuseError>(())
    }
    .await;
    if result.is_err() {
        report.result = ConformanceResult::Failed { step: step.into() };
    }
    if let Some(mut probe) = client {
        if result.is_err() {
            let _ = probe.shutdown_inner().await;
        }
        report.frames = probe.take_transcript();
    }
    if let Some(root) = root {
        // Scrub the complete temporary root, including private HOME and git
        // workspace paths, before returning anything that may be persisted.
        let prefix = root.path().to_string_lossy();
        for frame in &mut report.frames {
            if let Some(raw) = frame["raw"].as_str() {
                frame["raw"] = raw.replace(prefix.as_ref(), "<CONFORMANCE_ROOT>").into();
            }
        }
    }
    report
}
