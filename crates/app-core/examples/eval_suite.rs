//! Private generated-execution eval suite (plan §9.3).
//!
//!   cargo run -p macroscope-app-core --example eval_suite -- \
//!     --route <muse_plan|api_key|replay> --tasks <all|01,02,...> --reps N --out <dir>
//!
//! For each task × repetition a fresh temporary data folder and `AppCore`
//! (MemoryStore vault; the real home for `muse_plan` so the plan sign-in is
//! found, exactly as `muse_plan_build.rs` does) drafts the task from
//! `task.md` and `input.csv`, builds with the chosen route, saves, stages on
//! `input.csv`, commits, and scores the committed bytes against
//! `expected.csv` plus `hidden.json`. `--route replay` scores recorded model
//! answers from `evals/csv-family/replay/<id>.json` only to test scoring;
//! its results are labeled `"replay": true, "generation_quality":
//! "not measured"`. Never prints or stores input rows or outputs beyond a
//! pass/fail and a short diff summary (first differing line, ≤200 chars).
//!
//! `--route api_key` reads the key only from the app's vault flow or the
//! `META_API_KEY` environment variable, never argv, and refuses to start if
//! neither is present. It prints the planned maximum spend and requires
//! `--confirm-spend`. `--route muse_plan` uses the owner's existing Muse
//! plan sign-in read-only and never calls login/logout/auth. A usage-limit
//! failure stops the whole run and reports INCOMPLETE.

use macroscope_app_core::dto::{BuildPhase, ModelRoute, StageResult, TaskKind};
use macroscope_app_core::eval::{self, FailureClass, HiddenChecks};
use macroscope_app_core::{AppCore, CoreConfig, NativeMuse};
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../evals/csv-family")
}

fn usage() -> ! {
    eprintln!(
        "usage: eval_suite --route <muse_plan|api_key|replay> --tasks <all|01,02,...> --reps N --out <dir> [--confirm-spend]"
    );
    std::process::exit(2);
}

#[derive(Debug, Clone)]
struct Args {
    route: String,
    tasks: String,
    reps: usize,
    out: PathBuf,
    confirm_spend: bool,
}

fn parse_args() -> Args {
    let mut route: Option<String> = None;
    let mut tasks: Option<String> = None;
    let mut reps: Option<usize> = None;
    let mut out: Option<PathBuf> = None;
    let mut confirm_spend = false;
    let mut it = std::env::args().skip(1).peekable();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--route" => route = it.next(),
            "--tasks" => tasks = it.next(),
            "--reps" => {
                let v = it.next().unwrap_or_else(|| usage());
                reps = Some(v.parse().unwrap_or_else(|_| usage()));
            }
            "--out" => out = it.next().map(PathBuf::from),
            "--confirm-spend" => confirm_spend = true,
            _ => usage(),
        }
    }
    Args {
        route: route.unwrap_or_else(|| usage()),
        tasks: tasks.unwrap_or_else(|| "all".into()),
        reps: reps.unwrap_or(3),
        out: out.unwrap_or_else(|| usage()),
        confirm_spend,
    }
}

fn list_tasks(root: &Path, filter: &str) -> Vec<(String, PathBuf)> {
    let mut all: Vec<(String, PathBuf)> = Vec::new();
    let entries = std::fs::read_dir(root).expect("corpus dir is readable");
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.len() > 3
            && name.as_bytes()[2] == b'-'
            && name[..2].chars().all(|c| c.is_ascii_digit())
        {
            all.push((name[..2].to_string(), e.path()));
        }
    }
    all.sort();
    if filter == "all" {
        return all;
    }
    let want: Vec<&str> = filter.split(',').collect();
    all.into_iter()
        .filter(|(id, _)| want.contains(&id.as_str()))
        .collect()
}

#[derive(Debug, Clone, serde::Serialize)]
struct Attempt {
    task: String,
    rep: usize,
    pass: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    failure_class: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    diff: Option<String>,
    duration_ms: u64,
    model_calls: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    cost_micros: Option<i64>,
}

fn infra_keywords(message: &str) -> bool {
    let lower = message.to_lowercase();
    [
        "quota",
        "usage limit",
        "rate limit",
        "timed out",
        "timeout",
        "unavailable",
        "not signed in",
        "not installed",
        "sign in",
        "local only",
        "stopping",
        "transport",
        "billing",
        "budget",
        "no model",
    ]
    .iter()
    .any(|k| lower.contains(k))
}

fn core_error_class(message: &str, stage: &str) -> FailureClass {
    if message.to_lowercase().contains("forbidden") {
        return FailureClass::ForbiddenEffect;
    }
    if infra_keywords(message) {
        return FailureClass::Infrastructure;
    }
    if stage == "run" {
        FailureClass::ChecksFailed
    } else {
        FailureClass::BuildFailed
    }
}

async fn wait_terminal(
    core: &AppCore,
    build_id: &str,
) -> Option<macroscope_app_core::dto::BuildState> {
    for _ in 0..600 {
        match core.build_state(build_id) {
            Ok(s) if s.phase.is_terminal() => return Some(s),
            Ok(_) => tokio::time::sleep(std::time::Duration::from_secs(1)).await,
            Err(_) => return None,
        }
    }
    None
}

#[allow(clippy::too_many_lines)]
async fn run_generation(
    task_id: &str,
    task_dir: &Path,
    rep: usize,
    route: ModelRoute,
    api_key: Option<&str>,
    real_home: Option<PathBuf>,
) -> (Attempt, bool) {
    let started_at = std::time::Instant::now();
    let mut attempt = Attempt {
        task: task_id.into(),
        rep,
        pass: false,
        failure_class: None,
        diff: None,
        duration_ms: 0,
        model_calls: 0,
        cost_micros: None,
    };
    let mut usage_limited = false;
    let fail = |attempt: &mut Attempt, class: FailureClass, detail: &str| {
        attempt.failure_class = Some(class.as_str().into());
        let mut d: String = detail.chars().take(200).collect();
        if detail.chars().count() > 200 {
            d.push('…');
        }
        attempt.diff = Some(d);
        attempt.duration_ms = started_at.elapsed().as_millis() as u64;
    };
    let tmp = tempfile::tempdir().expect("tempdir");
    let data_dir = tmp.path().join("data");
    let home = if route == ModelRoute::MusePlan {
        real_home.clone().expect("HOME for muse_plan")
    } else {
        tmp.path().join("home")
    };
    let vault: Arc<dyn macroscope_vault::SecretStore> =
        Arc::new(macroscope_vault::MemoryStore::default());
    let core = match AppCore::new(CoreConfig {
        data_dir,
        vault,
        gateway_base_url: "https://api.meta.ai/v1".into(),
        home: home.clone(),
        muse: Arc::new(NativeMuse { home }),
        app_version: "eval-suite".into(),
        platform: std::env::consts::OS.into(),
        now_ms: CoreConfig::system_clock(),
    }) {
        Ok(c) => c,
        Err(e) => {
            fail(&mut attempt, FailureClass::Infrastructure, &e.to_string());
            return (attempt, false);
        }
    };
    if route == ModelRoute::ApiKey {
        let key = api_key.unwrap_or("").to_string();
        if key.trim().is_empty() {
            fail(
                &mut attempt,
                FailureClass::Infrastructure,
                "API key is missing",
            );
            return (attempt, false);
        }
        if core.save_api_key(&key).await.is_err() {
            fail(
                &mut attempt,
                FailureClass::Infrastructure,
                "API key was rejected",
            );
            return (attempt, false);
        }
        if core.select_route(ModelRoute::ApiKey).await.is_err() {
            fail(
                &mut attempt,
                FailureClass::Infrastructure,
                "route unavailable",
            );
            return (attempt, false);
        }
    } else {
        if core.select_route(ModelRoute::MusePlan).await.is_err() {
            fail(
                &mut attempt,
                FailureClass::Infrastructure,
                "route unavailable",
            );
            return (attempt, false);
        }
        if core.set_muse_unverified_ok(true).await.is_err() {
            fail(
                &mut attempt,
                FailureClass::Infrastructure,
                "route unavailable",
            );
            return (attempt, false);
        }
    }
    let description = std::fs::read_to_string(task_dir.join("task.md")).unwrap_or_default();
    let input_src = task_dir.join("input.csv");
    let input_path = tmp.path().join("input.csv");
    if std::fs::copy(&input_src, &input_path).is_err() {
        fail(
            &mut attempt,
            FailureClass::Infrastructure,
            "input could not be staged",
        );
        return (attempt, false);
    }
    let handle = match core.register_picked_file(&input_path) {
        Ok(h) => h,
        Err(e) => {
            fail(&mut attempt, FailureClass::Infrastructure, &e.to_string());
            return (attempt, false);
        }
    };
    let draft = match core.draft_task(&handle.handle_id, description.trim()) {
        Ok(d) => d,
        Err(e) => {
            fail(&mut attempt, FailureClass::BuildFailed, &e.to_string());
            return (attempt, false);
        }
    };
    let draft = match core.update_task(&draft.draft_id, TaskKind::Program, "output.csv", None) {
        Ok(d) => d,
        Err(e) => {
            fail(&mut attempt, FailureClass::BuildFailed, &e.to_string());
            return (attempt, false);
        }
    };
    if core.outbound_preview(&draft.draft_id).await.is_err() {
        fail(
            &mut attempt,
            FailureClass::Infrastructure,
            "outbound review unavailable",
        );
        return (attempt, false);
    }
    let started = match core.start_build(&draft.draft_id, true).await {
        Ok(s) => s,
        Err(e) => {
            let m = e.to_string();
            fail(&mut attempt, core_error_class(&m, "build"), &m);
            return (attempt, false);
        }
    };
    let state = match wait_terminal(&core, &started.build_id).await {
        Some(s) => s,
        None => {
            fail(
                &mut attempt,
                FailureClass::Infrastructure,
                "build timed out",
            );
            return (attempt, false);
        }
    };
    attempt.model_calls = state
        .events
        .iter()
        .filter(|e| e.message.contains("Candidate received"))
        .count();
    attempt.cost_micros = state.spent_micros;
    if state.phase != BuildPhase::Ready {
        let (kind, message) = match state.failure {
            Some(f) => (f.kind, f.message),
            None => (
                macroscope_app_core::dto::BuildFailureKind::Internal,
                "build did not become ready".to_string(),
            ),
        };
        if eval::is_usage_limit(kind, &message) {
            usage_limited = true;
        }
        let class = eval::classify_build_failure(kind, &message);
        fail(&mut attempt, class, &message);
        return (attempt, usage_limited);
    }
    if state
        .checks
        .iter()
        .any(|c| !c.passed || c.label.to_lowercase().contains("forbidden"))
    {
        let detail = state
            .checks
            .iter()
            .find(|c| !c.passed)
            .and_then(|c| c.detail.clone())
            .unwrap_or_else(|| "host checks failed".to_string());
        fail(&mut attempt, core_error_class(&detail, "run"), &detail);
        return (attempt, false);
    }
    let tool = match core
        .save_tool(&state.build_id, &format!("eval {task_id}"))
        .await
    {
        Ok(t) => t,
        Err(e) => {
            let m = e.to_string();
            fail(&mut attempt, core_error_class(&m, "run"), &m);
            return (attempt, false);
        }
    };
    let out_dir = tmp.path().join("out");
    if std::fs::create_dir_all(&out_dir).is_err() {
        fail(
            &mut attempt,
            FailureClass::Infrastructure,
            "output dir unavailable",
        );
        return (attempt, false);
    }
    let dir_handle = match core.register_picked_dir(&out_dir) {
        Ok(h) => h,
        Err(e) => {
            fail(&mut attempt, FailureClass::Infrastructure, &e.to_string());
            return (attempt, false);
        }
    };
    let staged = match core
        .stage_run(
            &tool.summary_row.id,
            &handle.handle_id,
            &dir_handle.handle_id,
            "output.csv",
        )
        .await
    {
        Ok(StageResult::Staged(s)) => s,
        Ok(StageResult::Failed(f)) => {
            let m = f.failure.message().to_string();
            fail(&mut attempt, core_error_class(&m, "run"), &m);
            return (attempt, false);
        }
        Err(e) => {
            let m = e.to_string();
            fail(&mut attempt, core_error_class(&m, "run"), &m);
            return (attempt, false);
        }
    };
    if !staged.planned_calls.is_empty() {
        fail(
            &mut attempt,
            FailureClass::ForbiddenEffect,
            "unapproved connection calls planned",
        );
        return (attempt, false);
    }
    let committed = match core.commit_run(&staged.run_id, &staged.plan_hash) {
        Ok(macroscope_app_core::dto::CommitResult::Committed(r)) => r,
        Ok(macroscope_app_core::dto::CommitResult::Failed(f)) => {
            let m = f.failure.message().to_string();
            fail(&mut attempt, core_error_class(&m, "run"), &m);
            return (attempt, false);
        }
        Err(e) => {
            let m = e.to_string();
            fail(&mut attempt, core_error_class(&m, "run"), &m);
            return (attempt, false);
        }
    };
    let _ = committed;
    let bytes = match std::fs::read(out_dir.join("output.csv")) {
        Ok(b) => b,
        Err(_) => {
            fail(
                &mut attempt,
                FailureClass::Infrastructure,
                "committed output unreadable",
            );
            return (attempt, false);
        }
    };
    let expected = std::fs::read(task_dir.join("expected.csv")).unwrap_or_default();
    let hidden: HiddenChecks =
        serde_json::from_slice(&std::fs::read(task_dir.join("hidden.json")).unwrap_or_default())
            .expect("hidden.json parses");
    match eval::score_output(&bytes, &expected, &hidden) {
        Ok(()) => {
            attempt.pass = true;
            attempt.duration_ms = started_at.elapsed().as_millis() as u64;
        }
        Err((class, summary)) => {
            fail(&mut attempt, class, &summary);
        }
    }
    (attempt, usage_limited)
}

fn score_replay(task_id: &str, task_dir: &Path, rep: usize) -> Attempt {
    let started_at = std::time::Instant::now();
    let mut attempt = Attempt {
        task: task_id.into(),
        rep,
        pass: false,
        failure_class: None,
        diff: None,
        duration_ms: 0,
        model_calls: 0,
        cost_micros: None,
    };
    let replay_path = corpus_dir().join("replay").join(format!("{task_id}.json"));
    let bytes = match std::fs::read(&replay_path) {
        Ok(b) => b,
        Err(_) => {
            attempt.failure_class = Some(FailureClass::Infrastructure.as_str().into());
            attempt.diff = Some("replay answer is missing".into());
            attempt.duration_ms = started_at.elapsed().as_millis() as u64;
            return attempt;
        }
    };
    let recorded: serde_json::Value = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(_) => {
            attempt.failure_class = Some(FailureClass::Infrastructure.as_str().into());
            attempt.diff = Some("replay answer is unreadable".into());
            attempt.duration_ms = started_at.elapsed().as_millis() as u64;
            return attempt;
        }
    };
    let output = recorded
        .get("output_csv")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let expected = std::fs::read(task_dir.join("expected.csv")).unwrap_or_default();
    let hidden: HiddenChecks =
        serde_json::from_slice(&std::fs::read(task_dir.join("hidden.json")).unwrap_or_default())
            .expect("hidden.json parses");
    match eval::score_output(output.as_bytes(), &expected, &hidden) {
        Ok(()) => {
            attempt.pass = true;
            attempt.duration_ms = started_at.elapsed().as_millis() as u64;
        }
        Err((class, summary)) => {
            attempt.failure_class = Some(class.as_str().into());
            attempt.diff = Some(summary);
            attempt.duration_ms = started_at.elapsed().as_millis() as u64;
        }
    }
    attempt
}

#[tokio::main]
async fn main() {
    let args = parse_args();
    if !["muse_plan", "api_key", "replay"].contains(&args.route.as_str()) {
        usage();
    }
    if args.reps == 0 || args.reps > 10 {
        eprintln!("--reps must be 1..=10");
        std::process::exit(2);
    }
    let root = corpus_dir();
    let tasks = list_tasks(&root, &args.tasks);
    if tasks.is_empty() {
        eprintln!("no tasks match --tasks");
        std::process::exit(2);
    }
    let api_key: Option<String> = if args.route == "api_key" {
        match std::env::var("META_API_KEY") {
            Ok(k) if !k.trim().is_empty() => Some(k),
            _ => {
                eprintln!("refusing to start: set META_API_KEY (never pass the key via argv)");
                std::process::exit(2);
            }
        }
    } else {
        None
    };
    if args.route == "api_key" {
        let max = eval::planned_max_spend(tasks.len(), args.reps);
        println!(
            "planned maximum spend: ${max:.2} ({} tasks x {} reps x $1.216)",
            tasks.len(),
            args.reps
        );
        if !args.confirm_spend {
            eprintln!("refusing to start: pass --confirm-spend for api_key");
            std::process::exit(2);
        }
    }
    let real_home: Option<PathBuf> = if args.route == "muse_plan" {
        Some(PathBuf::from(std::env::var_os("HOME").expect("HOME")))
    } else {
        None
    };
    let expected_total = tasks.len() * args.reps;
    let mut attempts: Vec<Attempt> = Vec::new();
    let mut stopped_early = false;
    if args.route == "replay" {
        for (id, dir) in &tasks {
            for rep in 1..=args.reps {
                let a = score_replay(id, dir, rep);
                if a.pass {
                    println!("{id} rep{rep} PASS");
                } else {
                    let d = a.diff.clone().unwrap_or_default();
                    println!(
                        "{id} rep{rep} FAIL {} {d}",
                        a.failure_class.clone().unwrap_or_default()
                    );
                }
                attempts.push(a);
            }
        }
    } else {
        let route = if args.route == "api_key" {
            ModelRoute::ApiKey
        } else {
            ModelRoute::MusePlan
        };
        'tasks: for (id, dir) in &tasks {
            for rep in 1..=args.reps {
                let (a, limited) =
                    run_generation(id, dir, rep, route, api_key.as_deref(), real_home.clone())
                        .await;
                if a.pass {
                    println!("{id} rep{rep} PASS ({}ms)", a.duration_ms);
                } else {
                    let d = a.diff.clone().unwrap_or_default();
                    println!(
                        "{id} rep{rep} FAIL {} {d}",
                        a.failure_class.clone().unwrap_or_default()
                    );
                }
                if limited {
                    println!(
                        "usage limit reached: stopping the run; remaining attempts are missing"
                    );
                    attempts.push(a);
                    stopped_early = true;
                    break 'tasks;
                }
                attempts.push(a);
            }
        }
    }
    let _ = stopped_early;
    let missing = expected_total.saturating_sub(attempts.len());
    let passed = attempts.iter().filter(|a| a.pass).count();
    let infra = attempts
        .iter()
        .filter(|a| a.failure_class.as_deref() == Some("infrastructure"))
        .count();
    let rate = if attempts.is_empty() {
        0.0
    } else {
        passed as f64 / attempts.len() as f64
    };
    let verdict = eval::verdict(passed, attempts.len(), missing, 0.75);
    let mut per_task = serde_json::Map::new();
    for (id, _) in &tasks {
        let mine: Vec<&Attempt> = attempts.iter().filter(|a| &a.task == id).collect();
        let p = mine.iter().filter(|a| a.pass).count();
        per_task.insert(
            id.clone(),
            serde_json::json!({"passed": p, "total": mine.len(), "rate": if mine.is_empty() { 0.0 } else { p as f64 / mine.len() as f64 }}),
        );
    }
    let results = serde_json::json!({
        "route": args.route,
        "replay": args.route == "replay",
        "generation_quality": if args.route == "replay" { "not measured" } else { "measured" },
        "tasks": tasks.iter().map(|(id, _)| id).collect::<Vec<_>>(),
        "reps": args.reps,
        "attempts": attempts,
        "per_task": per_task,
        "overall": {"passed": passed, "total": attempts.len(), "expected": expected_total, "missing": missing, "rate": rate, "infrastructure": infra},
        "threshold": 0.75,
        "verdict": verdict,
        "note": "Replay results test scoring only, not generation quality. Infrastructure failures are labeled separately and still counted in the denominator; a stopped suite reports INCOMPLETE with missing attempts.",
    });
    std::fs::create_dir_all(&args.out).expect("out dir");
    std::fs::write(
        args.out.join("results.json"),
        serde_json::to_string_pretty(&results).unwrap() + "\n",
    )
    .expect("results.json");
    let mut md = String::from("# csv-family results\n\n");
    md.push_str(&format!(
        "route: {} | reps: {} | verdict: {verdict}\n\n",
        args.route, args.reps
    ));
    md.push_str("| task | passed/total | rate |\n|---|---|---|\n");
    for (id, _) in &tasks {
        let v = &per_task[id];
        md.push_str(&format!(
            "| {id} | {}/{} | {:.2} |\n",
            v["passed"],
            v["total"],
            v["rate"].as_f64().unwrap_or(0.0)
        ));
    }
    md.push_str(&format!("\noverall: {passed}/{} ({rate:.2}), infrastructure: {infra}, missing: {missing}, verdict: {verdict}\n", attempts.len()));
    std::fs::write(args.out.join("results.md"), md).expect("results.md");
    println!(
        "verdict: {verdict} ({passed}/{} passed, {infra} infrastructure, {missing} missing)",
        attempts.len()
    );
}
