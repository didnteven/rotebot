//! Experimental generated-runtime seam; deliberately absent from product flows.
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
        })
    }
    pub fn stage(
        &self,
        run: StageRequest,
        code: &str,
        limits: Limits,
    ) -> Result<StagedPlan, BrokerError> {
        self.stage_inner(run, code, limits, false)
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
            let input = std::str::from_utf8(snapshot)
                .map_err(|_| BrokerError::Runner("input must be UTF-8".into()))?;
            let request = Request {
                code: code.into(),
                inputs: BTreeMap::from([("input_csv".into(), input.into())]),
                limits,
            };
            let command = if synthetic {
                // This branch is reachable only through the explicitly flagged test API.
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
                Response::Success { mut outputs, .. }
                    if outputs.len() == 1 && outputs[0].name == basename =>
                {
                    Ok(outputs.remove(0).text.into_bytes())
                }
                Response::Success { .. } => Err(BrokerError::Runner(
                    "expected one output with the approved basename".into(),
                )),
                Response::Error { error } => Err(BrokerError::Runner(format!(
                    "{}: {}",
                    error.kind, error.message
                ))),
            }
        })
    }
}
