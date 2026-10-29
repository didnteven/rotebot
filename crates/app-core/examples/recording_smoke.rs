//! Sends the built-in synthetic sample recording through the real analysis
//! path (preview, hash-bound consent, model call, draft) using the Meta API key
//! route, to check that the provider accepts the masked images and returns a
//! parseable task draft. The key comes only from `META_API_KEY`.

use macroscope_app_core::dto::ModelRoute;
use macroscope_app_core::recording::RecordingSource;
use macroscope_app_core::{AppCore, CoreConfig, NativeMuse};
use std::sync::Arc;

#[tokio::main]
async fn main() {
    let key = std::env::var("META_API_KEY").unwrap_or_default();
    if key.trim().is_empty() || !std::env::args().any(|a| a == "--confirm-spend") {
        eprintln!("usage: META_API_KEY=... recording_smoke --confirm-spend");
        std::process::exit(2);
    }
    let tmp = tempfile::tempdir().expect("tempdir");
    let home = tmp.path().join("home");
    let core = AppCore::new(CoreConfig {
        data_dir: tmp.path().join("data"),
        vault: Arc::new(macroscope_vault::MemoryStore::default()),
        gateway_base_url: "https://api.meta.ai/v1".into(),
        home: home.clone(),
        muse: Arc::new(NativeMuse { home }),
        app_version: "recording-smoke".into(),
        platform: std::env::consts::OS.into(),
        now_ms: CoreConfig::system_clock(),
    })
    .expect("core");
    core.save_api_key(&key).await.expect("key");
    core.select_route(ModelRoute::ApiKey).await.expect("route");
    let id = core
        .start_recording(RecordingSource::Sample)
        .expect("start");
    let frames = core.stop_recording(&id).expect("stop");
    println!("frames kept: {}", frames.frames.len());
    let preview = core.preview_recording(&id).await.expect("preview");
    println!(
        "preview: {} bytes to {} ({}), est {:?}",
        preview.total_bytes, preview.recipient, preview.model, preview.cost
    );
    match core.consent_and_analyze(&id, &preview.payload_hash).await {
        Ok(draft) => println!(
            "DRAFT OK\n{}",
            serde_json::to_string_pretty(&draft).unwrap()
        ),
        Err(e) => println!("ANALYZE FAILED: {e}"),
    }
}
