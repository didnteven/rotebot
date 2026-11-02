//! Product generated-runtime boundary: OS sandbox only, with an explicit test seam.
use macroscope_effect_broker::{self as broker, BrokerError, StageRequest, StagedPlan};
use macroscope_runner::{launch, Limits, Request, Response};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Clone)]
pub struct RunnerEngine {
    runner: PathBuf,
    profile: PathBuf,
    runner_hash: String,
    profile_hash: String,
    synthetic: bool,
}
fn hash_file(path: &Path) -> Result<String, BrokerError> {
    Ok(format!("{:x}", Sha256::digest(std::fs::read(path)?)))
}

/// Host-fetched web sources for one staged run, with the plan binding that
/// covers the exact fetched bytes.
pub struct SourceStage<'a> {
    pub sources: &'a BTreeMap<String, String>,
    pub output_name: Option<&'a str>,
    pub create_file: bool,
    pub has_input_file: bool,
    pub extra_binding: Option<serde_json::Value>,
    pub answers: &'a BTreeMap<String, serde_json::Value>,
    /// Exact model instruction approved at save time. The runner and the host
    /// both reject any model request whose prompt differs.
    pub declared_model_prompt: Option<&'a str>,
}

static EMPTY_ANSWERS: BTreeMap<String, serde_json::Value> = BTreeMap::new();
static EMPTY_SOURCES: BTreeMap<String, String> = BTreeMap::new();

impl Default for SourceStage<'static> {
    fn default() -> Self {
        Self {
            sources: &EMPTY_SOURCES,
            output_name: None,
            create_file: true,
            has_input_file: true,
            extra_binding: None,
            answers: &EMPTY_ANSWERS,
            declared_model_prompt: None,
        }
    }
}
impl RunnerEngine {
    pub fn new(runner: &Path, profile: &Path) -> Result<Self, BrokerError> {
        let runner = runner.canonicalize()?;
        let profile = profile.canonicalize()?;
        Ok(Self {
            runner_hash: hash_file(&runner)?,
            profile_hash: hash_file(&profile)?,
            runner,
            profile,
            synthetic: false,
        })
    }
    pub fn discover() -> Result<Self, BrokerError> {
        let exe = std::env::current_exe()?;
        Self::discover_at(&exe)
    }

    fn discover_at(exe: &Path) -> Result<Self, BrokerError> {
        let parent = exe
            .parent()
            .ok_or_else(|| BrokerError::Runner("executable directory missing".into()))?;
        let in_bundle = parent.file_name().is_some_and(|name| name == "MacOS")
            && parent.parent().is_some_and(|contents| {
                contents.file_name().is_some_and(|name| name == "Contents")
            })
            && parent
                .parent()
                .and_then(Path::parent)
                .is_some_and(|app| app.extension().is_some_and(|ext| ext == "app"));
        let target = if parent
            .file_name()
            .is_some_and(|n| n == "deps" || n == "examples")
        {
            parent.parent().unwrap()
        } else {
            parent
        };
        let runner = target.join(format!("macroscope-runner{}", std::env::consts::EXE_SUFFIX));
        let bundled = target.join("macroscope-runner.sb");
        if in_bundle && (!runner.is_file() || !bundled.is_file()) {
            return Err(BrokerError::Runner(
                "The sandbox runner is missing from this app; reinstall Rotebot".into(),
            ));
        }
        let profile = if in_bundle || bundled.exists() {
            bundled
        } else {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../runner/profile.sb")
        };
        Self::new(&runner, &profile)
    }

    #[cfg(feature = "synthetic-runtime-tests")]
    pub fn new_synthetic(runner: &Path, profile: &Path) -> Result<Self, BrokerError> {
        let mut engine = Self::new(runner, profile)?;
        engine.synthetic = true;
        Ok(engine)
    }

    pub fn execute(
        &self,
        input: &[u8],
        code: &str,
        limits: Limits,
    ) -> Result<Vec<macroscope_runner::Output>, BrokerError> {
        self.execute_plan(input, code, limits)
            .map(|(outputs, _)| outputs)
    }

    /// Executes with host-fetched web source bodies available as data through
    /// `macroscope.source(name)`. The sandbox still gets no network access.
    /// With `has_input_file` false the snapshot is absent and
    /// `macroscope.input` is unavailable (unknown handle).
    pub fn execute_with_sources(
        &self,
        input: &[u8],
        sources: &BTreeMap<String, String>,
        has_input_file: bool,
        code: &str,
        limits: Limits,
    ) -> Result<
        (
            Vec<macroscope_runner::Output>,
            Vec<macroscope_domain::PlannedCall>,
        ),
        BrokerError,
    > {
        self.execute_with_answers(
            input,
            sources,
            has_input_file,
            code,
            limits,
            &BTreeMap::new(),
            None,
        )
    }

    pub fn execute_plan(
        &self,
        input: &[u8],
        code: &str,
        limits: Limits,
    ) -> Result<
        (
            Vec<macroscope_runner::Output>,
            Vec<macroscope_domain::PlannedCall>,
        ),
        BrokerError,
    > {
        self.execute_with_sources(input, &BTreeMap::new(), true, code, limits)
    }

    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::too_many_arguments)]
    pub fn execute_with_answers(
        &self,
        input: &[u8],
        sources: &BTreeMap<String, String>,
        has_input_file: bool,
        code: &str,
        limits: Limits,
        answers: &BTreeMap<String, serde_json::Value>,
        declared_model_prompt: Option<&str>,
    ) -> Result<
        (
            Vec<macroscope_runner::Output>,
            Vec<macroscope_domain::PlannedCall>,
        ),
        BrokerError,
    > {
        match self.replay(
            input,
            sources,
            has_input_file,
            code,
            limits,
            answers,
            declared_model_prompt,
        )? {
            Response::Success {
                outputs,
                planned_calls,
                ..
            } => Ok((outputs, planned_calls)),
            Response::ModelRequests { .. } => Err(BrokerError::Runner("ModelAnswerNeeded".into())),
            Response::Error { error } => Err(BrokerError::Runner(format!(
                "{}: {}",
                error.kind, error.message
            ))),
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub fn replay(
        &self,
        input: &[u8],
        sources: &BTreeMap<String, String>,
        has_input_file: bool,
        code: &str,
        limits: Limits,
        answers: &BTreeMap<String, serde_json::Value>,
        declared_model_prompt: Option<&str>,
    ) -> Result<Response, BrokerError> {
        if hash_file(&self.runner)? != self.runner_hash
            || hash_file(&self.profile)? != self.profile_hash
        {
            return Err(BrokerError::Runner("runner/profile pin changed".into()));
        }
        let text = std::str::from_utf8(input)
            .map_err(|_| BrokerError::Runner("input must be UTF-8".into()))?;
        let mut inputs = BTreeMap::new();
        if has_input_file {
            inputs.insert("input_csv".to_string(), text.to_string());
        }
        let request = Request {
            code: code.into(),
            inputs,
            sources: sources.clone(),
            limits,
            answers: answers.clone(),
            declared_model_prompt: declared_model_prompt.map(str::to_string),
            today: Some(crate::web::today_local()),
        };
        let command = if self.synthetic {
            #[cfg(feature = "synthetic-runtime-tests")]
            {
                launch::synthetic_command(&self.runner)
            }
            #[cfg(not(feature = "synthetic-runtime-tests"))]
            {
                return Err(BrokerError::Runner("synthetic mode disabled".into()));
            }
        } else {
            launch::sandbox_command(&self.runner, &self.profile)?
        };
        Ok(launch::invoke(command, &request)?)
    }

    pub fn stage(
        &self,
        run: StageRequest,
        code: &str,
        limits: Limits,
    ) -> Result<StagedPlan, BrokerError> {
        self.stage_inner(
            run,
            &SourceStage::default(),
            code,
            limits,
            self.synthetic,
            &[],
        )
    }

    /// Stages a program run with host-fetched web sources. The stage binding
    /// covers the exact fetched bytes: the run plan hash covers them instead
    /// of an input file snapshot when there is no input file.
    pub fn stage_with_sources(
        &self,
        run: StageRequest,
        stage_sources: &SourceStage<'_>,
        code: &str,
        limits: Limits,
    ) -> Result<StagedPlan, BrokerError> {
        self.stage_inner(run, stage_sources, code, limits, self.synthetic, &[])
    }
    /// Explicit synthetic developer experiment, never an OS-denial fallback.
    #[cfg(feature = "synthetic-runtime-tests")]
    pub fn stage_synthetic(
        &self,
        run: StageRequest,
        code: &str,
        limits: Limits,
    ) -> Result<StagedPlan, BrokerError> {
        self.stage_inner(run, &SourceStage::default(), code, limits, true, &[])
    }
    pub fn stage_connections(
        &self,
        run: StageRequest,
        code: &str,
        limits: Limits,
        selected: &[crate::dto::SelectedConnection],
    ) -> Result<StagedPlan, BrokerError> {
        self.stage_with_sources_and_connections(
            run,
            &SourceStage::default(),
            code,
            limits,
            selected,
        )
    }
    pub fn stage_with_sources_and_connections(
        &self,
        run: StageRequest,
        stage_sources: &SourceStage<'_>,
        code: &str,
        limits: Limits,
        selected: &[crate::dto::SelectedConnection],
    ) -> Result<StagedPlan, BrokerError> {
        self.stage_inner(run, stage_sources, code, limits, self.synthetic, selected)
    }
    fn stage_inner(
        &self,
        run: StageRequest,
        stage_sources: &SourceStage<'_>,
        code: &str,
        limits: Limits,
        synthetic: bool,
        selected: &[crate::dto::SelectedConnection],
    ) -> Result<StagedPlan, BrokerError> {
        limits
            .validate()
            .map_err(|e| BrokerError::Runner(e.message))?;
        if hash_file(&self.runner)? != self.runner_hash
            || hash_file(&self.profile)? != self.profile_hash
        {
            return Err(BrokerError::Runner("runner/profile pin changed".into()));
        }
        let binding = serde_json::json!({"code":code,"runner":self.runner_hash,"profile":self.profile_hash,
            "rquickjs":macroscope_runner::RQUICKJS_VERSION,"sdk":"1","limits":limits,"synthetic":synthetic,
            "answers":stage_sources.answers,"model_prompt":stage_sources.declared_model_prompt,"output_name":stage_sources.output_name,"create_file":stage_sources.create_file,"sources": stage_sources.extra_binding.clone().unwrap_or(serde_json::Value::Null)});
        let execution_hash = format!("{:x}", Sha256::digest(broker::canonical_json(&binding)?));
        let basename = stage_sources
            .output_name
            .unwrap_or(&run.basename)
            .to_string();
        let sources = stage_sources.sources.clone();
        let has_input_file = stage_sources.has_input_file;
        let mut execution_engine = self.clone();
        execution_engine.synthetic = synthetic;
        broker::stage_generated_outputs(
            run,
            execution_hash,
            stage_sources.create_file,
            |snapshot| {
                let (mut outputs, calls) = execution_engine.execute_with_answers(
                    snapshot,
                    &sources,
                    has_input_file,
                    code,
                    limits,
                    stage_sources.answers,
                    stage_sources.declared_model_prompt,
                )?;
                if outputs.len() > 1
                    || outputs.first().is_some_and(|o| o.name != basename)
                    || (outputs.is_empty() && calls.is_empty())
                {
                    return Err(BrokerError::Runner(
                        "expected one output with the approved basename".into(),
                    ));
                }
                Ok((
                    if outputs.is_empty() {
                        vec![]
                    } else {
                        outputs.remove(0).text.into_bytes()
                    },
                    crate::connections::validate_calls(&calls, selected)
                        .map_err(BrokerError::Runner)?,
                ))
            },
        )
    }
}

#[cfg(test)]
mod discovery_tests {
    use super::*;

    fn bundle_exe(root: &Path) -> PathBuf {
        let macos = root.join("X.app/Contents/MacOS");
        std::fs::create_dir_all(&macos).unwrap();
        macos.join("macroscope-desktop")
    }

    fn assert_missing_bundle(exe: &Path) {
        let error = RunnerEngine::discover_at(exe)
            .err()
            .expect("incomplete bundle must fail");
        assert!(matches!(error, BrokerError::Runner(message)
            if message == "The sandbox runner is missing from this app; reinstall Rotebot"));
    }

    const RUNNER: &str = if cfg!(windows) {
        "macroscope-runner.exe"
    } else {
        "macroscope-runner"
    };

    #[test]
    fn bundle_discovers_adjacent_runner_and_profile() {
        let temp = tempfile::tempdir().unwrap();
        let exe = bundle_exe(temp.path());
        let macos = exe.parent().unwrap();
        let runner = macos.join(RUNNER);
        let profile = macos.join("macroscope-runner.sb");
        std::fs::write(&runner, b"test runner").unwrap();
        std::fs::write(&profile, b"bundled profile").unwrap();

        let engine = RunnerEngine::discover_at(&exe).unwrap();
        assert_eq!(engine.runner, runner.canonicalize().unwrap());
        assert_eq!(engine.profile, profile.canonicalize().unwrap());
        assert_eq!(engine.profile_hash, hash_file(&profile).unwrap());
    }

    #[test]
    fn bundle_missing_runner_reports_reinstall() {
        let temp = tempfile::tempdir().unwrap();
        let exe = bundle_exe(temp.path());
        assert_missing_bundle(&exe);
        std::fs::write(exe.with_file_name("macroscope-runner.sb"), b"profile").unwrap();
        assert_missing_bundle(&exe);
    }

    #[test]
    fn bundle_missing_profile_never_uses_source_tree_fallback() {
        let temp = tempfile::tempdir().unwrap();
        let exe = bundle_exe(temp.path());
        std::fs::write(exe.with_file_name(RUNNER), b"test runner").unwrap();
        assert_missing_bundle(&exe);
    }

    #[test]
    fn non_bundle_discovery_keeps_source_profile_fallback() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("debug");
        std::fs::create_dir_all(&target).unwrap();
        let runner = target.join(RUNNER);
        std::fs::write(&runner, b"test runner").unwrap();
        let source_profile = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../runner/profile.sb")
            .canonicalize()
            .unwrap();
        for directory in ["", "deps", "examples"] {
            let parent = target.join(directory);
            std::fs::create_dir_all(&parent).unwrap();
            let engine = RunnerEngine::discover_at(&parent.join("app")).unwrap();
            assert_eq!(engine.runner, runner.canonicalize().unwrap());
            assert_eq!(engine.profile, source_profile);
        }
    }
}
