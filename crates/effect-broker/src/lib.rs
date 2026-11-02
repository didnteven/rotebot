//! Trusted stage-once, review, create-only commit broker for one CSV per run.
//! Paths and approval calls belong to the native host, never to model output.
mod platform;

use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    XChaCha20Poly1305, XNonce,
};
use macroscope_domain::{CsvMapping, RunId, TaskContract, ToolId};
use macroscope_executor::{apply_mapping, require_columns, validate_mapping, Limits, MappingError};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use zeroize::Zeroizing;

const JOURNAL_LIMIT: u64 = 1_048_576;
const CIPHERTEXT_FILE: &str = "output.xchacha20";
const JOURNAL_FILE: &str = "journal.jsonl";

#[derive(Debug, thiserror::Error)]
pub enum BrokerError {
    #[error("generated runner failed: {0}")]
    Runner(String),
    #[error("filesystem operation failed: {0}")]
    Io(#[from] io::Error),
    #[error("serialization failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Mapping(#[from] MappingError),
    #[error("mapping violates contract: {0:?}")]
    InvalidMapping(Vec<String>),
    #[error("selected path must be an absolute regular file/directory, without symlinks")]
    InvalidSelection,
    #[error("destination must be a single .csv, .tsv, .txt, .md or .json basename")]
    InvalidBasename,
    #[error("staging authentication, length, or digest mismatch")]
    StagingTampered,
    #[error("selected input changed")]
    InputChanged,
    #[error("selected destination parent changed")]
    DestinationChanged,
    #[error("destination exists")]
    DestinationExists,
    #[error("grant revision or policy epoch revoked")]
    Revoked,
    #[error("approval does not match the staged plan")]
    ApprovalMismatch,
    #[error("atomic no-replace publication unavailable")]
    UnsupportedAtomicNoReplace,
    #[error("invalid or inconsistent journal")]
    InvalidJournal,
    #[error("publication may have occurred; reconcile the run journal")]
    OutcomeUnknown,
}

/// Construct only from a trusted native picker result. This is not a grant.
#[derive(Debug)]
pub struct SelectedFile {
    path: PathBuf,
}
impl SelectedFile {
    /// Read through the same bounded, no-symlink boundary used for staging.
    pub fn read_bounded(&self, max_bytes: u64) -> Result<Vec<u8>, BrokerError> {
        read_snapshot(self, max_bytes).map(|(bytes, _)| bytes)
    }

    pub fn from_picker(path: impl Into<PathBuf>) -> Result<Self, BrokerError> {
        let path = path.into();
        validate_selected_path(&path)?;
        platform::open(&path, false)?;
        Ok(Self { path })
    }
}

#[derive(Debug)]
pub struct SelectedRoot {
    path: PathBuf,
    identity: FileIdentity,
}
impl SelectedRoot {
    pub fn from_picker(path: impl Into<PathBuf>) -> Result<Self, BrokerError> {
        let path = path.into();
        validate_selected_path(&path)?;
        let file = platform::open(&path, true)?;
        Ok(Self {
            path,
            identity: platform::identity(&file)?,
        })
    }
}

/// A private host-only request: callers must enforce tool admission, current
/// grants and native picker provenance. A model cannot construct authority.
pub struct StageRequest {
    pub tool_id: ToolId,
    pub version_hash: String,
    pub mapping: CsvMapping,
    pub contract: TaskContract,
    pub input: SelectedFile,
    pub destination: SelectedRoot,
    pub basename: String,
    pub limits: Limits,
    pub epochs: Epochs,
    pub staging_parent: PathBuf,
    pub key: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileIdentity {
    pub dev: u64,
    pub inode: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputSnapshot {
    pub digest: String,
    pub size: u64,
    pub dev: u64,
    pub inode: u64,
    pub mtime_seconds: i64,
    pub mtime_nanoseconds: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Destination {
    pub parent: FileIdentity,
    pub basename: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateEffect {
    pub kind: String,
    pub staged_length: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Epochs {
    pub grant_revision: u64,
    pub policy_epoch: u64,
}

/// Server metadata is untrusted data and is bound to the reviewed digest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlannedAction {
    pub call: macroscope_domain::PlannedCall,
    pub service: String,
    pub resource: String,
    pub read_only_hint: Option<bool>,
    pub destructive_hint: Option<bool>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionStatus {
    Succeeded,
    Failed,
    OutcomeUnknown,
    NotRun,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionResult {
    pub action: PlannedAction,
    pub status: ActionStatus,
    /// Bounded untrusted server data. Render as text only.
    pub summary: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    pub run_id: RunId,
    pub tool_id: ToolId,
    pub version_hash: String,
    pub engine: String,
    #[serde(default)]
    pub planned_calls: Vec<PlannedAction>,
    pub mapping_hash: String,
    pub input: InputSnapshot,
    pub destination: Destination,
    /// At most one create effect, alongside the ordered connection calls.
    pub effects: Vec<CreateEffect>,
    #[serde(default = "default_create_file")]
    pub create_file: bool,
    pub limits: Limits,
    pub grant_revision: u64,
    pub policy_epoch: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Approval {
    pub plan_hash: String,
    pub approved_at_ms: u64,
}

/// Private fields prevent editing the plan after review. There is deliberately
/// no mapping, callback, or executor retained here: commit cannot rerun it.
pub struct StagedPlan {
    plan: Plan,
    plan_hash: String,
    input: SelectedFile,
    destination: SelectedRoot,
    run_dir: PathBuf,
    key: Zeroizing<[u8; 32]>,
    approval: Option<Approval>,
    staged_at_ms: u64,
    rows: u64,
}
impl StagedPlan {
    pub fn plan(&self) -> &Plan {
        &self.plan
    }
    pub fn plan_hash(&self) -> &str {
        &self.plan_hash
    }
    pub fn run_dir(&self) -> &Path {
        &self.run_dir
    }
    pub fn rows(&self) -> u64 {
        self.rows
    }
    /// Preview is the exact authenticated, bounded staged output. Render as
    /// escaped text in a host table; these bytes are never HTML.
    pub fn preview(&self) -> Result<Vec<u8>, BrokerError> {
        staged_bytes(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptStatus {
    Committed,
    Failed,
    OutcomeUnknown,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Timestamps {
    pub staged_at_ms: u64,
    pub approved_at_ms: u64,
    pub committed_at_ms: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Receipt {
    pub run_id: RunId,
    pub tool_id: ToolId,
    pub version_hash: String,
    pub plan_hash: String,
    pub input_digest: String,
    pub committed_digest: String,
    pub output_path: PathBuf,
    #[serde(default)]
    pub actions: Vec<ActionResult>,
    pub status: ReceiptStatus,
    pub timestamps: Timestamps,
}

/// Sorted JSON object keys recursively; arrays retain effect order. Compact
/// serde JSON escaping/UTF-8 is part of this encoding. Plans contain integers,
/// strings, arrays and objects, with no floating-point fields.
fn default_create_file() -> bool {
    true
}

pub fn canonical_json<T: Serialize>(value: &T) -> Result<Vec<u8>, BrokerError> {
    fn encode(value: &serde_json::Value, out: &mut Vec<u8>) -> Result<(), serde_json::Error> {
        match value {
            serde_json::Value::Object(map) => {
                out.push(b'{');
                for (i, (key, value)) in map
                    .iter()
                    .collect::<BTreeMap<_, _>>()
                    .into_iter()
                    .enumerate()
                {
                    if i > 0 {
                        out.push(b',');
                    }
                    serde_json::to_writer(&mut *out, key)?;
                    out.push(b':');
                    encode(value, out)?;
                }
                out.push(b'}');
            }
            serde_json::Value::Array(values) => {
                out.push(b'[');
                for (i, value) in values.iter().enumerate() {
                    if i > 0 {
                        out.push(b',');
                    }
                    encode(value, out)?;
                }
                out.push(b']');
            }
            _ => serde_json::to_writer(&mut *out, value)?,
        }
        Ok(())
    }
    let mut bytes = Vec::new();
    encode(&serde_json::to_value(value)?, &mut bytes)?;
    Ok(bytes)
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn validate_selected_path(path: &Path) -> Result<(), BrokerError> {
    if !path.is_absolute() || path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(BrokerError::InvalidSelection);
    }
    // Reject symlink ancestors as well as the last component. The final open
    // uses NOFOLLOW too. Destination operations subsequently use a pinned FD.
    // Only named components are checked: a Windows prefix such as `\\?\C:` names the
    // volume itself, whose metadata cannot be read ("Incorrect function").
    let mut ancestor = PathBuf::new();
    for part in path.components() {
        ancestor.push(part);
        if !matches!(part, Component::Normal(_)) {
            continue;
        }
        if fs::symlink_metadata(&ancestor)?.file_type().is_symlink() {
            return Err(BrokerError::InvalidSelection);
        }
    }
    Ok(())
}
/// Shared admission check for the core and broker, including platform restrictions.
pub fn validate_basename(name: &str) -> Result<(), BrokerError> {
    if name.is_empty()
        || name.contains(['/', '\\', '\0', ':'])
        || name == "."
        || name == ".."
        || name.len() > 255
        || ![".csv", ".tsv", ".txt", ".md", ".json"]
            .iter()
            .any(|ext| name.to_ascii_lowercase().ends_with(ext))
        || name.chars().any(char::is_control)
    {
        return Err(BrokerError::InvalidBasename);
    }
    #[cfg(windows)]
    {
        let stem = name.split('.').next().unwrap_or("").to_ascii_uppercase();
        if matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || (stem.len() == 4
                && (stem.starts_with("COM") || stem.starts_with("LPT"))
                && matches!(stem.as_bytes()[3], b'1'..=b'9'))
            || name.ends_with([' ', '.'])
        {
            return Err(BrokerError::InvalidBasename);
        }
    }
    Ok(())
}

fn read_snapshot(
    input: &SelectedFile,
    limit: u64,
) -> Result<(Vec<u8>, InputSnapshot), BrokerError> {
    validate_selected_path(&input.path)?;
    let mut file = platform::open(&input.path, false)?;
    let identity = platform::identity(&file)?;
    let size = file.metadata()?.len();
    let (seconds, nanos) = platform::mtime(&file)?;
    if size > limit {
        return Err(MappingError::Limit {
            kind: "input bytes",
            limit,
        }
        .into());
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(MappingError::Limit {
            kind: "input bytes",
            limit,
        }
        .into());
    }
    if file.metadata()?.len() != size
        || platform::mtime(&file)? != (seconds, nanos)
        || bytes.len() as u64 != size
    {
        return Err(BrokerError::InputChanged);
    }
    let input_digest = digest(&bytes);
    Ok((
        bytes,
        InputSnapshot {
            digest: input_digest,
            size,
            dev: identity.dev,
            inode: identity.inode,
            mtime_seconds: seconds,
            mtime_nanoseconds: nanos,
        },
    ))
}

pub fn stage(run: StageRequest) -> Result<StagedPlan, BrokerError> {
    if !run.basename.to_ascii_lowercase().ends_with(".csv") {
        return Err(BrokerError::InvalidBasename);
    }
    validate_mapping(&run.mapping, &run.contract).map_err(BrokerError::InvalidMapping)?;
    validate_basename(&run.basename)?;
    validate_selected_path(&run.destination.path)?;
    let root = platform::open(&run.destination.path, true)?;
    if platform::identity(&root)? != run.destination.identity {
        return Err(BrokerError::DestinationChanged);
    }
    if platform::destination_exists(&root, &run.basename)? {
        return Err(BrokerError::DestinationExists);
    }
    let (input, snapshot) = read_snapshot(&run.input, run.limits.max_input_bytes)?;
    require_columns(
        &input,
        run.mapping.skip_header_rows,
        &run.contract.input_schema,
    )?;
    let output = apply_mapping(&run.mapping, &input, &run.limits)?;
    freeze_output(run, snapshot, output, "csv-mapping", None, vec![], true)
}

/// Host-only seam: execute against the broker's exact bounded snapshot and reap
/// the worker before freezing bytes. Commit retains no callback and cannot rerun.
/// `execution_hash` must bind source, runner/profile/SDK pins and runtime limits.
pub fn stage_generated(
    run: StageRequest,
    execution_hash: String,
    execute: impl FnOnce(&[u8]) -> Result<Vec<u8>, BrokerError>,
) -> Result<StagedPlan, BrokerError> {
    stage_generated_actions(run, execution_hash, |input| Ok((execute(input)?, vec![])))
}

pub fn stage_generated_actions(
    run: StageRequest,
    execution_hash: String,
    execute: impl FnOnce(&[u8]) -> Result<(Vec<u8>, Vec<PlannedAction>), BrokerError>,
) -> Result<StagedPlan, BrokerError> {
    stage_generated_outputs(run, execution_hash, true, execute)
}

pub fn stage_generated_outputs(
    run: StageRequest,
    execution_hash: String,
    create_file: bool,
    execute: impl FnOnce(&[u8]) -> Result<(Vec<u8>, Vec<PlannedAction>), BrokerError>,
) -> Result<StagedPlan, BrokerError> {
    validate_basename(&run.basename)?;
    validate_selected_path(&run.destination.path)?;
    let root = platform::open(&run.destination.path, true)?;
    if platform::identity(&root)? != run.destination.identity {
        return Err(BrokerError::DestinationChanged);
    }
    if create_file && platform::destination_exists(&root, &run.basename)? {
        return Err(BrokerError::DestinationExists);
    }
    let (input, snapshot) = read_snapshot(&run.input, run.limits.max_input_bytes)?;
    if run.contract.input_kind == macroscope_domain::InputKind::Csv {
        require_columns(&input, 0, &run.contract.input_schema)?;
    }
    let (bytes, planned_calls) = execute(&input)?;
    if planned_calls.len() > 5
        || planned_calls.iter().any(|a| {
            !a.call.args.is_object()
                || serde_json::to_vec(&a.call.args).map_or(true, |b| b.len() > 65_536)
        })
    {
        return Err(BrokerError::Runner("invalid planned calls".into()));
    }
    if bytes.len() as u64 > run.limits.max_output_bytes {
        return Err(MappingError::Limit {
            kind: "output bytes",
            limit: run.limits.max_output_bytes,
        }
        .into());
    }
    if !bytes.is_empty() || planned_calls.is_empty() {
        validate_text_output(&run.basename, &bytes)?;
    }
    if !run.contract.output_columns.is_empty() {
        require_columns(&bytes, 0, &run.contract.output_columns)?;
    }
    let rows_in = std::str::from_utf8(&input)
        .map_err(|_| BrokerError::Runner("input must be UTF-8".into()))?
        .lines()
        .count() as u64;
    let rows_out = std::str::from_utf8(&bytes).unwrap().lines().count() as u64;
    let output = macroscope_executor::MappingOutput {
        sha256: digest(&bytes),
        bytes,
        rows_in,
        rows_out,
    };
    freeze_output(
        run,
        snapshot,
        output,
        "js-quickjs",
        Some(execution_hash),
        planned_calls,
        create_file,
    )
}

/// Shared host validation for checked and staged program output.
pub fn validate_text_output(name: &str, bytes: &[u8]) -> Result<(), BrokerError> {
    validate_basename(name)?;
    let text = std::str::from_utf8(bytes)
        .map_err(|_| BrokerError::Runner("output must be valid UTF-8".into()))?;
    if text.is_empty() {
        return Err(BrokerError::Runner("output must not be empty".into()));
    }
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".json") {
        serde_json::from_str::<serde_json::Value>(text)
            .map_err(|_| BrokerError::Runner("output is not valid JSON".into()))?;
    } else if lower.ends_with(".csv") || lower.ends_with(".tsv") {
        let mut reader = csv::ReaderBuilder::new()
            .delimiter(if lower.ends_with(".tsv") { b'\t' } else { b',' })
            .from_reader(bytes);
        reader
            .headers()
            .map_err(|_| BrokerError::Runner("invalid delimited output header".into()))?;
        for record in reader.records() {
            record.map_err(|_| {
                BrokerError::Runner("output has inconsistent CSV/TSV records".into())
            })?;
        }
    }
    Ok(())
}

fn freeze_output(
    run: StageRequest,
    snapshot: InputSnapshot,
    output: macroscope_executor::MappingOutput,
    engine: &str,
    execution_hash: Option<String>,
    planned_calls: Vec<PlannedAction>,
    create_file: bool,
) -> Result<StagedPlan, BrokerError> {
    let key = Zeroizing::new(run.key);
    let plan = Plan {
        run_id: RunId::new(),
        tool_id: run.tool_id,
        version_hash: run.version_hash,
        engine: engine.into(),
        planned_calls,
        mapping_hash: execution_hash.unwrap_or(digest(&canonical_json(&run.mapping)?)),
        input: snapshot,
        destination: Destination {
            parent: run.destination.identity.clone(),
            basename: run.basename,
        },
        create_file,
        effects: if output.bytes.is_empty() {
            vec![]
        } else {
            vec![CreateEffect {
                kind: "create".into(),
                staged_length: output.bytes.len() as u64,
                sha256: output.sha256,
            }]
        },
        limits: run.limits,
        grant_revision: run.epochs.grant_revision,
        policy_epoch: run.epochs.policy_epoch,
    };
    let plan_hash = digest(&canonical_json(&plan)?);
    validate_selected_path(&run.staging_parent)?;
    let dir = tempfile::Builder::new()
        .prefix("macroscope-run-")
        .tempdir_in(&run.staging_parent)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700))?;
    }
    let mut nonce = [0u8; 24];
    OsRng
        .try_fill_bytes(&mut nonce)
        .map_err(|_| io::Error::other("OS randomness unavailable"))?;
    let cipher = XChaCha20Poly1305::new((&*key).into());
    let encrypted = cipher
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: &output.bytes,
                aad: plan_hash.as_bytes(),
            },
        )
        .map_err(|_| BrokerError::StagingTampered)?;
    let path = dir.path().join(CIPHERTEXT_FILE);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    file.write_all(&nonce)?;
    file.write_all(&encrypted)?;
    file.sync_all()?;
    let mut permissions = file.metadata()?.permissions();
    permissions.set_readonly(true);
    file.set_permissions(permissions)?;
    if !plan.planned_calls.is_empty() {
        let mut actions_nonce = [0; 24];
        OsRng
            .try_fill_bytes(&mut actions_nonce)
            .map_err(|_| io::Error::other("OS randomness unavailable"))?;
        let encrypted_actions = cipher
            .encrypt(
                XNonce::from_slice(&actions_nonce),
                Payload {
                    msg: &canonical_json(&plan.planned_calls)?,
                    aad: plan_hash.as_bytes(),
                },
            )
            .map_err(|_| BrokerError::StagingTampered)?;
        let mut actions_file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(dir.path().join("actions.xchacha20"))?;
        actions_file.write_all(&actions_nonce)?;
        actions_file.write_all(&encrypted_actions)?;
        actions_file.sync_all()?;
    }
    file.sync_all()?;
    append_journal(
        dir.path(),
        &JournalEvent::Staged {
            plan_hash: plan_hash.clone(),
        },
    )?;
    platform::sync_directory(&platform::open(dir.path(), true)?)?;
    // Persist the run directory's entry too, so a crash cannot leave a
    // published destination with its already-synced journal unreachable.
    platform::sync_directory(&platform::open(&run.staging_parent, true)?)?;
    Ok(StagedPlan {
        plan,
        plan_hash,
        input: run.input,
        destination: run.destination,
        run_dir: dir.keep(),
        key,
        approval: None,
        staged_at_ms: now_ms(),
        rows: output.rows_out,
    })
}

/// Record deliberate host approval of precisely this staged digest. Signatures
/// and durable vault-backed approval authentication are a later integration.
pub fn approve(
    staged: &mut StagedPlan,
    plan_hash: &str,
    approval: Approval,
) -> Result<(), BrokerError> {
    if plan_hash != staged.plan_hash || approval.plan_hash != staged.plan_hash {
        return Err(BrokerError::ApprovalMismatch);
    }
    if staged.approval.is_some() {
        return Err(BrokerError::ApprovalMismatch);
    }
    append_journal(
        &staged.run_dir,
        &JournalEvent::Approved {
            approval: approval.clone(),
        },
    )?;
    staged.approval = Some(approval);
    Ok(())
}

fn staged_bytes(staged: &StagedPlan) -> Result<Vec<u8>, BrokerError> {
    let effect = staged.plan.effects.first();
    let length = effect.map_or(0, |e| e.staged_length);
    let expected = length
        .checked_add(24 + 16)
        .ok_or(BrokerError::StagingTampered)?;
    let mut file = platform::open(&staged.run_dir.join(CIPHERTEXT_FILE), false)
        .map_err(|_| BrokerError::StagingTampered)?;
    if file
        .metadata()
        .map_err(|_| BrokerError::StagingTampered)?
        .len()
        != expected
    {
        return Err(BrokerError::StagingTampered);
    }
    let mut encrypted = Vec::new();
    (&mut file)
        .take(expected.saturating_add(1))
        .read_to_end(&mut encrypted)
        .map_err(|_| BrokerError::StagingTampered)?;
    if encrypted.len() as u64 != expected || encrypted.len() < 40 {
        return Err(BrokerError::StagingTampered);
    }
    let cipher = XChaCha20Poly1305::new((&*staged.key).into());
    let bytes = cipher
        .decrypt(
            XNonce::from_slice(&encrypted[..24]),
            Payload {
                msg: &encrypted[24..],
                aad: staged.plan_hash.as_bytes(),
            },
        )
        .map_err(|_| BrokerError::StagingTampered)?;
    if bytes.len() as u64 != length || effect.is_some_and(|e| digest(&bytes) != e.sha256) {
        return Err(BrokerError::StagingTampered);
    }
    Ok(bytes)
}

/// Consume a run, publish the approved bytes once, and persist its receipt.
/// No mapping or generated-code callback is available to this API.
pub fn commit(
    staged: StagedPlan,
    approval: Approval,
    current_epochs: Epochs,
) -> Result<Receipt, BrokerError> {
    if !staged.plan.planned_calls.is_empty() {
        return Err(BrokerError::Runner(
            "connection actions require the trusted connection executor".into(),
        ));
    }
    commit_with_actions(staged, approval, current_epochs, |_| unreachable!())
}

/// The callback is trusted host code only; no generated callback is retained.
/// An intent is synced before each send. Never retries, including on transport loss.
pub fn commit_with_actions(
    staged: StagedPlan,
    approval: Approval,
    current_epochs: Epochs,
    mut execute: impl FnMut(&PlannedAction) -> (ActionStatus, String),
) -> Result<Receipt, BrokerError> {
    if staged.approval.as_ref() != Some(&approval)
        || approval.plan_hash != staged.plan_hash
        || digest(&canonical_json(&staged.plan)?) != staged.plan_hash
    {
        return Err(BrokerError::ApprovalMismatch);
    }
    let bytes = staged_bytes(&staged)?;
    let (_, snapshot) = read_snapshot(&staged.input, staged.plan.limits.max_input_bytes)
        .map_err(|_| BrokerError::InputChanged)?;
    if snapshot != staged.plan.input {
        return Err(BrokerError::InputChanged);
    }
    validate_selected_path(&staged.destination.path)
        .map_err(|_| BrokerError::DestinationChanged)?;
    let root = platform::open(&staged.destination.path, true)
        .map_err(|_| BrokerError::DestinationChanged)?;
    if platform::identity(&root)? != staged.plan.destination.parent {
        return Err(BrokerError::DestinationChanged);
    }
    if current_epochs
        != (Epochs {
            grant_revision: staged.plan.grant_revision,
            policy_epoch: staged.plan.policy_epoch,
        })
    {
        return Err(BrokerError::Revoked);
    }
    let name = &staged.plan.destination.basename;
    if staged.plan.create_file && platform::destination_exists(&root, name)? {
        return Err(BrokerError::DestinationExists);
    }
    let mut actions = Vec::new();
    let mut stopped = false;
    for (index, action) in staged.plan.planned_calls.iter().enumerate() {
        let (status, summary) = if stopped {
            (
                ActionStatus::NotRun,
                "Stopped after the previous failure".into(),
            )
        } else {
            append_journal(
                &staged.run_dir,
                &JournalEvent::ActionIntent {
                    index,
                    plan_hash: staged.plan_hash.clone(),
                },
            )?;
            let (status, summary) = execute(action);
            append_journal(
                &staged.run_dir,
                &JournalEvent::ActionResult {
                    index,
                    status: status.clone(),
                },
            )
            .map_err(|_| BrokerError::OutcomeUnknown)?;
            (status, summary.chars().take(2000).collect())
        };
        stopped |= status != ActionStatus::Succeeded;
        actions.push(ActionResult {
            action: action.clone(),
            status,
            summary,
        });
    }
    if stopped || staged.plan.effects.is_empty() || !staged.plan.create_file {
        let status = if actions
            .iter()
            .any(|a| a.status == ActionStatus::OutcomeUnknown)
        {
            ReceiptStatus::OutcomeUnknown
        } else if stopped {
            ReceiptStatus::Failed
        } else {
            ReceiptStatus::Committed
        };
        let receipt = Receipt {
            run_id: staged.plan.run_id.clone(),
            tool_id: staged.plan.tool_id.clone(),
            version_hash: staged.plan.version_hash.clone(),
            plan_hash: staged.plan_hash.clone(),
            input_digest: staged.plan.input.digest.clone(),
            committed_digest: if stopped {
                String::new()
            } else {
                digest(&bytes)
            },
            output_path: PathBuf::new(),
            actions,
            status,
            timestamps: Timestamps {
                staged_at_ms: staged.staged_at_ms,
                approved_at_ms: approval.approved_at_ms,
                committed_at_ms: now_ms(),
            },
        };
        append_journal(&staged.run_dir, &JournalEvent::ActionsFinished)?;
        return Ok(receipt);
    }
    let output_path = staged.destination.path.join(name);
    let temporary = format!(".macroscope-{}.tmp", RunId::new());
    let mut temp = platform::create_temp(&root, &temporary)?;
    let result = (|| {
        temp.write_all(&bytes)?;
        temp.sync_all()?;
        // Intent must be durable before an externally visible mutation.
        append_journal(
            &staged.run_dir,
            &JournalEvent::Intent {
                plan_hash: staged.plan_hash.clone(),
                destination: output_path.clone(),
                parent: staged.plan.destination.parent.clone(),
                length: bytes.len() as u64,
                digest: staged.plan.effects[0].sha256.clone(),
            },
        )?;
        // Recheck the named root immediately before rename as well as pinning
        // publication to its FD, so a swapped path is never used as authority.
        let current = platform::open(&staged.destination.path, true)
            .map_err(|_| BrokerError::DestinationChanged)?;
        if platform::identity(&current)? != staged.plan.destination.parent {
            return Err(BrokerError::DestinationChanged);
        }
        platform::publish(&root, &temporary, name)?;
        // Once rename succeeds, any further failure is explicitly uncertain;
        // callers must reconcile instead of trying the write again.
        (|| {
            platform::sync_directory(&root)?;
            append_journal(
                &staged.run_dir,
                &JournalEvent::Published {
                    plan_hash: staged.plan_hash.clone(),
                },
            )?;
            verify_published(
                &root,
                name,
                bytes.len() as u64,
                &staged.plan.effects[0].sha256,
            )?;
            let receipt = Receipt {
                run_id: staged.plan.run_id.clone(),
                tool_id: staged.plan.tool_id.clone(),
                version_hash: staged.plan.version_hash.clone(),
                plan_hash: staged.plan_hash.clone(),
                input_digest: staged.plan.input.digest.clone(),
                committed_digest: staged.plan.effects[0].sha256.clone(),
                output_path,
                actions,
                status: ReceiptStatus::Committed,
                timestamps: Timestamps {
                    staged_at_ms: staged.staged_at_ms,
                    approved_at_ms: approval.approved_at_ms,
                    committed_at_ms: now_ms(),
                },
            };
            let mut journal_receipt = receipt.clone();
            // Private args/server content stay in encrypted staging/database, never the journal.
            journal_receipt.actions.clear();
            append_journal(
                &staged.run_dir,
                &JournalEvent::Receipt {
                    receipt: journal_receipt,
                },
            )?;
            Ok(receipt)
        })()
        .map_err(|_: BrokerError| BrokerError::OutcomeUnknown)
    })();
    drop(temp);
    platform::remove_temp(&root, &temporary);
    result
}

fn verify_published(
    root: &File,
    name: &str,
    length: u64,
    expected: &str,
) -> Result<(), BrokerError> {
    let mut file = platform::open_child(root, name)?;
    if file.metadata()?.len() != length {
        return Err(BrokerError::OutcomeUnknown);
    }
    let mut hasher = Sha256::new();
    let mut total = 0u64;
    let mut buffer = [0u8; 8192];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        total = total.saturating_add(n as u64);
        if total > length {
            return Err(BrokerError::OutcomeUnknown);
        }
        hasher.update(&buffer[..n]);
    }
    if total != length || format!("{:x}", hasher.finalize()) != expected {
        return Err(BrokerError::OutcomeUnknown);
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
enum JournalEvent {
    ActionIntent {
        index: usize,
        plan_hash: String,
    },
    ActionResult {
        index: usize,
        status: ActionStatus,
    },
    ActionsFinished,
    Staged {
        plan_hash: String,
    },
    Approved {
        approval: Approval,
    },
    Intent {
        plan_hash: String,
        destination: PathBuf,
        parent: FileIdentity,
        length: u64,
        digest: String,
    },
    Published {
        plan_hash: String,
    },
    Receipt {
        receipt: Receipt,
    },
}
fn append_journal(run_dir: &Path, event: &JournalEvent) -> Result<(), BrokerError> {
    let path = run_dir.join(JOURNAL_FILE);
    let mut options = OpenOptions::new();
    options.append(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let mut file = options.open(path)?;
    if !file.metadata()?.is_file() || file.metadata()?.len() > JOURNAL_LIMIT {
        return Err(BrokerError::InvalidJournal);
    }
    let mut line = serde_json::to_vec(event)?;
    line.push(b'\n');
    file.write_all(&line)?;
    file.sync_all()?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReconcileOutcome {
    ReconciledPublished,
    OutcomeUnknown,
    NotPublished,
}

/// Read-only destination inspection plus append-only reconciliation markers.
/// Never creates/replaces a destination or automatically retries a write.
pub fn reconcile(run_dir: &Path) -> Result<ReconcileOutcome, BrokerError> {
    let mut file = platform::open(&run_dir.join(JOURNAL_FILE), false)?;
    if file.metadata()?.len() > JOURNAL_LIMIT {
        return Err(BrokerError::InvalidJournal);
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take(JOURNAL_LIMIT + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > JOURNAL_LIMIT {
        return Err(BrokerError::InvalidJournal);
    }
    // Ignore only an incomplete final line: a crash can interrupt an append.
    let complete = bytes.iter().rposition(|b| *b == b'\n').map_or(0, |i| i + 1);
    let events: Vec<JournalEvent> = bytes[..complete]
        .split(|b| *b == b'\n')
        .filter(|b| !b.is_empty())
        .map(serde_json::from_slice)
        .collect::<Result<_, _>>()
        .map_err(|_| BrokerError::InvalidJournal)?;
    let mut plan_hash = None;
    let mut approved = None;
    let mut intent = None;
    let mut published = false;
    let mut action_pending = None;
    let mut action_count = 0;
    let mut had_actions = false;
    for event in events {
        match event {
            JournalEvent::ActionIntent {
                index,
                plan_hash: hash,
            } if approved.as_ref() == Some(&hash)
                && action_pending.is_none()
                && index == action_count =>
            {
                had_actions = true;
                action_pending = Some(index);
            }
            JournalEvent::ActionResult { index, .. } if action_pending == Some(index) => {
                action_pending = None;
                action_count += 1;
            }
            JournalEvent::ActionsFinished if action_pending.is_none() && had_actions => {}
            JournalEvent::Staged { plan_hash: hash } if plan_hash.is_none() => {
                plan_hash = Some(hash)
            }
            JournalEvent::Approved { approval }
                if plan_hash.as_ref() == Some(&approval.plan_hash) && intent.is_none() =>
            {
                approved = Some(approval.plan_hash)
            }
            JournalEvent::Intent {
                plan_hash: hash,
                destination,
                parent,
                length,
                digest,
            } if approved.as_ref() == Some(&hash) && intent.is_none() => {
                intent = Some((hash, destination, parent, length, digest))
            }
            JournalEvent::Published { plan_hash: hash }
                if intent.as_ref().is_some_and(|i| i.0 == hash) =>
            {
                published = true
            }
            JournalEvent::Receipt { receipt }
                if published && plan_hash.as_ref() == Some(&receipt.plan_hash) => {}
            _ => return Err(BrokerError::InvalidJournal),
        }
    }
    if action_pending.is_some() {
        return Ok(ReconcileOutcome::OutcomeUnknown);
    }
    let Some((hash, destination, parent, length, digest)) = intent else {
        return Ok(if had_actions {
            ReconcileOutcome::OutcomeUnknown
        } else {
            ReconcileOutcome::NotPublished
        });
    };
    let Some(root_path) = destination.parent() else {
        return Err(BrokerError::InvalidJournal);
    };
    let Some(name) = destination.file_name().and_then(|n| n.to_str()) else {
        return Err(BrokerError::InvalidJournal);
    };
    validate_basename(name).map_err(|_| BrokerError::InvalidJournal)?;
    if validate_selected_path(root_path).is_err() {
        return Ok(ReconcileOutcome::OutcomeUnknown);
    }
    let root = match platform::open(root_path, true) {
        Ok(root) => root,
        Err(_) => return Ok(ReconcileOutcome::OutcomeUnknown),
    };
    if platform::identity(&root)? != parent {
        return Ok(ReconcileOutcome::OutcomeUnknown);
    }
    if !platform::destination_exists(&root, name)? {
        return Ok(if published || had_actions {
            ReconcileOutcome::OutcomeUnknown
        } else {
            ReconcileOutcome::NotPublished
        });
    }
    if verify_published(&root, name, length, &digest).is_err() {
        return Ok(ReconcileOutcome::OutcomeUnknown);
    }
    if !published {
        if complete != bytes.len() {
            return Ok(ReconcileOutcome::OutcomeUnknown);
        }
        append_journal(run_dir, &JournalEvent::Published { plan_hash: hash })?;
    }
    Ok(ReconcileOutcome::ReconciledPublished)
}

#[cfg(test)]
mod tests;

/// Create a new export directory beneath the selected root. All traversal and writes
/// use pinned directory descriptors; existing directories and links are refused.
#[cfg(unix)]
pub fn export_bundle(
    root: &Path,
    name: &str,
    files: &[(&str, Vec<u8>)],
) -> Result<PathBuf, BrokerError> {
    if name.is_empty() || name.contains(['/', '\\', '\0']) || name == "." || name == ".." {
        return Err(BrokerError::InvalidSelection);
    }
    let root_fd = platform::open(root, true)?;
    let dir = platform::create_directory(&root_fd, name)?;
    for (name, bytes) in files {
        if name.contains(['/', '\\', '\0']) || *name == "." || *name == ".." {
            return Err(BrokerError::InvalidSelection);
        }
        let mut file = platform::create_temp(&dir, name)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    platform::sync_directory(&dir)?;
    platform::sync_directory(&root_fd)?;
    Ok(root.join(name))
}

/// Read encrypted staged calls and recover statuses only. Never sends requests.
pub fn recover_action_results(
    run_dir: &Path,
    key: &[u8; 32],
    plan_hash: &str,
) -> Result<Vec<ActionResult>, BrokerError> {
    let path = run_dir.join("actions.xchacha20");
    if !path.exists() {
        return Ok(vec![]);
    }
    let mut file = platform::open(&path, false)?;
    if file.metadata()?.len() > JOURNAL_LIMIT {
        return Err(BrokerError::StagingTampered);
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take(JOURNAL_LIMIT + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() < 40 || bytes.len() as u64 > JOURNAL_LIMIT {
        return Err(BrokerError::StagingTampered);
    }
    let cipher = XChaCha20Poly1305::new(key.into());
    let plain = cipher
        .decrypt(
            XNonce::from_slice(&bytes[..24]),
            Payload {
                msg: &bytes[24..],
                aad: plan_hash.as_bytes(),
            },
        )
        .map_err(|_| BrokerError::StagingTampered)?;
    let actions: Vec<PlannedAction> = serde_json::from_slice(&plain)?;
    let mut results: Vec<_> = actions
        .into_iter()
        .map(|action| ActionResult {
            action,
            status: ActionStatus::NotRun,
            summary: "Not sent".into(),
        })
        .collect();
    let mut journal = platform::open(&run_dir.join(JOURNAL_FILE), false)?;
    let mut bytes = Vec::new();
    (&mut journal)
        .take(JOURNAL_LIMIT + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > JOURNAL_LIMIT {
        return Err(BrokerError::InvalidJournal);
    }
    let complete = bytes.iter().rposition(|b| *b == b'\n').map_or(0, |i| i + 1);
    for line in bytes[..complete]
        .split(|b| *b == b'\n')
        .filter(|b| !b.is_empty())
    {
        match serde_json::from_slice::<JournalEvent>(line)? {
            JournalEvent::ActionIntent { index, .. } => {
                let result = results.get_mut(index).ok_or(BrokerError::InvalidJournal)?;
                result.status = ActionStatus::OutcomeUnknown;
                result.summary = "Interrupted after intent: outcome unknown. Never retried.".into();
            }
            JournalEvent::ActionResult { index, status } => {
                let result = results.get_mut(index).ok_or(BrokerError::InvalidJournal)?;
                result.status = status;
                result.summary = "Recovered journal status; server response unavailable.".into();
            }
            _ => {}
        }
    }
    Ok(results)
}
