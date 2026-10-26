use crate::{
    classify_rpc_error, ensure_conformance, AcceptanceStore, BinaryInfo, BuildEvent, ChildEnv,
    JsonAcceptanceStore, MuseError, SetupState, TurnReducer,
};
use macroscope_protocol::{read_message, write_message, MAX_MESSAGE_BYTES};
use serde_json::{json, Value};
use std::{collections::VecDeque, path::Path, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
};

pub(crate) struct ManagedChild {
    pub child: Option<Child>,
    #[cfg(unix)]
    group_id: Option<u32>,
}
impl ManagedChild {
    pub fn new(mut command: Command) -> Result<Self, MuseError> {
        command.kill_on_drop(true);
        #[cfg(unix)]
        command.process_group(0);
        let child = command.spawn()?;
        #[cfg(unix)]
        let group_id = child.id();
        Ok(Self {
            child: Some(child),
            #[cfg(unix)]
            group_id,
        })
    }
    pub fn child(&mut self) -> &mut Child {
        self.child.as_mut().unwrap()
    }
    pub fn kill_group(&mut self) {
        #[cfg(unix)]
        if let Some(id) = self.group_id.take() {
            // SAFETY: the child was created in its own process group. Negative
            // pid targets that group, including reminder/worker descendants.
            unsafe {
                libc::kill(-(id as i32), libc::SIGKILL);
            }
        }
        let _ = self.child().start_kill();
    }
    pub async fn finish(
        &mut self,
        deadline: Duration,
    ) -> Result<std::process::ExitStatus, MuseError> {
        match tokio::time::timeout(deadline, self.child().wait()).await {
            Ok(status) => {
                let status = status?;
                self.kill_group();
                Ok(status)
            }
            Err(_) => {
                self.kill_group();
                self.child().wait().await?;
                Err(MuseError::Timeout)
            }
        }
    }
}
impl Drop for ManagedChild {
    fn drop(&mut self) {
        if self.child.is_none() {
            return;
        }
        self.kill_group();
        if let Some(mut child) = self.child.take() {
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                handle.spawn(async move {
                    let _ = child.wait().await;
                });
            }
        }
    }
}
/// Bounded async collection followed by the shared protocol parser. Never use
/// read_line/read_until, which allocate without a cap on an untrusted stream.
pub(crate) async fn read_frame(
    reader: &mut (impl AsyncBufRead + Unpin),
) -> Result<Value, MuseError> {
    let mut line = Vec::new();
    loop {
        let available = reader.fill_buf().await?;
        if available.is_empty() {
            return Err(if line.is_empty() {
                MuseError::Transport
            } else {
                MuseError::Framing
            });
        }
        let newline = available.iter().position(|&b| b == b'\n');
        let n = newline.unwrap_or(available.len());
        if n > MAX_MESSAGE_BYTES - line.len() {
            return Err(MuseError::Framing);
        }
        line.extend_from_slice(&available[..n]);
        reader.consume(n + usize::from(newline.is_some()));
        if newline.is_some() {
            line.push(b'\n');
            return read_message(&mut line.as_slice()).map_err(|_| MuseError::Framing);
        }
    }
}
pub(crate) fn redact_value(v: &mut Value, key: Option<&macroscope_vault::Secret>) {
    match v {
        Value::String(s) => {
            if let Some(secret) = key
                .and_then(|s| std::str::from_utf8(s.expose()).ok())
                .filter(|s| !s.is_empty())
            {
                *s = s.replace(secret, "<redacted>");
            }
        }
        Value::Array(a) => {
            for v in a {
                redact_value(v, key)
            }
        }
        Value::Object(o) => {
            // Provider text can also put a credential in a property name.
            let values = std::mem::take(o);
            for (k, mut v) in values {
                let mut name = Value::String(k);
                redact_value(&mut name, key);
                redact_value(&mut v, key);
                o.insert(name.as_str().unwrap().into(), v);
            }
        }
        _ => {}
    }
}
#[derive(Debug, Clone, Copy)]
pub enum Provider {
    Meta,
    Echo,
}
pub fn serve_args(provider: Provider) -> Vec<&'static str> {
    let mut args = vec![
        "serve",
        "--provider",
        match provider {
            Provider::Meta => "meta",
            Provider::Echo => "echo",
        },
    ];
    if matches!(provider, Provider::Meta) {
        args.extend(["--model", "muse-spark-1.3"]);
    }
    args.extend([
        "--contributor-mode",
        "off",
        "--sandbox-network",
        "restricted",
        "--disable-shell",
    ]);
    args
}
pub struct MspClient {
    process: ManagedChild,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    stderr: tokio::task::JoinHandle<()>,
    next_id: u64,
    pending: VecDeque<Value>,
    env: ChildEnv,
    pub reducer: TurnReducer,
    pub setup: SetupState,
    transcript: Option<Vec<Value>>,
    probe_home: Option<tempfile::TempDir>,
    binary: BinaryInfo,
}
impl MspClient {
    pub async fn spawn(
        binary: &BinaryInfo,
        provider: Provider,
        env: ChildEnv,
        workspace: &Path,
    ) -> Result<Self, MuseError> {
        let store = JsonAcceptanceStore::new(env.state_dir());
        Self::spawn_with_store(binary, provider, env, workspace, &store).await
    }
    pub async fn spawn_with_store(
        binary: &BinaryInfo,
        provider: Provider,
        env: ChildEnv,
        workspace: &Path,
        store: &dyn AcceptanceStore,
    ) -> Result<Self, MuseError> {
        // Resolve schema safety in an isolated echo process before starting Meta.
        let (mut probe, init) = Self::spawn_echo_probe(binary, workspace, false).await?;
        probe.shutdown_inner().await?;
        let setup = ensure_conformance(binary, &init, store, workspace).await?;
        match &setup {
            SetupState::Ready { .. } => {}
            SetupState::ConformanceRequired {
                fingerprint,
                failing_step,
            } => {
                return Err(MuseError::ConformanceRequired {
                    fingerprint: fingerprint.clone(),
                    step: failing_step.clone().unwrap_or_else(|| "unchecked".into()),
                })
            }
            SetupState::SchemaMajorMismatch => return Err(MuseError::SchemaMajorMismatch),
            _ => return Err(MuseError::UnsupportedVersion),
        }
        let (mut client, actual) =
            Self::spawn_transport(binary, provider, env, workspace, false).await?;
        if actual["schema"] != init["schema"] {
            return Err(MuseError::BinaryChanged);
        }
        client.setup = setup;
        Ok(client)
    }
    pub(crate) async fn spawn_echo_probe(
        binary: &BinaryInfo,
        scratch: &Path,
        record: bool,
    ) -> Result<(Self, Value), MuseError> {
        // Lifetime of this temporary private HOME is owned by the caller of
        // conformance; the lightweight setup probe also retains it until exit.
        let root = tempfile::tempdir_in(scratch)?;
        let env = ChildEnv::for_route(
            crate::CredentialRoute::ApiKey,
            &root.path().join("home"),
            root.path(),
            Some(macroscope_vault::Secret::new(
                b"echo-only-conformance".to_vec(),
            )),
        )?;
        let (mut client, init) =
            Self::spawn_transport(binary, Provider::Echo, env, scratch, record).await?;
        client.probe_home = Some(root);
        Ok((client, init))
    }
    async fn spawn_transport(
        binary: &BinaryInfo,
        provider: Provider,
        env: ChildEnv,
        workspace: &Path,
        record: bool,
    ) -> Result<(Self, Value), MuseError> {
        binary.validate()?;
        let mut command = Command::new(&binary.path);
        env.apply(&mut command, true)?;
        command
            .args(serve_args(provider))
            .current_dir(workspace)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut process = ManagedChild::new(command)?;
        let stdin = process.child().stdin.take();
        let stdout = BufReader::new(process.child().stdout.take().ok_or(MuseError::Transport)?);
        let mut stderr = process.child().stderr.take().ok_or(MuseError::Transport)?;
        // Drain without persistence or logging. RPC carries authoritative typed
        // failures; raw stderr may contain credentials even during startup.
        let stderr = tokio::spawn(async move {
            let mut buf = [0; 4096];
            while let Ok(n) = stderr.read(&mut buf).await {
                if n == 0 {
                    break;
                }
            }
        });
        let mut client = Self {
            process,
            stdin,
            stdout,
            stderr,
            next_id: 0,
            pending: VecDeque::new(),
            env,
            reducer: TurnReducer::default(),
            setup: SetupState::Missing,
            transcript: record.then(Vec::new),
            probe_home: None,
            binary: binary.clone(),
        };
        let handshake = async {
        let init=client.request("initialize",json!({"clientInfo":{"name":"macroscope","version":env!("CARGO_PKG_VERSION")},"capabilities":{"requestedCapabilities":[]}})).await?;
        match binary.setup_state(&init) {
            SetupState::UnsupportedVersion => return Err(MuseError::UnsupportedVersion),
            SetupState::SchemaMajorMismatch => return Err(MuseError::SchemaMajorMismatch),
            _ => {}
        }
        if matches!(provider, Provider::Meta) && init["sandboxAvailability"] != "available" {
            return Err(MuseError::SandboxUnavailable);
        }
        client
            .send(&json!({"jsonrpc":"2.0","method":"initialized"}))
            .await?;
        Ok::<Value, MuseError>(init)
        }.await;
        let init = match handshake {
            Ok(init) => init,
            Err(error) => {
                client.stdin.take();
                client.process.kill_group();
                let _ = client.process.finish(Duration::from_secs(2)).await;
                return Err(error);
            }
        };
        Ok((client, init))
    }
    pub(crate) fn take_transcript(&mut self) -> Vec<Value> {
        self.transcript.take().unwrap_or_default()
    }
    fn record(&mut self, dir: &str, value: &Value) -> Result<(), MuseError> {
        if let Some(frames) = &mut self.transcript {
            if frames.len() >= 4096 {
                return Err(MuseError::Framing);
            }
            frames.push(json!({"dir": dir, "raw": serde_json::to_string(value).map_err(|_| MuseError::Framing)?}));
        }
        Ok(())
    }
    async fn send(&mut self, v: &Value) -> Result<(), MuseError> {
        self.record("c2s", v)?;
        let mut bytes = Vec::new();
        write_message(&mut bytes, v).map_err(|_| MuseError::Framing)?;
        let stdin = self.stdin.as_mut().ok_or(MuseError::Transport)?;
        stdin.write_all(&bytes).await?;
        stdin.flush().await?;
        Ok(())
    }
    async fn receive(&mut self) -> Result<Value, MuseError> {
        let mut value = read_frame(&mut self.stdout).await?;
        if value["jsonrpc"] != "2.0" {
            return Err(MuseError::Protocol);
        }
        redact_value(&mut value, self.env.key());
        self.record("s2c", &value)?;
        Ok(value)
    }
    // The allowlist makes it impossible to reach credential/account methods via
    // a generic public JSON-RPC API. Request ids, not arrival order, match replies.
    async fn request(&mut self, method: &str, params: Value) -> Result<Value, MuseError> {
        let value = self.request_reply(method, params).await?;
        if value.get("error").is_some() {
            return Err(classify_rpc_error(&value));
        }
        value.get("result").cloned().ok_or(MuseError::Protocol)
    }
    pub(crate) async fn unknown_method_probe(&mut self) -> Result<(), MuseError> {
        let value = self
            .request_reply("macroscope/conformanceUnknown", json!({}))
            .await?;
        if value["error"]["code"] != -32601 || value.get("result").is_some() {
            return Err(MuseError::Protocol);
        }
        // A valid reply after the error proves the server stayed alive.
        self.model_list().await?;
        Ok(())
    }
    async fn request_reply(&mut self, method: &str, params: Value) -> Result<Value, MuseError> {
        self.next_id += 1;
        let id = self.next_id;
        tokio::time::timeout(Duration::from_secs(20), async {
            self.send(&json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))
                .await?;
            loop {
                let value = self.receive().await?;
                if value["id"] == id {
                    return Ok(value);
                }
                if self.pending.len() >= 256 {
                    return Err(MuseError::Framing);
                }
                self.pending.push_back(value);
            }
        })
        .await
        .map_err(|_| MuseError::Timeout)?
    }
    pub async fn model_list(&mut self) -> Result<Value, MuseError> {
        self.request("model/list", json!({})).await
    }
    pub async fn session_start(&mut self, workspace: &Path) -> Result<String, MuseError> {
        let result = self
            .request(
                "session/start",
                json!({"commandId":uuid::Uuid::now_v7().to_string(),"workspaceRoot":workspace}),
            )
            .await?;
        result["session"]["sessionId"]
            .as_str()
            .map(String::from)
            .ok_or(MuseError::Protocol)
    }
    pub async fn session_resume(
        &mut self,
        session: &str,
        cursor: Option<&str>,
    ) -> Result<Value, MuseError> {
        let mut params = json!({"commandId":uuid::Uuid::now_v7().to_string(),"sessionId":session});
        if let Some(c) = cursor {
            params["cursor"] = c.into();
        }
        self.request("session/resume", params).await
    }
    pub async fn turn_start(&mut self, session: &str, text: &str) -> Result<String, MuseError> {
        let result=self.request("turn/start",json!({"commandId":uuid::Uuid::now_v7().to_string(),"sessionId":session,"input":[{"type":"text","text":text}]})).await?;
        if result["status"] != "accepted" {
            return Err(MuseError::Protocol);
        }
        let turn = result["turnId"]
            .as_str()
            .ok_or(MuseError::Protocol)?
            .to_string();
        self.reducer.acknowledge_turn(session.into(), turn.clone());
        Ok(turn)
    }
    pub async fn interrupt(&mut self) -> Result<(), MuseError> {
        let result=self.request("turn/interrupt",json!({"commandId":uuid::Uuid::now_v7().to_string(),"sessionId":self.reducer.session_id,"turnId":self.reducer.turn_id})).await?;
        if result["status"] != "accepted" {
            return Err(MuseError::Protocol);
        }
        Ok(())
    }
    pub async fn next_event(&mut self) -> Result<Option<BuildEvent>, MuseError> {
        let value = if let Some(v) = self.pending.pop_front() {
            v
        } else {
            tokio::time::timeout(Duration::from_secs(120), self.receive())
                .await
                .map_err(|_| MuseError::Timeout)??
        };
        // Capability requests are never treated as authority to approve tools.
        if value["method"] == "approval/request" || value["method"] == "userInput/request" {
            return Err(MuseError::Protocol);
        }
        Ok(self.reducer.apply(&value))
    }
    pub async fn wait_turn(&mut self) -> Result<String, MuseError> {
        tokio::time::timeout(Duration::from_secs(120), async {
            loop {
                match self.next_event().await? {
                    Some(BuildEvent::AuthRequired) => return Err(MuseError::AuthRequired),
                    Some(BuildEvent::Completed { terminal }) => {
                        return if terminal == "completed" {
                            Ok(self.reducer.text())
                        } else {
                            Err(MuseError::TurnFailed)
                        }
                    }
                    Some(BuildEvent::Quota { .. }) => return Err(MuseError::TurnFailed),
                    _ => {}
                }
            }
        })
        .await
        .map_err(|_| MuseError::Timeout)?
    }
    pub async fn shutdown(mut self) -> Result<(), MuseError> {
        self.shutdown_inner().await
    }
    pub(crate) async fn shutdown_inner(&mut self) -> Result<(), MuseError> {
        // MSP v1 has no shutdown method. EOF triggers orderly host unload.
        self.stdin.take();
        let status = self.process.finish(Duration::from_secs(5)).await;
        self.stderr.abort();
        match status {
            Ok(s) if s.success() => self.binary.validate(),
            Ok(_) => Err(MuseError::Transport),
            Err(e) => Err(e),
        }
    }
}
impl Drop for MspClient {
    fn drop(&mut self) {
        self.stderr.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn async_framing_rejects_oversize_and_partial_frames() {
        let bytes = vec![b'x'; MAX_MESSAGE_BYTES + 1];
        assert_eq!(
            read_frame(&mut BufReader::new(bytes.as_slice()))
                .await
                .unwrap_err(),
            MuseError::Framing
        );
        assert_eq!(
            read_frame(&mut BufReader::new(b"{}".as_slice()))
                .await
                .unwrap_err(),
            MuseError::Framing
        );
        assert_eq!(
            read_frame(&mut BufReader::new(b"{}\r\n".as_slice()))
                .await
                .unwrap(),
            json!({})
        );
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn process_deadline_kills_and_reaps_group() {
        let mut cmd = Command::new("/bin/sh");
        cmd.env_clear()
            .env("PATH", crate::SAFE_PATH)
            .args(["-c", "sleep 30 & wait"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = ManagedChild::new(cmd).unwrap();
        let mut output = child.child().stdout.take().unwrap();
        assert_eq!(
            child.finish(Duration::from_millis(20)).await.unwrap_err(),
            MuseError::Timeout
        );
        assert!(child.child().try_wait().unwrap().is_some());
        let mut bytes = vec![];
        tokio::time::timeout(Duration::from_secs(2), output.read_to_end(&mut bytes))
            .await
            .unwrap()
            .unwrap();
    }
}
