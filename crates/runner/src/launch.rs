//! Host-only child launcher. Production callers always use Seatbelt on macOS.
use crate::{Request, Response, OUTPUT_CAP, REQUEST_CAP};
use std::{
    io::{self, Read, Write},
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

pub const ADDRESS_SPACE_BYTES: u64 = 512 * 1024 * 1024;
pub const NOFILE: u64 = 32;
pub const CPU_SECONDS: u64 = 11;

/// Construct an empty-environment sandbox launch. Resource limits are applied
/// before sandbox-exec and inherited by the worker; no shell is involved.
pub fn sandbox_command(runner: &Path, profile: &Path) -> io::Result<Command> {
    #[cfg(target_os = "macos")]
    {
        let runner = runner.canonicalize()?;
        let profile = profile.canonicalize()?;
        let mut cmd = Command::new("/usr/bin/sandbox-exec");
        cmd.arg("-D")
            .arg(format!("RUNNER={}", runner.display()))
            .arg("-f")
            .arg(profile)
            .arg(runner);
        // macOS rejects any finite RLIMIT_AS (EINVAL), so it cannot be the memory
        // boundary here. The boundary is QuickJS's allocator limit, which fails every
        // JS allocation (objects, strings, inputs) past `limits.memory_bytes`; the
        // memory/worker-memory canaries prove it. Every other rlimit stays mandatory.
        configure(&mut cmd, false);
        Ok(cmd)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (runner, profile);
        Err(io::Error::other("OS sandbox not admitted on this platform"))
    }
}
/// Synthetic developer tests only; never a fallback after sandbox rejection.
pub fn synthetic_command(runner: &Path) -> Command {
    let mut cmd = Command::new(runner);
    configure(&mut cmd, false);
    cmd
}
fn configure(cmd: &mut Command, require_address_space: bool) {
    cmd.env_clear()
        .current_dir("/")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(not(unix))]
    let _ = require_address_space;
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            cmd.pre_exec(move || {
                if libc::setpgid(0, 0) != 0 {
                    return Err(io::Error::last_os_error());
                }
                for (resource, value) in [
                    (libc::RLIMIT_AS, ADDRESS_SPACE_BYTES),
                    (libc::RLIMIT_CPU, CPU_SECONDS),
                    (libc::RLIMIT_FSIZE, 0),
                    (libc::RLIMIT_NOFILE, NOFILE),
                    (libc::RLIMIT_NPROC, 0),
                    (libc::RLIMIT_CORE, 0),
                ] {
                    let limit = libc::rlimit {
                        rlim_cur: value as libc::rlim_t,
                        rlim_max: value as libc::rlim_t,
                    };
                    // macOS rejects finite RLIMIT_AS with EINVAL; callers decide
                    // whether to require it (see `sandbox_command`).
                    if resource == libc::RLIMIT_AS && !require_address_space {
                        continue;
                    }
                    if libc::setrlimit(resource, &limit) != 0 {
                        return Err(io::Error::last_os_error());
                    }
                }
                Ok(())
            });
        }
    }
}
/// Bounded pipe readers and a parent deadline cover compilation/native calls as
/// well as bytecode. Kill the group and reap before returning any output.
pub fn invoke(mut cmd: Command, request: &Request) -> io::Result<Response> {
    request
        .limits
        .validate()
        .map_err(|e| io::Error::other(e.message))?;
    let bytes = serde_json::to_vec(request)?;
    if bytes.len() > REQUEST_CAP {
        return Err(io::Error::other("request cap exceeded"));
    }
    let mut child = cmd.spawn()?;
    let pid = child.id();
    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let writer = std::thread::spawn(move || stdin.write_all(&bytes));
    let out = std::thread::spawn(move || read_pipe(stdout, OUTPUT_CAP));
    let err = std::thread::spawn(move || read_pipe(stderr, 64 * 1024));
    let started = Instant::now();
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if started.elapsed() > Duration::from_millis(request.limits.wall_ms + 1000) {
            timed_out = true;
            kill(pid);
            break child.wait()?;
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    // No descendants are permitted; also close inherited pipes on abnormal exit.
    kill(pid);
    let _ = writer.join();
    let output = out
        .join()
        .map_err(|_| io::Error::other("stdout reader failed"))??;
    let errors = err
        .join()
        .map_err(|_| io::Error::other("stderr reader failed"))??;
    if timed_out {
        return Ok(Response::failure("wall", "parent deadline exceeded"));
    }
    if !status.success() {
        return Err(io::Error::other(format!(
            "runner exited {status}: {}",
            String::from_utf8_lossy(&errors)
        )));
    }
    Response::from_wire(&output).map_err(io::Error::other)
}
fn read_pipe(reader: impl Read, cap: usize) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader.take(cap as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > cap {
        return Err(io::Error::other("runner pipe cap exceeded"));
    }
    Ok(bytes)
}
fn kill(pid: u32) {
    #[cfg(not(unix))]
    let _ = pid;
    #[cfg(unix)]
    unsafe {
        libc::kill(-(pid as i32), libc::SIGKILL);
    }
}
