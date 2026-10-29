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
    // RECORDING_SMOKE_BIG=1 adds six large, browser-sized frames with many labels, to check that the
    // provider accepts a payload of the size a real browser recording produces.
    if std::env::var("RECORDING_SMOKE_BIG").is_ok() {
        let (w, h) = (1280usize, 720usize);
        for n in 0..6u64 {
            let mut rgba = vec![255u8; w * h * 4];
            let mut x = 0x9e37_79b9_u64 + n;
            for by in (0..h).step_by(6) {
                for bx in (0..w).step_by(6) {
                    x ^= x << 13; x ^= x >> 7; x ^= x << 17;
                    let c = [(x & 0xff) as u8, ((x >> 8) & 0xff) as u8, ((x >> 16) & 0xff) as u8];
                    for yy in by..(by + 6).min(h) {
                        for xx in bx..(bx + 6).min(w) {
                            let i = (yy * w + xx) * 4;
                            rgba[i..i + 3].copy_from_slice(&c);
                        }
                    }
                }
            }
            let labels: Vec<String> = (0..120).map(|i| format!("Bookmark label number {i}")).collect();
            let dto: macroscope_app_core::recording::RecordedFrameDto = serde_json::from_value(serde_json::json!({
                "seq": 10 + n, "monoMs": 10_000 + n * 1000, "appName": "Google Chrome", "windowTitle": "",
                "width": w, "height": h, "rgba": rgba, "axLabels": labels,
                "ocrBoxes": [{"text": "Overview of plan limits", "rect": {"x": 10, "y": 10, "width": 200, "height": 20}}],
                "masks": [], "uncertain": n == 0, "secureInputGap": false
            })).expect("frame");
            core.submit_frame(&id, dto).expect("submit");
        }
    }
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
