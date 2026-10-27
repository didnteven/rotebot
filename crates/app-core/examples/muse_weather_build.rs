//! Real Muse-plan build of a no-input web-source task through `AppCore` (owner-run smoke test).
//! Fetches the real Open-Meteo API (public, no key) through the host; the sandbox never fetches.
//!   cargo build -p macroscope-runner --bins
//!   cargo run -p macroscope-app-core --example muse_weather_build
use macroscope_app_core::dto::{BuildPhase, CommitResult, ModelRoute, StageResult, TaskKind};
use macroscope_app_core::{AppCore, CoreConfig, NativeMuse};
use std::sync::Arc;

#[tokio::main]
async fn main() {
    let home = std::path::PathBuf::from(std::env::var_os("HOME").expect("HOME"));
    let tmp = tempfile::tempdir().unwrap();
    let core = AppCore::new(CoreConfig {
        data_dir: tmp.path().join("data"),
        vault: Arc::new(macroscope_vault::MemoryStore::default()),
        gateway_base_url: "https://api.meta.ai/v1".into(),
        home: home.clone(),
        muse: Arc::new(NativeMuse { home }),
        app_version: "smoke".into(),
        platform: "macos".into(),
        now_ms: CoreConfig::system_clock(),
    })
    .unwrap();
    println!("muse: {:?}", core.refresh_muse_status().await.muse);
    core.select_route(ModelRoute::MusePlan).await.unwrap();
    core.set_muse_unverified_ok(true).await.unwrap();
    let draft = core
        .draft_task_no_input(
            "Get today's weather forecast for Auckland, New Zealand (latitude -36.85, longitude 174.76) \
             from the Open-Meteo API (https://api.open-meteo.com/v1/forecast, daily max/min temperature \
             and precipitation, timezone Pacific/Auckland) and write a short plain-text summary to weather.txt",
        )
        .unwrap();
    let draft = core
        .update_task(&draft.draft_id, TaskKind::Program, "weather.txt", None)
        .unwrap();
    let outbound = core.outbound_preview(&draft.draft_id).await.unwrap();
    println!("destination: {}", outbound.destination);
    let started = core.start_build(&draft.draft_id, true).await.unwrap();
    let t0 = std::time::Instant::now();
    let mut state = core.build_state(&started.build_id).unwrap();
    while !matches!(
        state.phase,
        BuildPhase::Ready | BuildPhase::Failed | BuildPhase::Stopped
    ) && t0.elapsed().as_secs() < 600
    {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        state = core.build_state(&started.build_id).unwrap();
    }
    println!(
        "phase: {:?} after {}s via {}",
        state.phase,
        t0.elapsed().as_secs(),
        state.route_label
    );
    for c in &state.checks {
        println!(
            "  check: {} {}",
            if c.passed { "PASS" } else { "FAIL" },
            c.label
        );
    }
    if let Some(f) = &state.failure {
        println!("failure: {:?} {}", f.kind, f.message);
    }
    if let Some(p) = &state.program {
        println!(
            "web sources: {:?}",
            p.web_sources
                .iter()
                .map(|s| s.url.clone())
                .collect::<Vec<_>>()
        );
    }
    assert_eq!(state.phase, BuildPhase::Ready, "{:?}", state.failure);
    let tool = core
        .save_tool(&state.build_id, "Auckland weather")
        .await
        .unwrap();
    let folder = core.register_picked_dir(tmp.path()).unwrap();
    let StageResult::Staged(staged) = core
        .stage_run_no_input(&tool.summary_row.id, &folder.handle_id, "weather.txt")
        .await
        .unwrap()
    else {
        panic!("stage failed")
    };
    let CommitResult::Committed(receipt) =
        core.commit_run(&staged.run_id, &staged.plan_hash).unwrap()
    else {
        panic!("commit failed")
    };
    println!("committed: {}", receipt.output_path);
    println!(
        "--- weather.txt ---\n{}",
        std::fs::read_to_string(&receipt.output_path).unwrap()
    );
    let outbound: Vec<_> = core
        .list_outbound()
        .unwrap()
        .into_iter()
        .map(|o| format!("{} -> {}", o.purpose, o.destination))
        .collect();
    println!("outbound log: {outbound:?}");
}
