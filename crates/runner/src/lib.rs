//! One-shot data-only QuickJS worker. No loader or OS bindings are registered.
pub mod launch;
mod sdk;

use rquickjs::{context::intrinsic, Context, Function, Runtime};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

pub const REQUEST_CAP: usize = 20_000_000;
pub const OUTPUT_CAP: usize = 20_000_000;
pub const MEMORY_CAP: usize = 64 * 1024 * 1024;
pub const STACK_CAP: usize = 512 * 1024;
pub const LOG_CAP: usize = 1_048_576;
pub const RQUICKJS_VERSION: &str = "0.14.0";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Limits {
    pub memory_bytes: usize,
    pub stack_bytes: usize,
    pub cpu_ms: u64,
    pub wall_ms: u64,
    pub output_bytes: usize,
    pub log_bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            memory_bytes: MEMORY_CAP,
            stack_bytes: STACK_CAP,
            cpu_ms: 10_000,
            wall_ms: 12_000,
            output_bytes: OUTPUT_CAP,
            log_bytes: LOG_CAP,
        }
    }
}
impl Limits {
    pub fn validate(&self) -> Result<(), Failure> {
        let max = Self::default();
        if self.memory_bytes < 1024 * 1024
            || self.memory_bytes > max.memory_bytes
            || self.stack_bytes < 64 * 1024
            || self.stack_bytes > max.stack_bytes
            || self.cpu_ms == 0
            || self.cpu_ms > max.cpu_ms
            || self.wall_ms == 0
            || self.wall_ms > max.wall_ms
            || self.output_bytes == 0
            || self.output_bytes > max.output_bytes
            || self.log_bytes == 0
            || self.log_bytes > max.log_bytes
        {
            return Err(Failure::new(
                "forbidden",
                "limits must be positive and cannot exceed host ceilings",
            ));
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub code: String,
    pub inputs: BTreeMap<String, String>,
    #[serde(default)]
    pub limits: Limits,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Output {
    pub name: String,
    pub text: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Failure {
    pub kind: String,
    pub message: String,
}
impl Failure {
    pub fn new(kind: &str, message: &str) -> Self {
        Self {
            kind: kind.into(),
            message: message.into(),
        }
    }
}
#[derive(Debug, Clone)]
pub enum Response {
    Success {
        outputs: Vec<Output>,
        log: Vec<String>,
    },
    Error {
        error: Failure,
    },
}
impl Serialize for Response {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        match self {
            Self::Success { outputs, log } => {
                let mut wire = serializer.serialize_struct("Response", 3)?;
                wire.serialize_field("ok", &true)?;
                wire.serialize_field("outputs", outputs)?;
                wire.serialize_field("log", log)?;
                wire.end()
            }
            Self::Error { error } => {
                let mut wire = serializer.serialize_struct("Response", 2)?;
                wire.serialize_field("ok", &false)?;
                wire.serialize_field("error", error)?;
                wire.end()
            }
        }
    }
}
/// Bound serialized bytes too: escaping must not multiply host allocations.
pub fn encode_response(response: &Response) -> Vec<u8> {
    struct Capped(Vec<u8>);
    impl std::io::Write for Capped {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > OUTPUT_CAP.saturating_sub(self.0.len()) {
                return Err(std::io::Error::other("response cap"));
            }
            self.0.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut out = Capped(vec![]);
    if serde_json::to_writer(&mut out, response).is_err() {
        return serde_json::to_vec(&Response::failure(
            "output_limit",
            "serialized response exceeds 20 MB",
        ))
        .unwrap();
    }
    out.0
}
impl Response {
    pub fn failure(kind: &str, message: &str) -> Self {
        Self::Error {
            error: Failure::new(kind, message),
        }
    }
    pub fn from_wire(bytes: &[u8]) -> Result<Self, String> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            ok: bool,
            outputs: Option<Vec<Output>>,
            log: Option<Vec<String>>,
            error: Option<Failure>,
        }
        let v: Wire = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        match (v.ok, v.outputs, v.log, v.error) {
            (true, Some(outputs), Some(log), None) => Ok(Self::Success { outputs, log }),
            (false, None, None, Some(error)) => Ok(Self::Error { error }),
            _ => Err("invalid runner response".into()),
        }
    }
}

#[derive(Default)]
struct State {
    outputs: Vec<Output>,
    log: Vec<String>,
    log_bytes: usize,
    failure: Option<Failure>,
}
type Shared = Arc<Mutex<State>>;

fn cpu_time() -> Duration {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // Process CPU clock includes time in native SDK calls; it has no OS authority.
    unsafe {
        libc::clock_gettime(libc::CLOCK_PROCESS_CPUTIME_ID, &mut ts);
    }
    Duration::new(ts.tv_sec as u64, ts.tv_nsec as u32)
}

pub fn execute(request: Request) -> Response {
    if let Err(error) = request.limits.validate() {
        return Response::Error { error };
    }
    if request.code.len()
        + request
            .inputs
            .iter()
            .map(|(k, v)| k.len() + v.len())
            .sum::<usize>()
        > REQUEST_CAP
    {
        return Response::failure("forbidden", "request exceeds 20 MB");
    }
    // Deliberately conservative source admission: rejects this byte sequence even
    // inside comments/strings. No parser evasion can introduce a loader: none is
    // installed, and every string-code constructor is removed before user code.
    if request.code.contains("import") {
        return Response::failure("syntax", "imports are forbidden");
    }
    let state: Shared = Arc::default();
    let rt = match Runtime::new() {
        Ok(rt) => rt,
        Err(_) => return Response::failure("memory", "runtime allocation failed"),
    };
    rt.set_memory_limit(request.limits.memory_bytes);
    rt.set_max_stack_size(request.limits.stack_bytes);
    let started = Instant::now();
    let cpu = cpu_time();
    let limits = request.limits.clone();
    let interrupted = state.clone();
    rt.set_interrupt_handler(Some(Box::new(move || {
        let kind = if started.elapsed() >= Duration::from_millis(limits.wall_ms) {
            Some("wall")
        } else if cpu_time().saturating_sub(cpu) >= Duration::from_millis(limits.cpu_ms) {
            Some("cpu")
        } else {
            None
        };
        let mut state = interrupted.lock().unwrap();
        if let Some(kind) = kind {
            state
                .failure
                .get_or_insert_with(|| Failure::new(kind, "execution budget exceeded"));
        }
        state.failure.is_some()
    })));
    // No Date, RegExp, Proxy, shared buffers, weak refs or job executor.
    // Promise intrinsics are needed for correct async-function teardown; the
    // Promise global is removed and returned promises are refused.
    let context = match Context::custom::<(
        intrinsic::Eval,
        intrinsic::Json,
        intrinsic::MapSet,
        intrinsic::TypedArrays,
        intrinsic::Promise,
    )>(&rt)
    {
        Ok(ctx) => ctx,
        Err(_) => return Response::failure("memory", "context allocation failed"),
    };
    let result = context.with(|ctx| {
        let classify: Function = ctx.eval(include_str!("classify.js"))?;
        sdk::install(&ctx, request.inputs, &request.limits, state.clone())?;
        ctx.eval::<(), _>(include_str!("harden.js"))?;
        let user_result = (|| {
            ctx.eval::<(), _>(request.code)?;
            let run: Function = ctx.globals().get("run")?;
            let value: rquickjs::Value = run.call(())?;
            if value.is_promise() {
                return Err(rquickjs::Exception::throw_type(
                    &ctx,
                    "run must be synchronous",
                ));
            }
            Ok::<_, rquickjs::Error>(())
        })();
        if let Err(err) = user_result {
            let exception = ctx.catch();
            let kind: String = classify
                .call((exception,))
                .unwrap_or_else(|_| "runtime".into());
            let kind = if matches!(err, rquickjs::Error::Allocation) {
                "memory"
            } else {
                &kind
            };
            return Ok(Some(Failure::new(kind, "JavaScript execution failed: use synchronous SDK code only; eval, Function, Worker, Date, Math.random and OS APIs are unavailable")));
        }
        Ok::<_, rquickjs::Error>(None)
    });
    let mut state = state.lock().unwrap();
    if let Some(error) = state.failure.take() {
        return Response::Error { error };
    }
    match result {
        Ok(Some(error)) => return Response::Error { error },
        Err(_) => return Response::failure("memory", "runtime initialization failed"),
        Ok(None) => {}
    }
    if started.elapsed() >= Duration::from_millis(request.limits.wall_ms) {
        return Response::failure("wall", "execution budget exceeded");
    }
    if cpu_time().saturating_sub(cpu) >= Duration::from_millis(request.limits.cpu_ms) {
        return Response::failure("cpu", "execution budget exceeded");
    }
    Response::Success {
        outputs: std::mem::take(&mut state.outputs),
        log: std::mem::take(&mut state.log),
    }
}
