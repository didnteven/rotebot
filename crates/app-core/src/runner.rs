//! Product generated-runtime boundary: OS sandbox only, with an explicit test seam.
use macroscope_effect_broker::{self as broker, BrokerError, StageRequest, StagedPlan};
use macroscope_runner::{launch, Limits, Request, Response};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

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
        let parent = exe
            .parent()
            .ok_or_else(|| BrokerError::Runner("executable directory missing".into()))?;
        let target = if parent
            .file_name()
            .is_some_and(|n| n == "deps" || n == "examples")
        {
            parent.parent().unwrap()
        } else {
            parent
        };
        let runner = target.join("macroscope-runner");
        let bundled = target.join("macroscope-runner.sb");
        let profile = if bundled.exists() {
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
        self.execute_inner(input, code, limits, self.synthetic)
    }

    fn execute_inner(
        &self,
        input: &[u8],
        code: &str,
        limits: Limits,
        synthetic: bool,
    ) -> Result<Vec<macroscope_runner::Output>, BrokerError> {
        if hash_file(&self.runner)? != self.runner_hash
            || hash_file(&self.profile)? != self.profile_hash
        {
            return Err(BrokerError::Runner("runner/profile pin changed".into()));
        }
        let text = std::str::from_utf8(input)
            .map_err(|_| BrokerError::Runner("input must be UTF-8".into()))?;
        let request = Request {
            code: code.into(),
            inputs: BTreeMap::from([("input_csv".into(), text.into())]),
            limits,
        };
        let command = if synthetic {
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
        match launch::invoke(command, &request)? {
            Response::Success { outputs, .. } => Ok(outputs),
            Response::Error { error } => Err(BrokerError::Runner(format!(
                "{}: {}",
                error.kind, error.message
            ))),
        }
    }

    pub fn stage(
        &self,
        run: StageRequest,
        code: &str,
        limits: Limits,
    ) -> Result<StagedPlan, BrokerError> {
        self.stage_inner(run, code, limits, self.synthetic)
    }
    /// Explicit synthetic developer experiment, never an OS-denial fallback.
    #[cfg(feature = "synthetic-runtime-tests")]
    pub fn stage_synthetic(
        &self,
        run: StageRequest,
        code: &str,
        limits: Limits,
    ) -> Result<StagedPlan, BrokerError> {
        self.stage_inner(run, code, limits, true)
    }
    fn stage_inner(
        &self,
        run: StageRequest,
        code: &str,
        limits: Limits,
        synthetic: bool,
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
            "rquickjs":macroscope_runner::RQUICKJS_VERSION,"sdk":"1","limits":limits,"synthetic":synthetic});
        let execution_hash = format!("{:x}", Sha256::digest(broker::canonical_json(&binding)?));
        let basename = run.basename.clone();
        broker::stage_generated(run, execution_hash, |snapshot| {
            let mut outputs = self.execute_inner(snapshot, code, limits, synthetic)?;
            if outputs.len() != 1 || outputs[0].name != basename {
                return Err(BrokerError::Runner(
                    "expected one output with the approved basename".into(),
                ));
            }
            Ok(outputs.remove(0).text.into_bytes())
        })
    }
}
