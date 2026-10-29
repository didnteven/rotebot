//! Builds and stages no-input web tasks from plain-language prompts and prints
//! the answer each one produces, so it can be checked against the live source.
//!
//!   META_API_KEY=... cargo run -p macroscope-app-core --example web_scenarios -- \
//!     --confirm-spend "prompt one" "prompt two" ...
//!
//! The key comes only from `META_API_KEY`, never argv. Each prompt gets a fresh
//! temporary data folder and an in-memory vault. Nothing is committed: the run
//! is staged and its preview printed, then the folder is discarded.

use macroscope_app_core::dto::{BuildPhase, ModelRoute, StageResult, TaskKind};
use macroscope_app_core::{AppCore, CoreConfig, NativeMuse};
use std::sync::Arc;

#[tokio::main]
async fn main() {
    let mut confirm = false;
    let mut prompts = Vec::new();
    for arg in std::env::args().skip(1) {
        if arg == "--confirm-spend" {
            confirm = true;
        } else {
            prompts.push(arg);
        }
    }
    let key = std::env::var("META_API_KEY").unwrap_or_default();
    if key.trim().is_empty() || !confirm || prompts.is_empty() {
        eprintln!(
            "usage: META_API_KEY=... web_scenarios --confirm-spend \"prompt\" [\"prompt\" ...]"
        );
        std::process::exit(2);
    }
    for prompt in prompts {
        println!("=== {prompt}");
        println!("{}", run_one(&prompt, &key).await);
    }
}

async fn run_one(prompt: &str, key: &str) -> String {
    let tmp = tempfile::tempdir().expect("tempdir");
    let home = tmp.path().join("home");
    let vault: Arc<dyn macroscope_vault::SecretStore> =
        Arc::new(macroscope_vault::MemoryStore::default());
    let open = |vault: Arc<dyn macroscope_vault::SecretStore>| {
        AppCore::new(CoreConfig {
            data_dir: tmp.path().join("data"),
            vault,
            gateway_base_url: "https://api.meta.ai/v1".into(),
            home: home.clone(),
            muse: Arc::new(NativeMuse { home: home.clone() }),
            app_version: "web-scenarios".into(),
            platform: std::env::consts::OS.into(),
            now_ms: CoreConfig::system_clock(),
        })
    };
    let core = match open(vault.clone()) {
        Ok(c) => c,
        Err(e) => return format!("SETUP FAILED: {e}"),
    };
    if core.save_api_key(key).await.is_err() || core.select_route(ModelRoute::ApiKey).await.is_err()
    {
        return "SETUP FAILED: key rejected".into();
    }
    let draft = match core.draft_task_no_input(prompt) {
        Ok(d) => d,
        Err(e) => return format!("DRAFT FAILED: {e}"),
    };
    let draft = match core.update_task(
        &draft.draft_id,
        TaskKind::Program,
        &std::env::var("SCENARIO_OUTPUT_NAME").unwrap_or_else(|_| "answer.txt".into()),
        std::env::var("SCENARIO_EXAMPLE").ok(),
    ) {
        Ok(d) => d,
        Err(e) => return format!("DRAFT FAILED: {e}"),
    };
    if let Err(e) = core.outbound_preview(&draft.draft_id).await {
        return format!("REVIEW FAILED: {e}");
    }
    let started = match core.start_build(&draft.draft_id, true).await {
        Ok(s) => s,
        Err(e) => return format!("BUILD START FAILED: {e}"),
    };
    let mut approved = false;
    let mut state = None;
    for _ in 0..300 {
        let Ok(s) = core.build_state(&started.build_id) else {
            break;
        };
        if s.phase == BuildPhase::Candidate && !approved {
            if let Some(program) = s.program.as_ref() {
                approved = core
                    .approve_build_sources(&started.build_id, program)
                    .is_ok();
            }
        }
        if s.phase.is_terminal() {
            state = Some(s);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
    let Some(state) = state else {
        return "BUILD TIMED OUT".into();
    };
    let steps: Vec<String> = state.events.iter().map(|e| e.message.clone()).collect();
    let steps = steps.join(" > ");
    if state.phase != BuildPhase::Ready {
        let why = state.failure.map(|f| f.message).unwrap_or_default();
        // SCENARIO_DEBUG=1 also shows the declared sources and the program, to see what the model wrote.
        let detail = if std::env::var("SCENARIO_DEBUG").is_ok() {
            state.program.as_ref().map(|p| {
                let sources: Vec<String> = p.web_sources.iter().map(|w| format!("{}={}", w.name, w.url)).collect();
                format!("\n  declared: {}\n  code: {}", sources.join(", "), p.code.chars().take(1500).collect::<String>())
            }).unwrap_or_default()
        } else { String::new() };
        return format!("BUILD FAILED: {why}\n  steps: {steps}{detail}");
    }
    let hosts = state
        .program
        .as_ref()
        .map(|p| {
            p.web_sources
                .iter()
                .map(|s| s.url.clone())
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();
    let tool = match core.save_tool(&state.build_id, "scenario").await {
        Ok(t) => t,
        Err(e) => return format!("SAVE FAILED: {e}"),
    };
    match core.preview_run_no_input(&tool.summary_row.id).await {
        Ok(StageResult::Staged(p)) => eprintln!(
            "preview ok: {}",
            p.preview_text
                .unwrap_or_default()
                .lines()
                .next()
                .unwrap_or("")
        ),
        Ok(StageResult::Failed(f)) => eprintln!("preview failed: {}", f.failure.message()),
        Err(e) => eprintln!("preview error: {e}"),
    }
    eprintln!(
        "scratch left behind: {}",
        std::fs::read_dir(tmp.path().join("data/previews"))
            .map(|d| d.count())
            .unwrap_or(0)
    );
    // SCENARIO_RESTART=1 reopens the core on the same data folder, as the app
    // does when it is closed and opened again, before the saved tool is run.
    let core = if std::env::var("SCENARIO_RESTART").is_ok() {
        drop(core);
        match open(vault.clone()) {
            Ok(c) => c,
            Err(e) => return format!("REOPEN FAILED: {e}"),
        }
    } else {
        core
    };
    let program_name = state
        .program
        .as_ref()
        .map(|p| p.output_name.clone())
        .unwrap_or_default();
    eprintln!("program output_name = {program_name}");
    let run_name = std::env::var("SCENARIO_RUN_NAME").unwrap_or_else(|_| program_name.clone());
    let out_dir = tmp.path().join("out");
    std::fs::create_dir_all(&out_dir).expect("out dir");
    let dir = match core.register_picked_dir(&out_dir) {
        Ok(h) => h,
        Err(e) => return format!("RUN SETUP FAILED: {e}"),
    };
    match core
        .stage_run_no_input(&tool.summary_row.id, &dir.handle_id, &run_name)
        .await
    {
        Ok(StageResult::Staged(s)) => {
            let committed = match core.commit_run(&s.run_id, &s.plan_hash) {
                Ok(_) => format!(
                    "created {:?}",
                    std::fs::read_dir(&out_dir)
                        .map(|d| d.flatten().map(|e| e.file_name()).collect::<Vec<_>>())
                        .unwrap_or_default()
                ),
                Err(e) => format!("commit failed: {e}"),
            };
            format!(
                "OK\n  sources: {hosts}\n  steps: {steps}\n  file: {committed}\n  answer: {}",
                s.preview_text.unwrap_or_default().trim()
            )
        }
        Ok(StageResult::Failed(f)) => {
            format!("RUN FAILED: {}\n  sources: {hosts}", f.failure.message())
        }
        Err(e) => format!("RUN FAILED: {e}\n  sources: {hosts}"),
    }
}
