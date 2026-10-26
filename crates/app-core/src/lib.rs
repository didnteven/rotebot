//! The product service behind the `Backend` contract (plan §3.1). No Tauri types:
//! the desktop shell is a thin command layer over `AppCore`, and tests drive the
//! same state machine directly.
//!
//! Authority rules this module keeps: the UI never supplies paths (only handle ids
//! from native pickers), model output is a finite `CsvMapping` validated by host
//! checks, outbound packets are synthetic, and runs commit the exact staged bytes
//! the user approved.

pub mod checks;
pub mod drafts;
pub mod dto;

use dto::*;
use macroscope_builder::{
    BuildBudget, BuildError, BuildRoute, CandidateValidator, MappingBuilder, MuseConfig,
    SyntheticSamples, ValidationError,
};
use macroscope_domain::{CsvMapping, TaskContract, ToolId};
use macroscope_effect_broker::{
    self as broker, Approval, BrokerError, Epochs, SelectedFile, SelectedRoot, StageRequest,
    StagedPlan,
};
use macroscope_executor::{Limits, MappingError};
use macroscope_model_gateway::{GatewayError, MetaResponsesClient, DEFAULT_MODEL};
use macroscope_muse_client::{BinaryInfo, CredentialRoute, PlanStatus};
use macroscope_policy::{EgressPolicy, EgressState as PolicyState};
use macroscope_storage::db::{self, Db};
use macroscope_vault::{validate_key, Secret, SecretRef, SecretStore};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

pub const API_KEY_REF: &str = "meta-api-key";
const DB_KEY_REF: &str = "db-key";
const STAGING_KEY_REF: &str = "staging-key";
const BUILD_BUDGET_MICROS: i64 = 2_000_000;
const SAMPLE_CSV: &[u8] = include_bytes!("../../../fixtures/orders/input.csv");
const SAMPLE_TASK: &str = include_str!("../../../fixtures/orders/task.json");

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("{0} was not found")]
    NotFound(&'static str),
    #[error("{0}")]
    Invalid(String),
    #[error("storage error: {0}")]
    Storage(String),
    #[error("secure storage error: {0}")]
    Vault(String),
    #[error("{0}")]
    Internal(String),
}

impl CoreError {
    /// Stable code for the UI; messages are already safe to show.
    pub fn code(&self) -> &'static str {
        match self {
            CoreError::NotFound(_) => "not_found",
            CoreError::Invalid(_) => "invalid",
            CoreError::Storage(_) => "storage",
            CoreError::Vault(_) => "vault",
            CoreError::Internal(_) => "internal",
        }
    }
}

impl From<db::DbError> for CoreError {
    fn from(e: db::DbError) -> Self {
        CoreError::Storage(e.to_string())
    }
}

pub type CoreResult<T> = Result<T, CoreError>;
type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Muse plan status, injectable for tests. The default runs the read-only
/// `muse usage --json` on the native binary (ADR 0001 §1).
pub trait MuseProbe: Send + Sync {
    fn plan_status(&self) -> BoxFuture<'_, PlanStatus>;
    fn version(&self) -> Option<String>;
}

pub struct NativeMuse {
    pub home: PathBuf,
}

impl MuseProbe for NativeMuse {
    fn plan_status(&self) -> BoxFuture<'_, PlanStatus> {
        Box::pin(macroscope_muse_client::plan_status(&self.home))
    }
    fn version(&self) -> Option<String> {
        let path = macroscope_muse_client::resolve_native(&self.home).ok()?;
        let name = path.file_name()?.to_string_lossy().into_owned();
        let rest = name.strip_prefix("muse-bin-")?;
        Some(rest.split('-').next().unwrap_or(rest).to_string())
    }
}

pub struct CoreConfig {
    pub data_dir: PathBuf,
    pub vault: Arc<dyn SecretStore>,
    pub gateway_base_url: String,
    /// The user's real home: Muse plan sign-in and native binary resolution.
    pub home: PathBuf,
    pub muse: Arc<dyn MuseProbe>,
    pub app_version: String,
    pub platform: String,
    pub now_ms: Arc<dyn Fn() -> i64 + Send + Sync>,
}

impl CoreConfig {
    pub fn system_clock() -> Arc<dyn Fn() -> i64 + Send + Sync> {
        Arc::new(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0)
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PickKind {
    File,
    Dir,
}

struct Picked {
    path: PathBuf,
    kind: PickKind,
    handle: FileHandle,
}

struct BuildSlot {
    state: BuildState,
    tx: broadcast::Sender<BuildState>,
    task: Option<JoinHandle<()>>,
}

struct StagedSlot {
    staged: StagedPlan,
    dto: StagedRun,
    tool_name: String,
}

struct Inner {
    cfg: CoreConfig,
    db: Db,
    staging_key: [u8; 32],
    policy: EgressPolicy,
    handles: Mutex<HashMap<String, Picked>>,
    builds: Mutex<HashMap<String, BuildSlot>>,
    staged: Mutex<HashMap<String, StagedSlot>>,
    plan_cache: Mutex<Option<(i64, PlanStatus)>>,
    last_key_test: Mutex<Option<KeyTestResult>>,
}

#[derive(Clone)]
pub struct AppCore {
    inner: Arc<Inner>,
}

fn random_key() -> CoreResult<[u8; 32]> {
    use rand_core::{OsRng, RngCore};
    let mut key = [0u8; 32];
    OsRng
        .try_fill_bytes(&mut key)
        .map_err(|_| CoreError::Internal("secure random generation failed".into()))?;
    Ok(key)
}

fn new_id(prefix: &str) -> String {
    format!("{prefix}_{}", uuid::Uuid::new_v4().simple())
}

fn vault_key(vault: &dyn SecretStore, name: &str) -> CoreResult<[u8; 32]> {
    let reference = SecretRef::new(name).map_err(|e| CoreError::Vault(e.to_string()))?;
    if let Some(secret) = vault
        .get(&reference)
        .map_err(|e| CoreError::Vault(e.to_string()))?
    {
        if let Ok(key) = <[u8; 32]>::try_from(secret.expose()) {
            return Ok(key);
        }
    }
    let key = random_key()?;
    vault
        .set(&reference, Secret::new(key.to_vec()))
        .map_err(|e| CoreError::Vault(e.to_string()))?;
    Ok(key)
}

fn hash_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

fn map_run_failure(e: BrokerError) -> RunFailure {
    match e {
        BrokerError::Mapping(MappingError::Row { row_number, column, reason, .. }) => RunFailure::InputInvalid {
            message: format!("Row {row_number}, column {column}: {reason}. Nothing was written."),
            row: Some(row_number),
            column: Some(column),
        },
        BrokerError::Mapping(MappingError::MissingColumns(cols)) => RunFailure::InputInvalid {
            message: format!("This CSV is missing {}. Nothing was written.", cols.join(", ")),
            row: None,
            column: cols.first().cloned(),
        },
        BrokerError::Mapping(other) => RunFailure::InputInvalid {
            message: format!("{other}. Nothing was written."),
            row: None,
            column: None,
        },
        BrokerError::InputChanged => RunFailure::InputChanged {
            message: "The input file changed after you reviewed it. Stage it again.".into(),
        },
        BrokerError::DestinationExists => RunFailure::DestinationExists {
            message: "A file with that name already exists. Choose another name; existing files are never replaced.".into(),
        },
        BrokerError::DestinationChanged => RunFailure::DestinationChanged {
            message: "The output folder changed after you reviewed it. Choose it again.".into(),
        },
        BrokerError::Revoked => RunFailure::Revoked {
            message: "Permissions changed since staging. Stage the run again.".into(),
        },
        BrokerError::InvalidSelection => RunFailure::InputInvalid {
            message: "Choose a regular file and folder (links and special files are refused).".into(),
            row: None,
            column: None,
        },
        BrokerError::InvalidBasename => RunFailure::InputInvalid {
            message: "Use an output name ending in .csv, without folders.".into(),
            row: None,
            column: None,
        },
        other => RunFailure::Internal { message: other.to_string() },
    }
}

struct NewTool<'a> {
    name: &'a str,
    contract: &'a TaskContract,
    mapping: &'a CsvMapping,
    summary: Vec<String>,
    build_ref: &'a str,
    model: &'a str,
    sample: bool,
}

struct HostValidator;

impl CandidateValidator for HostValidator {
    fn validate(
        &self,
        contract: &TaskContract,
        mapping: &CsvMapping,
    ) -> Result<(), ValidationError> {
        macroscope_executor::validate_mapping(mapping, contract).map_err(|_| ValidationError)
    }
}

impl AppCore {
    pub fn new(cfg: CoreConfig) -> CoreResult<AppCore> {
        for dir in ["db", "runs", "tools", "samples", "builds", "muse/home"] {
            std::fs::create_dir_all(cfg.data_dir.join(dir))
                .map_err(|e| CoreError::Internal(e.to_string()))?;
        }
        // Canonical data dir: the broker refuses symlinked ancestors (macOS /var -> /private/var).
        let data_dir =
            std::fs::canonicalize(&cfg.data_dir).map_err(|e| CoreError::Internal(e.to_string()))?;
        let cfg = CoreConfig { data_dir, ..cfg };
        let db_key = vault_key(&*cfg.vault, DB_KEY_REF)?;
        let staging_key = vault_key(&*cfg.vault, STAGING_KEY_REF)?;
        let db = Db::open(&cfg.data_dir.join("db/state.db"), &db_key)?;
        let core = AppCore {
            inner: Arc::new(Inner {
                cfg,
                db,
                staging_key,
                policy: EgressPolicy::default(),
                handles: Mutex::new(HashMap::new()),
                builds: Mutex::new(HashMap::new()),
                staged: Mutex::new(HashMap::new()),
                plan_cache: Mutex::new(None),
                last_key_test: Mutex::new(None),
            }),
        };
        // The policy starts closed; reopen cloud egress unless the user chose Local only.
        if core.db().setting("local_only")? != Some(json!(true)) {
            core.inner
                .policy
                .allow_cloud()
                .map_err(|e| CoreError::Internal(e.to_string()))?;
        }
        core.recover()?;
        Ok(core)
    }

    fn now(&self) -> i64 {
        (self.inner.cfg.now_ms)()
    }

    fn db(&self) -> &Db {
        &self.inner.db
    }

    fn activity(&self, kind: &str, title: impl Into<String>, detail: Option<String>) {
        let _ = self.db().add_activity(&db::ActivityRow {
            id: new_id("act"),
            at_ms: self.now(),
            kind: kind.into(),
            title: title.into(),
            detail,
        });
    }

    /// Startup recovery (plan §3.2): builds that were running are marked interrupted,
    /// expired outbound bodies are purged, and runs with a journal but no receipt are
    /// reconciled without ever retrying a write.
    fn recover(&self) -> CoreResult<()> {
        let now = self.now();
        self.db().purge_expired_outbound(now)?;
        for id in self.db().unfinished_builds()? {
            if let Some(mut row) = self.db().build(&id)? {
                row.phase = BuildPhase::Failed.as_str().into();
                if let Ok(mut state) = serde_json::from_value::<BuildState>(row.state.clone()) {
                    state.phase = BuildPhase::Failed;
                    state.failure = Some(BuildFailure {
                        kind: BuildFailureKind::Internal,
                        message:
                            "Macroscope quit during this build. Start it again; your draft is kept."
                                .into(),
                    });
                    row.state = serde_json::to_value(&state).unwrap_or(Value::Null);
                }
                row.updated_ms = now;
                self.db().put_build(&row)?;
            }
        }
        for mut run in self.db().unreceipted_runs()? {
            let Some(dir) = run.run_dir.clone() else {
                continue;
            };
            let dir = PathBuf::from(dir);
            if !dir.join("journal.jsonl").exists() {
                continue;
            }
            match broker::reconcile(&dir) {
                Ok(broker::ReconcileOutcome::ReconciledPublished) => {
                    run.state = "reconciled".into();
                    self.activity(
                        "run",
                        "A run that was interrupted had finished writing its file",
                        None,
                    );
                }
                Ok(broker::ReconcileOutcome::OutcomeUnknown) | Err(_) => {
                    run.state = "outcome_unknown".into();
                    self.activity(
                        "run",
                        "A run was interrupted and its result is unknown",
                        Some("Check the output folder before running it again; nothing will be overwritten.".into()),
                    );
                }
                Ok(broker::ReconcileOutcome::NotPublished) => run.state = "not_published".into(),
            }
            self.db().put_run(&run)?;
        }
        Ok(())
    }

    pub fn app_info(&self) -> AppInfo {
        AppInfo {
            version: self.inner.cfg.app_version.clone(),
            platform: self.inner.cfg.platform.clone(),
            mock: false,
        }
    }

    // ---------- handles ----------

    fn register(&self, path: &Path, kind: PickKind) -> CoreResult<FileHandle> {
        let canonical = std::fs::canonicalize(path)
            .map_err(|_| CoreError::Invalid("That item could not be opened.".into()))?;
        let meta = std::fs::metadata(&canonical)
            .map_err(|_| CoreError::Invalid("That item could not be opened.".into()))?;
        match kind {
            PickKind::File if !meta.is_file() => {
                return Err(CoreError::Invalid("Choose a file.".into()))
            }
            PickKind::Dir if !meta.is_dir() => {
                return Err(CoreError::Invalid("Choose a folder.".into()))
            }
            _ => {}
        }
        let handle = FileHandle {
            handle_id: new_id("fh"),
            display_name: canonical
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            display_path: canonical.to_string_lossy().into_owned(),
        };
        lock(&self.inner.handles).insert(
            handle.handle_id.clone(),
            Picked {
                path: canonical,
                kind,
                handle: handle.clone(),
            },
        );
        Ok(handle)
    }

    /// Registers a file chosen in the native picker. Only the shell calls this.
    pub fn register_picked_file(&self, path: &Path) -> CoreResult<FileHandle> {
        self.register(path, PickKind::File)
    }

    /// Registers a folder chosen in the native picker. Only the shell calls this.
    pub fn register_picked_dir(&self, path: &Path) -> CoreResult<FileHandle> {
        self.register(path, PickKind::Dir)
    }

    fn handle_path(&self, handle_id: &str, kind: PickKind) -> CoreResult<(PathBuf, FileHandle)> {
        let handles = lock(&self.inner.handles);
        let picked = handles
            .get(handle_id)
            .ok_or(CoreError::NotFound("selected file"))?;
        if picked.kind != kind {
            return Err(CoreError::Invalid(
                "That selection is the wrong kind of item.".into(),
            ));
        }
        Ok((picked.path.clone(), picked.handle.clone()))
    }

    fn read_limited(path: &Path) -> CoreResult<Vec<u8>> {
        let meta = std::fs::metadata(path).map_err(|e| CoreError::Invalid(e.to_string()))?;
        if meta.len() > drafts::MAX_INPUT_BYTES {
            return Err(CoreError::Invalid(
                "This CSV is larger than 10 MB, the current limit.".into(),
            ));
        }
        std::fs::read(path).map_err(|e| CoreError::Invalid(e.to_string()))
    }

    pub fn preview_handle(&self, handle_id: &str) -> CoreResult<CsvPreview> {
        let (path, handle) = self.handle_path(handle_id, PickKind::File)?;
        let bytes = Self::read_limited(&path)?;
        drafts::preview(handle, &bytes).map_err(CoreError::Invalid)
    }

    pub fn load_sample_csv(&self) -> CoreResult<CsvPreview> {
        let path = self.inner.cfg.data_dir.join("samples/orders.csv");
        if std::fs::read(&path).ok().as_deref() != Some(SAMPLE_CSV) {
            std::fs::write(&path, SAMPLE_CSV).map_err(|e| CoreError::Internal(e.to_string()))?;
        }
        let handle = self.register(&path, PickKind::File)?;
        self.preview_handle(&handle.handle_id)
    }

    /// A folder for sample outputs, so the sample tool can run without a picker.
    pub fn sample_output_dir(&self) -> CoreResult<FileHandle> {
        let dir = self.inner.cfg.data_dir.join("samples/output");
        std::fs::create_dir_all(&dir).map_err(|e| CoreError::Internal(e.to_string()))?;
        self.register(&dir, PickKind::Dir)
    }

    // ---------- drafts ----------

    fn save_draft(&self, draft: &TaskDraft) -> CoreResult<()> {
        self.db().put_draft(
            &draft.draft_id,
            draft.revision as i64,
            &serde_json::to_value(draft).map_err(|e| CoreError::Internal(e.to_string()))?,
            self.now(),
        )?;
        Ok(())
    }

    fn load_draft(&self, draft_id: &str) -> CoreResult<TaskDraft> {
        let (_, value) = self
            .db()
            .draft(draft_id)?
            .ok_or(CoreError::NotFound("draft"))?;
        serde_json::from_value(value).map_err(|e| CoreError::Internal(e.to_string()))
    }

    /// Connector ids a typed description mentions (ADR 0001 §2). Only the id and a
    /// count are recorded; the text itself is not stored by detection.
    fn mentioned_connectors(&self, text: &str) -> Vec<String> {
        let catalog = macroscope_connections::catalog::Catalog::embedded();
        let mut ids: Vec<String> = macroscope_connections::detect::detect(
            &catalog,
            macroscope_connections::detect::Signal::TaskText {
                text: text.to_string(),
            },
        )
        .into_iter()
        .map(|d| d.connector_id)
        .collect();
        ids.sort();
        ids.dedup();
        let now = self.now();
        for id in &ids {
            let _ = self.db().record_detection(id, "task_text", 1, now);
        }
        ids
    }

    pub fn draft_task(&self, input_handle: &str, description: &str) -> CoreResult<TaskDraft> {
        let (path, handle) = self.handle_path(input_handle, PickKind::File)?;
        let bytes = Self::read_limited(&path)?;
        let parsed = drafts::parse(&bytes, 200).map_err(CoreError::Invalid)?;
        let preview = drafts::preview(handle.clone(), &bytes).map_err(CoreError::Invalid)?;
        let proposal = drafts::propose(&parsed);
        let stem = Path::new(&handle.display_name)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "output".into());
        let mut draft = TaskDraft {
            draft_id: new_id("draft"),
            revision: 1,
            goal: description.trim().chars().take(2000).collect(),
            input: preview,
            summary: vec![],
            mapping: proposal.mapping,
            questions: proposal.questions,
            mentioned_connectors: self.mentioned_connectors(description),
            output_name: format!("{stem}_ready.csv"),
            ready_to_build: false,
            blockers: vec![],
        };
        drafts::refresh(&mut draft);
        self.save_draft(&draft)?;
        Ok(draft)
    }

    pub fn answer_question(
        &self,
        draft_id: &str,
        question_id: &str,
        answer: &str,
    ) -> CoreResult<TaskDraft> {
        let mut draft = self.load_draft(draft_id)?;
        let question = draft
            .questions
            .iter_mut()
            .find(|q| q.id == question_id)
            .ok_or(CoreError::NotFound("question"))?;
        if !question.options.iter().any(|o| o.value == answer) {
            return Err(CoreError::Invalid(
                "Choose one of the listed answers.".into(),
            ));
        }
        question.answer = Some(answer.into());
        if question_id == drafts::DATE_QUESTION {
            let column = question.column.clone();
            let from = drafts::parse_date_answer(answer)
                .ok_or_else(|| CoreError::Invalid("Unknown date format.".into()))?;
            for c in draft
                .mapping
                .output_columns
                .iter_mut()
                .filter(|c| c.source == column)
            {
                if let macroscope_domain::Transform::Date { from: f, .. } = &mut c.transform {
                    *f = from;
                }
            }
        }
        draft.revision += 1;
        drafts::refresh(&mut draft);
        self.save_draft(&draft)?;
        Ok(draft)
    }

    pub fn update_mapping(
        &self,
        draft_id: &str,
        mapping: CsvMapping,
        output_name: &str,
    ) -> CoreResult<TaskDraft> {
        let mut draft = self.load_draft(draft_id)?;
        draft.mapping = mapping;
        draft.output_name = output_name.trim().into();
        draft.revision += 1;
        drafts::refresh(&mut draft);
        self.save_draft(&draft)?;
        Ok(draft)
    }

    // ---------- model access ----------

    fn api_key_ref() -> SecretRef {
        SecretRef::new(API_KEY_REF).expect("static secret ref is valid")
    }

    fn api_key_present(&self) -> bool {
        matches!(self.inner.cfg.vault.get(&Self::api_key_ref()), Ok(Some(_)))
    }

    async fn plan_status(&self, refresh: bool) -> PlanStatus {
        let now = self.now();
        if !refresh {
            if let Some((at, status)) = lock(&self.inner.plan_cache).clone() {
                if now - at < 60_000 {
                    return status;
                }
            }
        }
        let status = self.inner.cfg.muse.plan_status().await;
        *lock(&self.inner.plan_cache) = Some((now, status.clone()));
        status
    }

    fn setting_route(&self) -> Option<ModelRoute> {
        self.db()
            .setting("selected_route")
            .ok()
            .flatten()
            .and_then(|v| serde_json::from_value(v).ok())
    }

    fn muse_containment_accepted(&self) -> bool {
        self.db().setting("muse_unverified_ok").ok().flatten() == Some(json!(true))
    }

    pub async fn model_access(&self) -> ModelAccessStatus {
        self.model_access_inner(false).await
    }

    pub async fn refresh_muse_status(&self) -> ModelAccessStatus {
        self.model_access_inner(true).await
    }

    async fn model_access_inner(&self, refresh: bool) -> ModelAccessStatus {
        let present = self.api_key_present();
        let fingerprint = self
            .db()
            .setting("api_key_fingerprint")
            .ok()
            .flatten()
            .and_then(|v| v.as_str().map(String::from));
        let version = self.inner.cfg.muse.version();
        let plan = self.plan_status(refresh).await;
        let muse = match (&plan, version.clone()) {
            (PlanStatus::MuseMissing, _) | (_, None) => MuseState::Missing,
            (PlanStatus::SignedIn { summary }, Some(v)) => MuseState::SignedIn {
                version: v,
                plan_summary: summary_text(summary),
            },
            (PlanStatus::NotSignedIn, Some(v)) => MuseState::NotSignedIn { version: v },
            (PlanStatus::Unknown(message), v) => MuseState::Unknown {
                version: v,
                message: message.clone(),
            },
        };
        let signed_in = matches!(muse, MuseState::SignedIn { .. });
        let egress = match self.inner.policy.state() {
            PolicyState::CloudAllowed => EgressState::CloudAllowed,
            PolicyState::Stopping => EgressState::Stopping,
            PolicyState::LocalOnly => EgressState::LocalOnly,
            PolicyState::KeyUnavailable => EgressState::KeyUnavailable,
        };
        let available = |r: ModelRoute| match r {
            ModelRoute::ApiKey => present,
            ModelRoute::MusePlan => signed_in,
            ModelRoute::OfflineDemo => true,
        };
        let selected = self.setting_route();
        let (effective, blocked) = if egress != EgressState::CloudAllowed {
            (
                Some(ModelRoute::OfflineDemo),
                Some(match egress {
                    EgressState::Stopping => "Stopping cloud work…".to_string(),
                    EgressState::KeyUnavailable => {
                        "Your Meta API key was rejected. Replace it in Settings.".into()
                    }
                    _ => "Local only is on: cloud models are paused. Builds use the offline demo."
                        .into(),
                }),
            )
        } else {
            match selected {
                Some(r) if available(r) => (Some(r), None),
                Some(ModelRoute::ApiKey) => (
                    Some(ModelRoute::OfflineDemo),
                    Some("Add a Meta API key to build with it.".into()),
                ),
                Some(ModelRoute::MusePlan) => (
                    Some(ModelRoute::OfflineDemo),
                    Some("Sign in to Muse Code in Terminal (muse login), then check again.".into()),
                ),
                _ if present => (Some(ModelRoute::ApiKey), None),
                _ if signed_in => (Some(ModelRoute::MusePlan), None),
                _ => (
                    Some(ModelRoute::OfflineDemo),
                    Some(
                        "Add a Meta API key or sign in to Muse Code to build with a model.".into(),
                    ),
                ),
            }
        };
        ModelAccessStatus {
            api_key: ApiKeyStatus {
                present,
                fingerprint,
                last_test: lock(&self.inner.last_key_test).clone(),
            },
            muse,
            selected_route: selected,
            effective_route: effective,
            blocked_reason: blocked,
            egress,
            model: DEFAULT_MODEL.into(),
            muse_unverified_ok: self.muse_containment_accepted(),
        }
    }

    pub async fn save_api_key(&self, raw: &str) -> CoreResult<ModelAccessStatus> {
        let (secret, fingerprint) = validate_key(raw)
            .map_err(|_| CoreError::Invalid("That key is empty or has line breaks.".into()))?;
        self.inner
            .cfg
            .vault
            .set(&Self::api_key_ref(), secret)
            .map_err(|e| CoreError::Vault(e.to_string()))?;
        let label = format!("Key ending …{}", fingerprint.last_four);
        self.db()
            .set_setting("api_key_fingerprint", &json!(label))?;
        *lock(&self.inner.last_key_test) = None;
        if self.inner.policy.state() == PolicyState::KeyUnavailable {
            let _ = self.inner.policy.allow_cloud();
        }
        self.activity("settings", "Meta API key saved", None);
        Ok(self.model_access().await)
    }

    pub async fn remove_api_key(&self) -> CoreResult<ModelAccessStatus> {
        self.inner
            .cfg
            .vault
            .delete(&Self::api_key_ref())
            .map_err(|e| CoreError::Vault(e.to_string()))?;
        self.db().set_setting("api_key_fingerprint", &Value::Null)?;
        *lock(&self.inner.last_key_test) = None;
        self.activity("settings", "Meta API key removed", None);
        Ok(self.model_access().await)
    }

    pub async fn test_api_key(&self) -> CoreResult<ModelAccessStatus> {
        let key = self
            .inner
            .cfg
            .vault
            .get(&Self::api_key_ref())
            .map_err(|e| CoreError::Vault(e.to_string()))?
            .ok_or_else(|| CoreError::Invalid("Add a key first.".into()))?;
        let now = self.now();
        let client = MetaResponsesClient::new(self.inner.cfg.gateway_base_url.clone());
        let result = match client.catalog_check(&key).await {
            Ok(_) => KeyTestResult::Ok { checked_at_ms: now },
            Err(GatewayError::AuthRejected) => KeyTestResult::Rejected { checked_at_ms: now },
            Err(GatewayError::BillingRequired) => {
                KeyTestResult::BillingRequired { checked_at_ms: now }
            }
            Err(GatewayError::Quota { retry_after }) => KeyTestResult::Quota {
                checked_at_ms: now,
                retry_at_ms: retry_after.map(|d| now + d.as_millis() as i64),
            },
            Err(other) => KeyTestResult::Network {
                checked_at_ms: now,
                message: other.to_string(),
            },
        };
        *lock(&self.inner.last_key_test) = Some(result);
        Ok(self.model_access().await)
    }

    pub async fn select_route(&self, route: ModelRoute) -> CoreResult<ModelAccessStatus> {
        self.db().set_setting("selected_route", &json!(route))?;
        self.activity(
            "settings",
            format!("Build route set to {}", route.label()),
            None,
        );
        Ok(self.model_access().await)
    }

    /// Owner opt-in for Muse plan builds before the native-tool containment gate has
    /// evidence (plan §4.3 / P7). Shell and writes stay disabled either way.
    pub async fn set_muse_unverified_ok(&self, ok: bool) -> CoreResult<ModelAccessStatus> {
        self.db().set_setting("muse_unverified_ok", &json!(ok))?;
        Ok(self.model_access().await)
    }

    pub async fn set_local_only(&self, local_only: bool) -> CoreResult<ModelAccessStatus> {
        self.db().set_setting("local_only", &json!(local_only))?;
        if local_only {
            let builds: Vec<String> = lock(&self.inner.builds)
                .iter()
                .filter(|(_, s)| {
                    !s.state.phase.is_terminal() && s.state.route != ModelRoute::OfflineDemo
                })
                .map(|(id, _)| id.clone())
                .collect();
            for id in builds {
                let _ = self.stop_build(&id);
            }
            if let Ok(barrier) = self.inner.policy.begin_local_only() {
                // Every admitted job has been stopped above; their guards drop and ack.
                let _ = barrier.is_resolved();
            }
            self.activity("settings", "Local only turned on", None);
        } else {
            self.inner
                .policy
                .allow_cloud()
                .map_err(|e| CoreError::Internal(e.to_string()))?;
            self.activity("settings", "Cloud models turned on", None);
        }
        Ok(self.model_access().await)
    }

    // ---------- outbound preview and builds ----------

    fn route_destination(route: ModelRoute) -> &'static str {
        match route {
            ModelRoute::ApiKey => "Meta API (api.meta.ai)",
            ModelRoute::MusePlan => "Muse Code with your Muse plan",
            ModelRoute::OfflineDemo => "Nothing leaves this Mac",
        }
    }

    fn packet_text(contract: &TaskContract) -> String {
        let samples = SyntheticSamples::from_contract(contract, 5);
        // The builder sends this same template; samples are host-made, never user rows.
        let packet = json!({
            "template": "macroscope-csv-mapping/v1",
            "untrusted_contract_data": {
                "columns": contract.input_schema,
                "output_columns": contract.output_columns,
                "confirmed_rules": contract.mapping,
                "error_policy": contract.error_policy,
            },
            "synthetic_example_rows": format!("{samples:?}"),
        });
        serde_json::to_string_pretty(&packet).unwrap_or_default()
    }

    pub async fn outbound_preview(&self, draft_id: &str) -> CoreResult<OutboundPreview> {
        let draft = self.load_draft(draft_id)?;
        let access = self.model_access().await;
        let route = access.effective_route.unwrap_or(ModelRoute::OfflineDemo);
        let contract = drafts::contract_for(&draft);
        let (payload_text, estimate) = match route {
            ModelRoute::OfflineDemo => (
                "Nothing leaves this Mac. The offline demo maps columns without a model."
                    .to_string(),
                Some(0),
            ),
            ModelRoute::ApiKey => (Self::packet_text(&contract), Some(20_000)),
            ModelRoute::MusePlan => (Self::packet_text(&contract), None),
        };
        Ok(OutboundPreview {
            draft_id: draft_id.into(),
            route,
            model: if route == ModelRoute::OfflineDemo {
                "No model".into()
            } else {
                DEFAULT_MODEL.into()
            },
            destination: Self::route_destination(route).into(),
            payload_text,
            contains_real_rows: false,
            estimated_cost_micros: estimate,
            budget_micros: BUILD_BUDGET_MICROS,
        })
    }

    pub fn subscribe_build(&self, build_id: &str) -> CoreResult<broadcast::Receiver<BuildState>> {
        lock(&self.inner.builds)
            .get(build_id)
            .map(|s| s.tx.subscribe())
            .ok_or(CoreError::NotFound("build"))
    }

    pub fn build_state(&self, build_id: &str) -> CoreResult<BuildState> {
        if let Some(slot) = lock(&self.inner.builds).get(build_id) {
            return Ok(slot.state.clone());
        }
        let row = self
            .db()
            .build(build_id)?
            .ok_or(CoreError::NotFound("build"))?;
        serde_json::from_value(row.state).map_err(|e| CoreError::Internal(e.to_string()))
    }

    fn update_build(&self, build_id: &str, f: impl FnOnce(&mut BuildState)) {
        let snapshot = {
            let mut builds = lock(&self.inner.builds);
            let Some(slot) = builds.get_mut(build_id) else {
                return;
            };
            if slot.state.phase.is_terminal() {
                return; // stopped/failed builds never resurrect
            }
            f(&mut slot.state);
            let _ = slot.tx.send(slot.state.clone());
            slot.state.clone()
        };
        let now = self.now();
        let _ = self.db().put_build(&db::BuildRow {
            id: snapshot.build_id.clone(),
            draft_id: snapshot.draft_id.clone(),
            route: snapshot.route.as_str().into(),
            phase: snapshot.phase.as_str().into(),
            reserved_micros: if snapshot.route == ModelRoute::OfflineDemo {
                0
            } else {
                BUILD_BUDGET_MICROS
            },
            spent_micros: snapshot.spent_micros,
            state: serde_json::to_value(&snapshot).unwrap_or(Value::Null),
            created_ms: snapshot.events.first().map(|e| e.at_ms).unwrap_or(now),
            updated_ms: now,
        });
    }

    fn push_phase(&self, build_id: &str, phase: BuildPhase, message: impl Into<String>) {
        let now = self.now();
        let message = message.into();
        self.update_build(build_id, |s| {
            s.phase = phase;
            s.events.push(BuildEvent {
                build_id: s.build_id.clone(),
                seq: s.events.len() as u32 + 1,
                phase,
                message,
                at_ms: now,
            });
        });
    }

    fn fail_build(&self, build_id: &str, kind: BuildFailureKind, message: impl Into<String>) {
        let message = message.into();
        let now = self.now();
        self.update_build(build_id, |s| {
            s.failure = Some(BuildFailure {
                kind,
                message: message.clone(),
            });
            s.phase = BuildPhase::Failed;
            s.events.push(BuildEvent {
                build_id: s.build_id.clone(),
                seq: s.events.len() as u32 + 1,
                phase: BuildPhase::Failed,
                message,
                at_ms: now,
            });
        });
    }

    pub async fn start_build(
        &self,
        draft_id: &str,
        outbound_reviewed: bool,
    ) -> CoreResult<BuildState> {
        let draft = self.load_draft(draft_id)?;
        if !draft.ready_to_build {
            return Err(CoreError::Invalid(format!(
                "Not ready to build: {}",
                draft.blockers.join("; ")
            )));
        }
        let access = self.model_access().await;
        let route = access.effective_route.unwrap_or(ModelRoute::OfflineDemo);
        if route != ModelRoute::OfflineDemo && !outbound_reviewed {
            return Err(CoreError::Invalid(
                "Review what will be sent before building.".into(),
            ));
        }
        if lock(&self.inner.builds)
            .values()
            .any(|s| !s.state.phase.is_terminal())
        {
            return Err(CoreError::Invalid(
                "Another build is running. Stop it or wait for it to finish.".into(),
            ));
        }
        let build_id = new_id("build");
        let contract = drafts::contract_for(&draft);
        let (input_path, _) = self.handle_path(&draft.input.handle.handle_id, PickKind::File)?;
        let route_label = match route {
            ModelRoute::OfflineDemo => "Offline demo (no model)".to_string(),
            ModelRoute::ApiKey => format!("Meta API key · {DEFAULT_MODEL}"),
            ModelRoute::MusePlan => format!("Muse plan · {DEFAULT_MODEL}"),
        };
        let state = BuildState {
            build_id: build_id.clone(),
            draft_id: draft_id.into(),
            route,
            phase: BuildPhase::Queued,
            events: vec![],
            candidate: None,
            checks: vec![],
            comparison: None,
            spent_micros: None,
            failure: None,
            route_label,
        };
        let (tx, _) = broadcast::channel(64);
        lock(&self.inner.builds).insert(
            build_id.clone(),
            BuildSlot {
                state,
                tx,
                task: None,
            },
        );
        self.push_phase(&build_id, BuildPhase::Queued, "Queued");
        if route != ModelRoute::OfflineDemo {
            let now = self.now();
            self.db().insert_outbound(&db::OutboundRow {
                id: format!("out_{build_id}"),
                at_ms: now,
                purpose: "Build packet for a CSV tool".into(),
                destination: Self::route_destination(route).into(),
                route: route.as_str().into(),
                synthetic_only: true,
                cost_micros: None,
                payload_text: Some(Self::packet_text(&contract)),
            })?;
        }
        self.activity(
            "build",
            format!("Build started ({})", route.label()),
            Some(draft.goal.clone()),
        );
        let core = self.clone();
        let id = build_id.clone();
        let task =
            tokio::spawn(async move { core.run_build(id, route, contract, input_path).await });
        if let Some(slot) = lock(&self.inner.builds).get_mut(&build_id) {
            slot.task = Some(task);
        }
        self.build_state(&build_id)
    }

    async fn run_build(
        &self,
        build_id: String,
        route: ModelRoute,
        contract: TaskContract,
        input: PathBuf,
    ) {
        self.push_phase(&build_id, BuildPhase::Writing, "Writing a candidate");
        let builder_route = match route {
            ModelRoute::OfflineDemo => BuildRoute::Offline,
            ModelRoute::ApiKey => BuildRoute::DirectMeta {
                key_ref: Self::api_key_ref(),
            },
            ModelRoute::MusePlan => BuildRoute::MuseExec {
                route: CredentialRoute::PlanSignIn,
            },
        };
        let muse = if route == ModelRoute::MusePlan {
            match BinaryInfo::detect(&self.inner.cfg.home).await {
                Ok(binary) => Some(MuseConfig {
                    binary,
                    private_home: self.inner.cfg.data_dir.join("muse/home"),
                    real_home: self.inner.cfg.home.clone(),
                    synthetic_root: self.inner.cfg.data_dir.join("builds"),
                    key_ref: None,
                    containment_verified: self.muse_containment_accepted(),
                }),
                Err(_) => {
                    self.fail_build(
                        &build_id,
                        BuildFailureKind::NoModelAccess,
                        "Muse Code's program could not be found or changed during the build. Check Settings.",
                    );
                    return;
                }
            }
        } else {
            None
        };
        let builder = MappingBuilder {
            route: builder_route,
            vault: &*self.inner.cfg.vault,
            gateway: MetaResponsesClient::new(self.inner.cfg.gateway_base_url.clone()),
            policy: &self.inner.policy,
            consent_epoch: self.inner.policy.epoch(),
            muse,
        };
        let samples = SyntheticSamples::from_contract(&contract, 5);
        let budget = BuildBudget {
            max_cost_microusd: BUILD_BUDGET_MICROS as u64,
            ..BuildBudget::default()
        };
        let candidate = match builder
            .propose(&contract, samples, budget, &HostValidator)
            .await
        {
            Ok(c) => c,
            Err(e) => {
                let (kind, message) = build_error_message(&e);
                self.fail_build(&build_id, kind, message);
                return;
            }
        };
        let spent = candidate.cost_microusd.map(|c| c as i64);
        if let Some(cost) = spent {
            let _ = self
                .db()
                .set_outbound_cost(&format!("out_{build_id}"), Some(cost));
        }
        let mapping = candidate.mapping.clone();
        self.update_build(&build_id, |s| {
            s.candidate = Some(mapping.clone());
            s.spent_micros = spent;
        });
        self.push_phase(
            &build_id,
            BuildPhase::Candidate,
            format!("Candidate received from {}", candidate.label),
        );
        self.push_phase(
            &build_id,
            BuildPhase::Checking,
            "Checking the candidate on your CSV and on cases that must stop",
        );
        let checked = tokio::task::spawn_blocking(move || {
            let read = || std::fs::read(&input).map_err(|e| e.to_string());
            let outcome = checks::run(&contract, &mapping, &read);
            (outcome.all_passed(), outcome.checks, outcome.comparison)
        })
        .await;
        match checked {
            Ok((passed, check_list, comparison)) => {
                let failed: Vec<String> = check_list
                    .iter()
                    .filter(|c| !c.passed)
                    .map(|c| c.label.clone())
                    .collect();
                self.update_build(&build_id, |s| {
                    s.checks = check_list;
                    s.comparison = comparison;
                });
                if passed {
                    self.push_phase(&build_id, BuildPhase::Ready, "Ready to compare");
                } else {
                    self.fail_build(
                        &build_id,
                        BuildFailureKind::Checks,
                        format!(
                            "Checks failed: {}. Edit the rules and build again.",
                            failed.join(", ")
                        ),
                    );
                }
            }
            Err(_) => self.fail_build(
                &build_id,
                BuildFailureKind::Internal,
                "The checks could not run.",
            ),
        }
    }

    pub fn stop_build(&self, build_id: &str) -> CoreResult<BuildState> {
        let task = {
            let mut builds = lock(&self.inner.builds);
            let slot = builds
                .get_mut(build_id)
                .ok_or(CoreError::NotFound("build"))?;
            slot.task.take()
        };
        if let Some(task) = task {
            task.abort();
        }
        self.push_phase(build_id, BuildPhase::Stopped, "Stopped by you");
        self.activity("build", "Build stopped", None);
        self.build_state(build_id)
    }

    // ---------- tools ----------

    fn tool_detail(&self, row: &db::ToolRow) -> CoreResult<ToolDetail> {
        let hash = row
            .active_version
            .clone()
            .ok_or(CoreError::NotFound("tool version"))?;
        let version = self
            .db()
            .version(&row.id, &hash)?
            .ok_or(CoreError::NotFound("tool version"))?;
        let mapping: CsvMapping = serde_json::from_value(version.mapping)
            .map_err(|e| CoreError::Internal(e.to_string()))?;
        let d = &row.detail;
        let strings = |k: &str| -> Vec<String> {
            d.get(k)
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default()
        };
        Ok(ToolDetail {
            summary_row: self.tool_summary(row),
            summary: strings("summary"),
            mapping,
            input_columns: strings("inputColumns"),
            output_columns: strings("outputColumns"),
            access: strings("access"),
            version_hash: hash,
        })
    }

    fn tool_summary(&self, row: &db::ToolRow) -> ToolSummary {
        ToolSummary {
            id: row.id.clone(),
            name: row.name.clone(),
            version: row
                .detail
                .get("version")
                .and_then(Value::as_u64)
                .unwrap_or(1) as u32,
            engine: "csv-mapping".into(),
            created_at_ms: row.created_ms,
            last_run_at_ms: row.last_run_ms,
            run_count: row.run_count,
            sample: row.sample,
        }
    }

    fn create_tool(&self, new: NewTool<'_>) -> CoreResult<ToolDetail> {
        let NewTool {
            name,
            contract,
            mapping,
            summary,
            build_ref,
            model,
            sample,
        } = new;
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 80 {
            return Err(CoreError::Invalid(
                "Give the tool a name of 1–80 characters.".into(),
            ));
        }
        let tool_id = ToolId(new_id("tool"));
        let contract_hash = format!(
            "sha256:{}",
            hash_hex(
                &broker::canonical_json(contract)
                    .map_err(|e| CoreError::Internal(e.to_string()))?
            )
        );
        let mut manifest = json!({
            "schema": "macroscope.tool/v1",
            "id": tool_id.0,
            "name": name,
            "version": 1,
            "runtime": {"engine": "csv-mapping", "pin": "csv-mapping-1", "sdk": "1.0", "entry": "mapping.json"},
            "platforms": ["macos"],
            "inputs": {
                "type": "object", "additionalProperties": false,
                "required": ["input_csv", "output_root"],
                "properties": {
                    "input_csv": {"type": "string", "format": "selected-file-handle"},
                    "output_root": {"type": "string", "format": "selected-root-handle"}
                }
            },
            "triggers": [{"type": "manual"}],
            "capabilities": {
                "fileRead": [{"handle": "input_csv", "maxBytes": 10_000_000}],
                "fileCreate": [{"root": "output_root", "namePattern": "*.csv", "overwrite": false}],
                "fileMove": [], "fileTrash": [], "network": [], "clipboard": "none",
                "nativeApps": [], "secrets": [], "modelCalls": false, "mcp": []
            },
            "limits": {"wallSeconds": 120, "memoryMiB": 256, "effects": 1, "outputBytes": 20_000_000},
            "execution": {"initialTrust": "supervised", "concurrency": 1},
            "tests": {"suiteRef": "host-checks-v1", "fixtureDigest": contract_hash},
            "provenance": {"contractHash": contract_hash, "buildRef": build_ref, "model": model},
            "integrity": {"treeHash": "", "approvalRef": ""}
        });
        let tree = broker::canonical_json(&json!({"manifest": manifest, "mapping": mapping}))
            .map_err(|e| CoreError::Internal(e.to_string()))?;
        let version_hash = hash_hex(&tree);
        manifest["integrity"] = json!({"treeHash": format!("sha256:{version_hash}"), "approvalRef": format!("save-{}", self.now())});
        // Deserializing proves the manifest matches the domain schema (deny_unknown_fields).
        serde_json::from_value::<macroscope_domain::ToolManifest>(manifest.clone())
            .map_err(|e| CoreError::Internal(format!("manifest invalid: {e}")))?;
        let dir = self
            .inner
            .cfg
            .data_dir
            .join("tools")
            .join(&tool_id.0)
            .join("versions")
            .join(&version_hash);
        std::fs::create_dir_all(&dir).map_err(|e| CoreError::Internal(e.to_string()))?;
        for (file, value) in [
            ("manifest.json", &manifest),
            ("mapping.json", &json!(mapping)),
            ("contract.json", &json!(contract)),
        ] {
            let path = dir.join(file);
            std::fs::write(&path, serde_json::to_vec_pretty(value).unwrap_or_default())
                .map_err(|e| CoreError::Internal(e.to_string()))?;
            let mut perms = std::fs::metadata(&path)
                .map_err(|e| CoreError::Internal(e.to_string()))?
                .permissions();
            perms.set_readonly(true);
            let _ = std::fs::set_permissions(&path, perms);
        }
        let now = self.now();
        let access = vec![
            "Read the CSV you choose".to_string(),
            "Create one new CSV in the folder you choose".into(),
            "Never overwrites a file; the source stays unchanged".into(),
        ];
        let row = db::ToolRow {
            id: tool_id.0.clone(),
            name: name.into(),
            active_version: Some(version_hash.clone()),
            state: "enabled".into(),
            sample,
            created_ms: now,
            last_run_ms: None,
            run_count: 0,
            detail: json!({
                "version": 1,
                "summary": summary,
                "inputColumns": contract.input_schema,
                "outputColumns": contract.output_columns,
                "access": access,
                "contract": contract,
            }),
        };
        self.db().put_tool(&row)?;
        self.db().insert_version(&db::VersionRow {
            tool_id: tool_id.0.clone(),
            hash: version_hash.clone(),
            manifest,
            mapping: json!(mapping),
            created_ms: now,
        })?;
        self.db().put_grant(
            &new_id("grant"),
            &tool_id.0,
            &version_hash,
            1,
            &json!({"read": "selected_input", "create": "one_output_in_selected_root"}),
        )?;
        self.activity("build", format!("Saved tool “{name}”"), None);
        self.tool_detail(&row)
    }

    pub async fn save_tool(&self, build_id: &str, name: &str) -> CoreResult<ToolDetail> {
        let state = self.build_state(build_id)?;
        if state.phase != BuildPhase::Ready {
            return Err(CoreError::Invalid(
                "Only a build whose checks passed can be saved.".into(),
            ));
        }
        let mapping = state
            .candidate
            .clone()
            .ok_or(CoreError::NotFound("candidate"))?;
        let draft = self.load_draft(&state.draft_id)?;
        let mut contract = drafts::contract_for(&draft);
        contract.mapping = mapping.clone();
        contract.output_columns = mapping
            .output_columns
            .iter()
            .map(|c| c.name.clone())
            .collect();
        let model = if state.route == ModelRoute::OfflineDemo {
            "offline-demo".to_string()
        } else {
            DEFAULT_MODEL.to_string()
        };
        self.create_tool(NewTool {
            name,
            contract: &contract,
            mapping: &mapping,
            summary: draft.summary.clone(),
            build_ref: build_id,
            model: &model,
            sample: false,
        })
    }

    pub fn install_sample_tool(&self) -> CoreResult<ToolDetail> {
        if let Some(existing) = self.db().list_tools()?.into_iter().find(|t| t.sample) {
            return self.tool_detail(&existing);
        }
        let contract: TaskContract =
            serde_json::from_str(SAMPLE_TASK).map_err(|e| CoreError::Internal(e.to_string()))?;
        let summary = vec![
            "Sample: fabricated orders export → clean import CSV.".to_string(),
            "Dates MM/DD/YY → YYYY-MM-DD; amounts to decimal USD; customer names are not copied."
                .into(),
        ];
        self.create_tool(NewTool {
            name: "Clean import CSV (sample)",
            contract: &contract,
            mapping: &contract.mapping,
            summary,
            build_ref: "sample",
            model: "prebuilt-sample",
            sample: true,
        })
    }

    pub fn list_tools(&self) -> CoreResult<Vec<ToolSummary>> {
        Ok(self
            .db()
            .list_tools()?
            .iter()
            .map(|r| self.tool_summary(r))
            .collect())
    }

    pub fn get_tool(&self, tool_id: &str) -> CoreResult<ToolDetail> {
        let row = self
            .db()
            .tool(tool_id)?
            .ok_or(CoreError::NotFound("tool"))?;
        self.tool_detail(&row)
    }

    pub fn delete_tool(&self, tool_id: &str) -> CoreResult<()> {
        let row = self
            .db()
            .tool(tool_id)?
            .ok_or(CoreError::NotFound("tool"))?;
        for run in self.db().runs_for_tool(tool_id)? {
            if let Some(dir) = run.run_dir {
                let _ = std::fs::remove_dir_all(dir);
            }
        }
        self.db().delete_tool(tool_id)?;
        let dir = self.inner.cfg.data_dir.join("tools").join(tool_id);
        make_writable(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        self.activity("build", format!("Deleted tool “{}”", row.name), None);
        Ok(())
    }

    // ---------- runs ----------

    pub fn stage_run(
        &self,
        tool_id: &str,
        input_handle: &str,
        dir_handle: &str,
        output_name: &str,
    ) -> CoreResult<StageResult> {
        let row = self
            .db()
            .tool(tool_id)?
            .ok_or(CoreError::NotFound("tool"))?;
        let detail = self.tool_detail(&row)?;
        let contract: TaskContract =
            serde_json::from_value(row.detail.get("contract").cloned().unwrap_or(Value::Null))
                .map_err(|e| CoreError::Internal(e.to_string()))?;
        let (input_path, input) = self.handle_path(input_handle, PickKind::File)?;
        let (dir_path, output_dir) = self.handle_path(dir_handle, PickKind::Dir)?;
        let output_name = output_name.trim().to_string();
        if dir_path.join(&output_name).exists() {
            return Ok(StageResult::Failed(FailureEnvelope {
                failure: map_run_failure(BrokerError::DestinationExists),
            }));
        }
        let (_, grant_revision, _) = self
            .db()
            .active_grant(tool_id, &detail.version_hash)?
            .ok_or_else(|| {
                CoreError::Invalid("This tool has no active permission grant.".into())
            })?;
        let epochs = Epochs {
            grant_revision: grant_revision as u64,
            policy_epoch: 0,
        };
        let request = (|| -> Result<StageRequest, BrokerError> {
            Ok(StageRequest {
                tool_id: ToolId(tool_id.into()),
                version_hash: detail.version_hash.clone(),
                mapping: detail.mapping.clone(),
                contract,
                input: SelectedFile::from_picker(&input_path)?,
                destination: SelectedRoot::from_picker(&dir_path)?,
                basename: output_name.clone(),
                limits: Limits::default(),
                epochs,
                staging_parent: self.inner.cfg.data_dir.join("runs"),
                key: self.inner.staging_key,
            })
        })();
        let staged = match request.and_then(broker::stage) {
            Ok(s) => s,
            Err(e) => {
                return Ok(StageResult::Failed(FailureEnvelope {
                    failure: map_run_failure(e),
                }))
            }
        };
        let bytes = staged
            .preview()
            .map_err(|e| CoreError::Internal(e.to_string()))?;
        let parsed = drafts::parse(&bytes, drafts::PREVIEW_ROWS).map_err(CoreError::Internal)?;
        let run_id = staged.plan().run_id.0.clone();
        let dto = StagedRun {
            run_id: run_id.clone(),
            tool_id: tool_id.into(),
            input,
            output_dir,
            output_name,
            plan_hash: staged.plan_hash().into(),
            rows_in: staged.rows(),
            rows_out: parsed.row_count,
            output_columns: parsed.columns,
            preview_rows: parsed.rows,
            destination_exists: false,
        };
        self.db().put_run(&db::RunRow {
            id: run_id.clone(),
            tool_id: tool_id.into(),
            version_hash: detail.version_hash.clone(),
            state: "staged".into(),
            plan_hash: Some(dto.plan_hash.clone()),
            receipt: None,
            failure: None,
            created_ms: self.now(),
            run_dir: Some(staged.run_dir().to_string_lossy().into_owned()),
        })?;
        lock(&self.inner.staged).insert(
            run_id,
            StagedSlot {
                staged,
                dto: dto.clone(),
                tool_name: row.name,
            },
        );
        Ok(StageResult::Staged(Box::new(dto)))
    }

    pub fn commit_run(&self, run_id: &str, plan_hash: &str) -> CoreResult<CommitResult> {
        let slot = lock(&self.inner.staged)
            .remove(run_id)
            .ok_or(CoreError::NotFound("staged run"))?;
        if slot.dto.plan_hash != plan_hash {
            return Err(CoreError::Invalid(
                "The approval does not match the staged output. Stage again.".into(),
            ));
        }
        let StagedSlot {
            mut staged,
            dto,
            tool_name,
        } = slot;
        let approval = Approval {
            plan_hash: plan_hash.into(),
            approved_at_ms: self.now().max(0) as u64,
        };
        // Current revisions, not the plan's: a grant revoked after staging must refuse.
        let current_grant = self
            .db()
            .active_grant(&dto.tool_id, &staged.plan().version_hash)?
            .map(|(_, rev, _)| rev as u64)
            .unwrap_or(u64::MAX);
        let epochs = Epochs {
            grant_revision: current_grant,
            policy_epoch: 0,
        };
        let run_dir = staged.run_dir().to_path_buf();
        let result = broker::approve(&mut staged, plan_hash, approval.clone())
            .and_then(|_| broker::commit(staged, approval, epochs));
        let mut run = self.db().run(run_id)?.ok_or(CoreError::NotFound("run"))?;
        match result {
            Ok(receipt) => {
                let out = Receipt {
                    run_id: run_id.into(),
                    tool_id: dto.tool_id.clone(),
                    tool_name: tool_name.clone(),
                    version_hash: receipt.version_hash,
                    plan_hash: receipt.plan_hash,
                    input_digest: receipt.input_digest,
                    committed_digest: receipt.committed_digest,
                    output_path: receipt.output_path.to_string_lossy().into_owned(),
                    status: ReceiptStatus::Committed,
                    at_ms: receipt.timestamps.committed_at_ms as i64,
                };
                run.state = "committed".into();
                run.receipt = Some(json!(out));
                self.db().put_run(&run)?;
                if let Some(mut tool) = self.db().tool(&dto.tool_id)? {
                    tool.run_count += 1;
                    tool.last_run_ms = Some(self.now());
                    self.db().put_tool(&tool)?;
                }
                let _ = std::fs::remove_dir_all(&run_dir);
                self.activity(
                    "run",
                    format!("Ran “{tool_name}”"),
                    Some(format!("Created {}", dto.output_name)),
                );
                Ok(CommitResult::Committed(out))
            }
            Err(e) => {
                let failure = map_run_failure(e);
                run.state = "failed".into();
                run.failure = Some(json!(failure));
                self.db().put_run(&run)?;
                Ok(CommitResult::Failed(FailureEnvelope { failure }))
            }
        }
    }

    pub fn discard_run(&self, run_id: &str) -> CoreResult<()> {
        if let Some(slot) = lock(&self.inner.staged).remove(run_id) {
            let _ = std::fs::remove_dir_all(slot.staged.run_dir());
        }
        if let Some(mut run) = self.db().run(run_id)? {
            run.state = "discarded".into();
            run.run_dir = None;
            self.db().put_run(&run)?;
        }
        Ok(())
    }

    /// The committed output path of a receipt, for "Show in Finder". Only paths
    /// recorded in receipts can be revealed.
    pub fn receipt_output_path(&self, run_id: &str) -> CoreResult<PathBuf> {
        let run = self.db().run(run_id)?.ok_or(CoreError::NotFound("run"))?;
        let receipt: Receipt =
            serde_json::from_value(run.receipt.ok_or(CoreError::NotFound("receipt"))?)
                .map_err(|e| CoreError::Internal(e.to_string()))?;
        Ok(PathBuf::from(receipt.output_path))
    }

    // ---------- activity and privacy ----------

    pub fn list_activity(&self) -> CoreResult<Vec<ActivityItem>> {
        Ok(self
            .db()
            .list_activity(200)?
            .into_iter()
            .map(|a| ActivityItem {
                id: a.id,
                at_ms: a.at_ms,
                kind: a.kind,
                title: a.title,
                detail: a.detail,
            })
            .collect())
    }

    pub fn list_receipts(&self) -> CoreResult<Vec<Receipt>> {
        Ok(self
            .db()
            .list_runs()?
            .into_iter()
            .filter_map(|r| r.receipt.and_then(|v| serde_json::from_value(v).ok()))
            .collect())
    }

    pub async fn privacy_overview(&self) -> CoreResult<PrivacyOverview> {
        let egress = self.model_access().await.egress;
        let stats = self.db().stats()?;
        let count = |t: &str| {
            stats
                .iter()
                .find(|s| s.table == t)
                .map(|s| s.rows)
                .unwrap_or(0)
        };
        let outbound = self.list_outbound()?;
        let dir_size = |p: PathBuf| -> i64 { walk_size(&p) as i64 };
        Ok(PrivacyOverview {
            egress,
            stored: vec![
                StoredData {
                    label: "Tools and versions".into(),
                    count: count("tools"),
                    bytes: dir_size(self.inner.cfg.data_dir.join("tools")),
                    expires: "Until you delete them".into(),
                },
                StoredData {
                    label: "Run receipts".into(),
                    count: count("runs"),
                    bytes: 0,
                    expires: "Until you delete the tool".into(),
                },
                StoredData {
                    label: "Staged outputs (encrypted)".into(),
                    count: lock(&self.inner.staged).len() as i64,
                    bytes: dir_size(self.inner.cfg.data_dir.join("runs")),
                    expires: "Removed after you create or discard the output".into(),
                },
                StoredData {
                    label: "Outbound records".into(),
                    count: count("outbound"),
                    bytes: 0,
                    expires: "Payloads 7 days; records 30 days".into(),
                },
                StoredData {
                    label: "Connection detections".into(),
                    count: count("detections"),
                    bytes: 0,
                    expires: "Counts only; until you delete them".into(),
                },
            ],
            outbound,
        })
    }

    pub fn list_outbound(&self) -> CoreResult<Vec<OutboundRecord>> {
        Ok(self
            .db()
            .list_outbound()?
            .into_iter()
            .map(|o| OutboundRecord {
                id: o.id,
                at_ms: o.at_ms,
                purpose: o.purpose,
                destination: o.destination,
                route: serde_json::from_value(json!(o.route)).unwrap_or(ModelRoute::OfflineDemo),
                synthetic_only: o.synthetic_only,
                cost_micros: o.cost_micros,
                payload_text: o.payload_text,
            })
            .collect())
    }

    pub fn delete_outbound_history(&self) -> CoreResult<()> {
        let n = self.db().delete_outbound_history()?;
        self.activity("settings", format!("Deleted {n} outbound records"), None);
        Ok(())
    }

    /// Stops builds and staged runs and turns on Local only. Show stop lives in the
    /// shell's Show controller and is invoked alongside this.
    pub async fn emergency_stop(&self) -> CoreResult<()> {
        let ids: Vec<String> = lock(&self.inner.builds)
            .iter()
            .filter(|(_, s)| !s.state.phase.is_terminal())
            .map(|(id, _)| id.clone())
            .collect();
        for id in ids {
            let _ = self.stop_build(&id);
        }
        let staged: Vec<String> = lock(&self.inner.staged).keys().cloned().collect();
        for id in staged {
            let _ = self.discard_run(&id);
        }
        self.set_local_only(true).await?;
        self.activity("settings", "Emergency stop", None);
        Ok(())
    }

    /// For the shell's other services (connections, Show) that persist through the core.
    pub fn database(&self) -> &Db {
        self.db()
    }

    pub fn now_ms(&self) -> i64 {
        self.now()
    }

    pub fn log_activity(&self, kind: &str, title: &str, detail: Option<String>) {
        self.activity(kind, title.to_string(), detail);
    }
}

fn summary_text(summary: &Value) -> Option<String> {
    for key in ["plan", "planName", "tier", "name"] {
        if let Some(s) = summary.get(key).and_then(Value::as_str) {
            return Some(s.chars().take(120).collect());
        }
    }
    None
}

fn build_error_message(e: &BuildError) -> (BuildFailureKind, String) {
    use macroscope_builder::NoAccessReason as R;
    match e {
        BuildError::Gateway(GatewayError::AuthRejected) | BuildError::Credential => {
            (BuildFailureKind::Auth, "Your Meta API key was rejected. Replace it in Settings.".into())
        }
        BuildError::Gateway(GatewayError::BillingRequired) => {
            (BuildFailureKind::Auth, "Your Meta account needs billing set up for API use.".into())
        }
        BuildError::Gateway(GatewayError::Quota { .. }) => {
            (BuildFailureKind::Quota, "Meta's rate limit was reached. Wait a little and build again; nothing was charged twice.".into())
        }
        BuildError::Muse(macroscope_muse_client::MuseError::AuthRequired) => {
            (BuildFailureKind::Auth, "Muse Code is not signed in. Run muse login in Terminal, then check again.".into())
        }
        BuildError::BudgetExceeded => (BuildFailureKind::Budget, "This build would exceed its $2.00 budget.".into()),
        BuildError::Validation | BuildError::Gateway(GatewayError::MalformedOutput) => {
            (BuildFailureKind::Malformed, "The model's answer did not match the task summary. Try again or edit the rules.".into())
        }
        BuildError::NoModelAccess(reason) => (
            BuildFailureKind::NoModelAccess,
            match reason {
                R::ApiKeyMissing => "Add a Meta API key in Settings.",
                R::NotSignedIn => "Sign in to Muse Code (muse login in Terminal), then check again.",
                R::MuseMissing => "Muse Code is not installed.",
                R::PlanStatusUnknown => "Muse Code's sign-in status could not be checked.",
            }
            .into(),
        ),
        BuildError::ContainmentUnverified => (
            BuildFailureKind::NoModelAccess,
            "Muse builds need your OK in Settings first: Muse's file-reading tools are not yet proven confined to the build folder.".into(),
        ),
        BuildError::EgressDenied => (BuildFailureKind::NoModelAccess, "Local only is on, so cloud builds are paused.".into()),
        other => (BuildFailureKind::Internal, format!("The build failed: {other}")),
    }
}

fn walk_size(path: &Path) -> u64 {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return 0;
    };
    if meta.is_file() {
        return meta.len();
    }
    if !meta.is_dir() {
        return 0;
    }
    std::fs::read_dir(path)
        .map(|entries| entries.flatten().map(|e| walk_size(&e.path())).sum())
        .unwrap_or(0)
}

fn make_writable(path: &Path) {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return;
    };
    if meta.is_dir() {
        if let Ok(entries) = std::fs::read_dir(path) {
            for e in entries.flatten() {
                make_writable(&e.path());
            }
        }
    } else if meta.is_file() {
        let mut perms = meta.permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        perms.set_readonly(false);
        let _ = std::fs::set_permissions(path, perms);
    }
}
