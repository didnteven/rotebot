//! Real Muse-plan build of the log extraction task through `AppCore` (owner-run smoke test).
//! Uses the signed-in Muse plan (no API key), a throwaway data dir, and an in-memory vault.
//!   cargo build -p macroscope-runner --bins
//!   cargo run -p macroscope-app-core --example muse_program_build
use macroscope_app_core::dto::{BuildPhase, ModelRoute};
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
    let access = core.refresh_muse_status().await;
    println!("muse: {:?}", access.muse);
    core.select_route(ModelRoute::MusePlan).await.unwrap();
    let access = core.set_muse_unverified_ok(true).await.unwrap();
    println!("effective route: {:?}", access.effective_route);
    let input = tmp.path().join("synthetic.log");
    std::fs::write(
        &input,
        "INFO synthetic start\nERROR synthetic failed\nINFO synthetic done\n",
    )
    .unwrap();
    let file = core.register_picked_file(&input).unwrap();
    let draft = core
        .draft_task(
            &file.handle_id,
            "extract every line containing ERROR from a log into errors.txt; preserve each retained line verbatim and terminate it with a newline",
        )
        .unwrap();
    let draft = core
        .update_task(
            &draft.draft_id,
            macroscope_app_core::dto::TaskKind::Program,
            "errors.txt",
            None,
        )
        .unwrap();
    let outbound = core.outbound_preview(&draft.draft_id).await.unwrap();
    println!(
        "destination: {} ({} bytes of synthetic packet)",
        outbound.destination,
        outbound.payload_text.len()
    );
    let started = core.start_build(&draft.draft_id, true).await.unwrap();
    let started_at = std::time::Instant::now();
    let mut state = core.build_state(&started.build_id).unwrap();
    while !matches!(
        state.phase,
        BuildPhase::Ready | BuildPhase::Failed | BuildPhase::Stopped
    ) && started_at.elapsed().as_secs() < 600
    {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        state = core.build_state(&started.build_id).unwrap();
    }
    println!(
        "phase: {:?} after {}s via {}",
        state.phase,
        started_at.elapsed().as_secs(),
        state.route_label
    );
    for e in &state.events {
        println!("  event: {:?} {}", e.phase, e.message);
    }
    for c in &state.checks {
        println!(
            "  check: {} {} {}",
            if c.passed { "PASS" } else { "FAIL" },
            c.label,
            c.detail.clone().unwrap_or_default()
        );
    }
    if let Some(f) = &state.failure {
        println!("failure: {:?} {}", f.kind, f.message);
    }
    if let Some(program) = &state.program {
        println!("candidate program: {}", program.code);
    }
    assert_eq!(state.phase, BuildPhase::Ready, "{:?}", state.failure);
    let tool = core
        .save_tool(&state.build_id, "Extract ERROR lines")
        .await
        .unwrap();
    let folder = core.register_picked_dir(tmp.path()).unwrap();
    let stage = core
        .stage_run(
            &tool.summary_row.id,
            &file.handle_id,
            &folder.handle_id,
            "errors.txt",
        )
        .await
        .unwrap();
    let macroscope_app_core::dto::StageResult::Staged(stage) = stage else {
        panic!("{stage:?}")
    };
    let receipt = core.commit_run(&stage.run_id, &stage.plan_hash).unwrap();
    let macroscope_app_core::dto::CommitResult::Committed(receipt) = receipt else {
        panic!("{receipt:?}")
    };
    assert_eq!(receipt.plan_hash, stage.plan_hash);
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("errors.txt")).unwrap(),
        "ERROR synthetic failed\n"
    );
    println!(
        "committed: {} (plan {})",
        receipt.output_path, receipt.plan_hash
    );
}
