//! Real Muse-plan build of the sample task through `AppCore` (owner-run smoke test).
//! Uses the signed-in Muse plan (no API key), a throwaway data dir, and an in-memory vault.
//!   cargo run -p macroscope-app-core --example muse_plan_build
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
    let preview = core.load_sample_csv().unwrap();
    let draft = core
        .draft_task(
            &preview.handle.handle_id,
            "Prepare the orders export for import",
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
    if let Some(m) = &state.candidate {
        println!("candidate mapping: {}", serde_json::to_string(m).unwrap());
    }
}
