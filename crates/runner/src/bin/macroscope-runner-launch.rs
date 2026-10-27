//! Developer harness entry point using the same pre_exec path as RunnerEngine.
use macroscope_runner::{encode_response, launch, Request, REQUEST_CAP};
use std::{
    io::{Read, Write},
    path::Path,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 4 || !matches!(args[1].as_str(), "--seatbelt" | "--synthetic") {
        return Err("usage: macroscope-runner-launch --seatbelt|--synthetic RUNNER PROFILE".into());
    }
    let mut bytes = vec![];
    std::io::stdin()
        .take(REQUEST_CAP as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > REQUEST_CAP {
        return Err("request cap exceeded".into());
    }
    let request: Request = serde_json::from_slice(&bytes)?;
    let command = if args[1] == "--seatbelt" {
        launch::sandbox_command(Path::new(&args[2]), Path::new(&args[3]))?
    } else {
        launch::synthetic_command(Path::new(&args[2]))
    };
    let response = launch::invoke(command, &request)?;
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    if unsafe { libc::getrusage(libc::RUSAGE_CHILDREN, &mut usage) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    #[cfg(target_os = "macos")]
    let peak = usage.ru_maxrss as u64;
    #[cfg(not(target_os = "macos"))]
    let peak = usage.ru_maxrss as u64 * 1024;
    eprintln!(
        "{}",
        serde_json::json!({"peak_rss_bytes":peak,
        "cpu_seconds":usage.ru_utime.tv_sec as f64+usage.ru_utime.tv_usec as f64/1e6+usage.ru_stime.tv_sec as f64+usage.ru_stime.tv_usec as f64/1e6})
    );
    std::io::stdout().write_all(&encode_response(&response))?;
    Ok(())
}
