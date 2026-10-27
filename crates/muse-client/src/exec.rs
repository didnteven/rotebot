use crate::{
    binary_sha256, diagnostic,
    supervisor::{read_frame, redact_value, ManagedChild},
    BinaryInfo, ChildEnv, CredentialRoute, MuseError,
};
use serde_json::Value;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, BufReader},
    process::Command,
    sync::OnceCell,
};

#[derive(Clone)]
pub struct ExecClient {
    binary: PathBuf,
    help: Arc<OnceCell<String>>,
    digest: Result<String, MuseError>,
}
#[derive(Clone)]
pub struct ExecOutput {
    pub events: Vec<Value>,
    pub terminal: Option<String>,
    pub text: Option<String>,
    pub usage: Option<Value>,
    pub exit_code: Option<i32>,
    pub stderr: String,
    root_command_id: Option<String>,
}
impl std::fmt::Debug for ExecOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExecOutput")
            .field("event_count", &self.events.len())
            .field("exit_code", &self.exit_code)
            .field("content", &"<redacted>")
            .finish()
    }
}
impl ExecOutput {
    pub fn parse(stdout: &str, exit_code: Option<i32>, stderr: &str) -> Result<Self, MuseError> {
        let mut result = Self {
            events: vec![],
            terminal: None,
            text: None,
            usage: None,
            exit_code,
            stderr: diagnostic(stderr),
            root_command_id: None,
        };
        for line in stdout.lines().filter(|l| !l.trim().is_empty()) {
            if line.len() > macroscope_protocol::MAX_MESSAGE_BYTES {
                return Err(MuseError::Framing);
            }
            let value: Value = serde_json::from_str(line).map_err(|_| MuseError::Framing)?;
            result.push(value)?;
        }
        Ok(result)
    }
    fn push(&mut self, value: Value) -> Result<(), MuseError> {
        if self.events.len() >= 4096 {
            return Err(MuseError::Framing);
        }
        if value["payload_type"] == "runtime.command.accepted"
            && value["payload"]["command_kind"] == "turn.submit"
            && self.root_command_id.is_none()
        {
            self.root_command_id = value["payload"]["command_id"].as_str().map(String::from);
        }
        if self
            .root_command_id
            .as_deref()
            .is_none_or(|id| value["payload"]["command_id"].as_str() == Some(id))
            && value["payload_type"]
                .as_str()
                .is_some_and(|s| s.starts_with("run.terminal."))
        {
            self.terminal = value["payload"]["terminal"].as_str().map(String::from);
            self.text = value["payload"]["text"].as_str().map(String::from);
        }
        if value["payload_type"] == "run.usage" {
            self.usage = Some(value["payload"].clone());
        }
        self.events.push(value);
        Ok(())
    }
}
impl ExecClient {
    pub fn new(binary: impl Into<PathBuf>) -> Self {
        let binary = binary.into();
        let digest = binary_sha256(&binary);
        Self::with_identity(binary, digest)
    }
    pub fn pinned(binary: &BinaryInfo) -> Self {
        Self::with_identity(binary.path.clone(), Ok(binary.binary_sha256.clone()))
    }
    fn with_identity(binary: PathBuf, digest: Result<String, MuseError>) -> Self {
        type HelpCache = Mutex<HashMap<(PathBuf, String), Arc<OnceCell<String>>>>;
        static HELP: OnceLock<HelpCache> = OnceLock::new();
        let help = HELP
            .get_or_init(Mutex::default)
            .lock()
            .unwrap()
            .entry((binary.clone(), digest.clone().unwrap_or_default()))
            .or_default()
            .clone();
        Self {
            binary,
            help,
            digest,
        }
    }
    fn validate(&self) -> Result<(), MuseError> {
        if binary_sha256(&self.binary)? != *self.digest.as_ref().map_err(Clone::clone)? {
            return Err(MuseError::BinaryChanged);
        }
        Ok(())
    }
    async fn help(&self, env: &ChildEnv, workspace: &Path) -> Result<&String, MuseError> {
        self.help
            .get_or_try_init(|| async {
                self.validate()?;
                let mut cmd = Command::new(&self.binary);
                env.apply(&mut cmd, false)?;
                cmd.args(["exec", "--help"])
                    .current_dir(workspace)
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null());
                let mut child = ManagedChild::new(cmd)?;
                let stdout = child.child().stdout.take().ok_or(MuseError::Transport)?;
                let mut bytes = vec![];
                let result = async {
                    tokio::time::timeout(
                        Duration::from_secs(20),
                        stdout.take(128 * 1024).read_to_end(&mut bytes),
                    )
                    .await
                    .map_err(|_| MuseError::Timeout)??;
                    let status = child.finish(Duration::from_secs(2)).await?;
                    if !status.success() {
                        return Err(MuseError::UnsupportedExecFlags);
                    }
                    Ok::<(), MuseError>(())
                }
                .await;
                if let Err(error) = result {
                    child.kill_group();
                    let _ = child.finish(Duration::from_secs(2)).await;
                    return Err(error);
                }
                Ok(String::from_utf8_lossy(&bytes).to_string())
            })
            .await
    }
    pub async fn exec_structured(
        &self,
        prompt: &str,
        schema_path: &Path,
        workspace: &Path,
        route: CredentialRoute,
        env: &ChildEnv,
    ) -> Result<ExecOutput, MuseError> {
        self.exec_with_provider(prompt, schema_path, workspace, Some(route), env)
            .await
    }
    /// Real-binary parser check; echo uses a private environment and no provider credentials.
    pub async fn exec_echo_structured(
        &self,
        prompt: &str,
        schema_path: &Path,
        workspace: &Path,
        env: &ChildEnv,
    ) -> Result<ExecOutput, MuseError> {
        self.exec_with_provider(prompt, schema_path, workspace, None, env)
            .await
    }
    async fn exec_with_provider(
        &self,
        prompt: &str,
        schema_path: &Path,
        workspace: &Path,
        credential_route: Option<CredentialRoute>,
        env: &ChildEnv,
    ) -> Result<ExecOutput, MuseError> {
        self.validate()?;
        let route = credential_route.unwrap_or(CredentialRoute::PlanSignIn);
        if env.key().is_some_and(|key| {
            !key.expose().is_empty()
                && prompt
                    .as_bytes()
                    .windows(key.expose().len())
                    .any(|w| w == key.expose())
        }) {
            return Err(MuseError::KeyUnavailable);
        }
        let help = self.help(env, workspace).await?;
        let mut args = exec_args(help, schema_path, workspace, route)?;
        if credential_route.is_none() {
            if !help.split_whitespace().any(|w| w == "--provider") {
                return Err(MuseError::UnsupportedExecFlags);
            }
            // Muse restricts --model to Meta/local providers, and 1.4.13 rejects
            // --output-schema with echo (echo sends no text.format block).
            for flag in ["--model", "--output-schema"] {
                if let Some(index) = args.iter().position(|arg| arg == flag) {
                    args.drain(index..index + 2);
                }
            }
            args.extend(["--provider".into(), "echo".into()]);
        }
        // NamedTempFile is created with mode 0600 and unlinks on every exit,
        // including when this future is dropped during cancellation.
        use std::io::Write;
        let mut prompt_file = tempfile::Builder::new()
            .prefix("model-prompt-")
            .tempfile_in(workspace)?;
        prompt_file.write_all(prompt.as_bytes())?;
        prompt_file.flush()?;
        self.validate()?;
        let mut cmd = Command::new(&self.binary);
        env.apply(&mut cmd, false)?;
        cmd.args(args)
            .arg("--prompt-file")
            .arg(prompt_file.path())
            .current_dir(workspace)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = ManagedChild::new(cmd)?;
        let result = async {
            let mut stdin = child.child().stdin.take().ok_or(MuseError::Transport)?;
            if route == CredentialRoute::ApiKey {
                let key = env.key().ok_or(MuseError::KeyUnavailable)?;
                stdin.write_all(key.expose()).await?;
                stdin.write_all(b"\n").await?;
            }
            drop(stdin);
            let stdout = child.child().stdout.take().ok_or(MuseError::Transport)?;
            let stderr = child.child().stderr.take().ok_or(MuseError::Transport)?;
            // Keep a bounded diagnostic prefix, discard the rest to avoid pipe
            // backpressure. Return only fixed diagnostic classes, never raw stderr.
            let stderr_task = tokio::spawn(async move {
                let mut stderr = stderr;
                let mut prefix = vec![];
                let mut buf = [0; 4096];
                while let Ok(n) = stderr.read(&mut buf).await {
                    if n == 0 {
                        break;
                    }
                    let take = n.min(4096 - prefix.len());
                    prefix.extend_from_slice(&buf[..take]);
                }
                diagnostic(&String::from_utf8_lossy(&prefix))
            });
            let mut result = ExecOutput::parse("", None, "")?;
            let read = tokio::time::timeout(Duration::from_secs(120), async {
                let mut reader = BufReader::new(stdout);
                loop {
                    // Distinguish clean EOF (including fresh-home pre-JSONL exit)
                    // from a partial/malformed frame.
                    use tokio::io::AsyncBufReadExt;
                    if reader.fill_buf().await?.is_empty() {
                        break;
                    }
                    let mut value = read_frame(&mut reader).await?;
                    redact_value(&mut value, env.key());
                    result.push(value)?;
                }
                Ok::<(), MuseError>(())
            })
            .await;
            if !matches!(read, Ok(Ok(()))) {
                stderr_task.abort();
                child.kill_group();
                let _ = child.finish(Duration::from_secs(2)).await;
                return Err(match read {
                    Ok(Err(e)) => e,
                    _ => MuseError::Timeout,
                });
            }
            result.exit_code = child.finish(Duration::from_secs(5)).await?.code();
            self.validate()?;
            result.stderr = tokio::time::timeout(Duration::from_secs(2), stderr_task)
                .await
                .map_err(|_| MuseError::Timeout)?
                .map_err(|_| MuseError::Transport)?;
            Ok(result)
        }
        .await;
        if result.is_err() {
            child.kill_group();
            let _ = child.finish(Duration::from_secs(2)).await;
        }
        result
    }
}
/// Unsupported optional limits are dropped. The mandatory credential, schema
/// and non-interactive safety posture fails closed if the binary lacks it.
pub fn exec_args(
    help: &str,
    schema: &Path,
    workspace: &Path,
    route: CredentialRoute,
) -> Result<Vec<std::ffi::OsString>, MuseError> {
    let supports = |flag: &str| {
        help.split_whitespace()
            .any(|w| w.trim_matches([',', '[', ']']) == flag)
    };
    for flag in [
        "--json",
        "--model",
        "--contributor-mode",
        "--approval-mode",
        "--output-schema",
        "--workspace",
        "--prompt-file",
    ] {
        if !supports(flag) {
            return Err(MuseError::UnsupportedExecFlags);
        }
    }
    if route == CredentialRoute::ApiKey && !supports("--api-key-stdin") {
        return Err(MuseError::UnsupportedExecFlags);
    }
    let mut args: Vec<std::ffi::OsString> = [
        "exec",
        "--json",
        "--model",
        "muse-spark-1.3",
        "--contributor-mode",
        "off",
        "--approval-mode",
        "never",
        "--output-schema",
    ]
    .into_iter()
    .map(Into::into)
    .collect();
    args.push(schema.into());
    args.push("--workspace".into());
    args.push(workspace.into());
    if supports("--max-model-steps") {
        args.extend(["--max-model-steps".into(), "8".into()]);
    }
    for f in [
        "--disable-web-tools",
        "--disable-shell",
        "--disable-reminders",
    ] {
        if supports(f) {
            args.push(f.into());
        }
    }
    if route == CredentialRoute::ApiKey {
        args.push("--api-key-stdin".into());
    }
    Ok(args)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replay_exec_empty_and_badkey_terminals() {
        let empty=ExecOutput::parse(include_str!("../../../plan/probes/transcripts/05-exec-envkey-badkey.stdout.jsonl"),Some(1),"failed to fetch model catalog: authentication failed: your API key from META_API_KEY was rejected").unwrap();
        assert!(empty.events.is_empty());
        assert_eq!(empty.exit_code, Some(1));
        assert!(empty.stderr.contains("rejected"));
        let result = ExecOutput::parse(
            include_str!(
                "../../../plan/probes/transcripts/07-exec-apikeystdin-badkey-realhome.stdout.jsonl"
            ),
            Some(1),
            "",
        )
        .unwrap();
        assert_eq!(result.terminal.as_deref(), Some("failed"));
        assert!(result.usage.is_some());
    }
    #[test]
    fn flags_are_runtime_gated() {
        let help="--json --model --contributor-mode --approval-mode --output-schema --workspace --prompt-file --api-key-stdin";
        let args = exec_args(
            help,
            Path::new("schema"),
            Path::new("ws"),
            CredentialRoute::ApiKey,
        )
        .unwrap();
        assert!(args.contains(&"--api-key-stdin".into()));
        assert!(!args.contains(&"--max-model-steps".into()));
        assert!(exec_args(
            "--json",
            Path::new("s"),
            Path::new("w"),
            CredentialRoute::ApiKey
        )
        .is_err());
    }
}

#[cfg(all(test, unix))]
mod process_tests {
    use super::*;
    use macroscope_vault::Secret;
    use std::os::unix::fs::PermissionsExt;
    #[tokio::test]
    async fn exec_canary_is_stdin_only_env_is_allowlisted_outputs_scrubbed_and_help_cached() {
        let root = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
        let script = root.path().join("fake-muse");
        std::fs::write(&script,r#"#!/usr/bin/python3
import sys,os,json
if '--help' in sys.argv:
    with open('help-count','a') as f: f.write('checked\n')
    print('--json --model --contributor-mode --approval-mode --output-schema --workspace --prompt-file --max-model-steps --disable-web-tools --disable-shell --disable-reminders --api-key-stdin')
    sys.exit(0)
key=sys.stdin.readline().strip()
assert key
assert key not in ' '.join(sys.argv)
assert 'PRIVATE-MODEL-DATA-CANARY' not in ' '.join(sys.argv)
path=sys.argv[sys.argv.index('--prompt-file')+1]
assert os.stat(path).st_mode & 0o777 == 0o600
with open(path) as f: assert 'PRIVATE-MODEL-DATA-CANARY' in f.read()
with open('prompt-path','w') as f: f.write(path)
assert 'META_API_KEY' not in os.environ
assert 'MACROSCOPE_PARENT_CANARY' not in os.environ
print(json.dumps({'payload_type':'run.terminal.completed','payload':{'terminal':'completed','text':'{}'},'argv':sys.argv,'reflected':key}))
print(key,file=sys.stderr)
"#).unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
        let env = ChildEnv::for_route(
            CredentialRoute::ApiKey,
            &root.path().join("home"),
            root.path(),
            Some(Secret::new(b"bogus-exec-secret-canary".to_vec())),
        )
        .unwrap();
        let schema = root.path().join("schema.json");
        std::fs::write(&schema, "{}").unwrap();
        let client = ExecClient::new(&script);
        for _ in 0..2 {
            let client = ExecClient::new(&script);
            let out = client
                .exec_structured(
                    "synthetic prompt PRIVATE-MODEL-DATA-CANARY",
                    &schema,
                    root.path(),
                    CredentialRoute::ApiKey,
                    &env,
                )
                .await
                .unwrap();
            assert!(!std::path::Path::new(
                &std::fs::read_to_string(root.path().join("prompt-path")).unwrap()
            )
            .exists());
            assert!(!out.events[0]["argv"]
                .to_string()
                .contains("PRIVATE-MODEL-DATA-CANARY"));
            assert_eq!(out.exit_code, Some(0));
            assert_eq!(out.terminal.as_deref(), Some("completed"));
            assert!(!format!("{out:?}").contains("bogus-exec-secret-canary"));
            assert_eq!(out.events[0]["reflected"], "<redacted>");
            assert!(!out.events[0]["argv"]
                .to_string()
                .contains("bogus-exec-secret-canary"));
        }
        assert_eq!(
            std::fs::read_to_string(root.path().join("help-count")).unwrap(),
            "checked\n"
        );
        assert!(client.help.get().is_some());
        std::fs::write(&script, "changed executable").unwrap();
        assert_eq!(
            client
                .exec_structured(
                    "synthetic prompt PRIVATE-MODEL-DATA-CANARY",
                    &schema,
                    root.path(),
                    CredentialRoute::ApiKey,
                    &env
                )
                .await
                .unwrap_err(),
            MuseError::BinaryChanged
        );
        assert_eq!(
            std::fs::read_to_string(root.path().join("help-count")).unwrap(),
            "checked\n"
        );
    }
}
