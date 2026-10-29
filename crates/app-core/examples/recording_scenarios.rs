//! Runs scripted browser recordings through the real analysis and builder path and prints the
//! recommended plan, the built tool's answer and any failure, so the plans can be judged.
//!
//!   META_API_KEY=... cargo run -p macroscope-app-core --example recording_scenarios -- \
//!     --confirm-spend scenarios.json images_dir [scenario_name ...]
//!
//! The scenario file lists frames as text lines with positions; `images_dir/<name>-<n>.png`
//! holds the matching rendered frame. The key comes only from `META_API_KEY`.

use macroscope_app_core::dto::{BuildPhase, ModelRoute, StageResult, TaskKind};
use macroscope_app_core::recording::{RecordedFrameDto, RecordingSource};
use macroscope_app_core::{AppCore, CoreConfig, NativeMuse};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::Arc;

fn decode(path: &std::path::Path) -> (u32, u32, Vec<u8>) {
    let mut reader = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path).expect("png")))
        .read_info()
        .expect("info");
    let mut buf = vec![0; reader.output_buffer_size().expect("size")];
    let info = reader.next_frame(&mut buf).expect("frame");
    let data = &buf[..info.buffer_size()];
    let rgba = match info.color_type {
        png::ColorType::Rgba => data.to_vec(),
        png::ColorType::Rgb => data.chunks(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
        other => panic!("unsupported png colour {other:?}"),
    };
    (info.width, info.height, rgba)
}

#[tokio::main]
async fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let confirm = args.iter().any(|a| a == "--confirm-spend");
    args.retain(|a| a != "--confirm-spend");
    let key = std::env::var("META_API_KEY").unwrap_or_default();
    if key.trim().is_empty() || !confirm || args.len() < 2 {
        eprintln!("usage: META_API_KEY=... recording_scenarios --confirm-spend scenarios.json images_dir [name ...]");
        std::process::exit(2);
    }
    let scenarios: Vec<Value> =
        serde_json::from_slice(&std::fs::read(&args[0]).expect("scenarios")).expect("json");
    let images = std::path::PathBuf::from(&args[1]);
    let only: Vec<&String> = args[2..].iter().collect();
    for sc in scenarios {
        let name = sc["name"].as_str().unwrap().to_string();
        if !only.is_empty() && !only.iter().any(|o| **o == name) {
            continue;
        }
        println!("=== {name}\n  expect: {}", sc["expect"].as_str().unwrap_or(""));
        println!("{}", run_one(&sc, &name, &images, &key).await);
    }
}

async fn run_one(sc: &Value, name: &str, images: &std::path::Path, key: &str) -> String {
    let tmp = tempfile::tempdir().expect("tempdir");
    let home = tmp.path().join("home");
    let core = match AppCore::new(CoreConfig {
        data_dir: tmp.path().join("data"),
        vault: Arc::new(macroscope_vault::MemoryStore::default()),
        gateway_base_url: "https://api.meta.ai/v1".into(),
        home: home.clone(),
        muse: Arc::new(NativeMuse { home }),
        app_version: "recording-scenarios".into(),
        platform: std::env::consts::OS.into(),
        now_ms: CoreConfig::system_clock(),
    }) {
        Ok(c) => c,
        Err(e) => return format!("  SETUP FAILED: {e}"),
    };
    if core.save_api_key(key).await.is_err() || core.select_route(ModelRoute::ApiKey).await.is_err() {
        return "  SETUP FAILED: key".into();
    }
    let id = core.start_recording(RecordingSource::Sample).expect("start");
    for (n, f) in sc["frames"].as_array().unwrap().iter().enumerate() {
        let (w, h, rgba) = decode(&images.join(format!("{name}-{}.png", n + 1)));
        let mut labels = vec![f["title"].as_str().unwrap_or("").to_string(), f["url"].as_str().unwrap_or("").to_string()];
        let mut boxes = vec![
            json!({"text": f["url"], "rect": {"x": 104, "y": 48, "width": 600, "height": 20}}),
            json!({"text": f["title"], "rect": {"x": 20, "y": 10, "width": 300, "height": 16}}),
        ];
        for l in f["lines"].as_array().unwrap() {
            let t = l["t"].as_str().unwrap();
            let size = l["s"].as_f64().unwrap_or(16.0);
            labels.push(t.to_string());
            boxes.push(json!({"text": t, "rect": {
                "x": l["x"].as_f64().unwrap() as u32, "y": l["y"].as_f64().unwrap() as u32,
                "width": ((t.chars().count() as f64 * size * 0.55) as u32).clamp(10, 1200),
                "height": (size * 1.3) as u32}}));
        }
        let dto: RecordedFrameDto = serde_json::from_value(json!({
            "seq": 10 + n, "monoMs": 10_000 + n as u64 * 4000, "appName": "Google Chrome", "windowTitle": "",
            "width": w, "height": h, "rgba": rgba, "axLabels": labels, "ocrBoxes": boxes,
            "masks": [], "uncertain": false, "secureInputGap": false
        }))
        .expect("frame");
        core.submit_frame(&id, dto).expect("submit");
    }
    core.stop_recording(&id).expect("stop");
    for sample_seq in [1, 2, 3] {
        core.remove_recording_frame(&id, sample_seq).expect("remove sample frame");
    }
    let preview = match core.preview_recording(&id).await {
        Ok(p) => p,
        Err(e) => return format!("  PREVIEW FAILED: {e}"),
    };
    let draft = match core.consent_and_analyze(&id, &preview.payload_hash).await {
        Ok(d) => d,
        Err(e) => return format!("  ANALYZE FAILED: {e}"),
    };
    let v = serde_json::to_value(&draft).unwrap();
    let mut out = String::new();
    out += &format!("  goal: {}\n  trigger: {}\n", v["goal"], v["trigger"]);
    for s in v["steps"].as_array().unwrap() {
        out += &format!("  step {} [{}] {}: {}\n", s["order"], s["capability"].as_str().unwrap_or(""), s["app"].as_str().unwrap_or(""), s["action"].as_str().unwrap_or(""));
    }
    out += &format!("  variables: {}\n", v["variables"].as_array().unwrap().iter().map(|x| format!("{}={}", x["name"].as_str().unwrap_or(""), x["description"].as_str().unwrap_or(""))).collect::<Vec<_>>().join(" | "));
    out += &format!("  constants: {}\n", v["constants"].as_array().unwrap().iter().map(|x| format!("{}={}", x["name"].as_str().unwrap_or(""), x["value"].as_str().unwrap_or(""))).collect::<Vec<_>>().join(" | "));
    out += &format!("  output: {}\n", v["output_description"]);
    for q in v["questions"].as_array().unwrap() {
        out += &format!("  question: {} (default {})\n", q["prompt"].as_str().unwrap_or(""), q["default"]);
    }
    let mut answers = BTreeMap::new();
    for q in draft.questions.iter() {
        answers.insert(q.id.clone(), q.default.clone());
    }
    let goal = match core.answer_recording_questions(&id, answers) {
        Ok(g) => g,
        Err(e) => return format!("{out}  ANSWERS FAILED: {e}"),
    };
    out += &format!("  clarified goal chars: {}\n", goal.chars().count());
    let task = match core.approve_recording_draft(&id) {
        Ok(t) => t,
        Err(e) => return format!("{out}  APPROVE FAILED: {e}"),
    };
    let task = match core.update_task(&task.draft_id, TaskKind::Program, "output.txt", None) {
        Ok(t) => t,
        Err(e) => return format!("{out}  DRAFT FAILED: {e}"),
    };
    if let Err(e) = core.outbound_preview(&task.draft_id).await {
        return format!("{out}  REVIEW FAILED: {e}");
    }
    let started = match core.start_build(&task.draft_id, true).await {
        Ok(s) => s,
        Err(e) => return format!("{out}  BUILD START FAILED: {e}"),
    };
    let mut approved = false;
    let mut state = None;
    for _ in 0..360 {
        let Ok(s) = core.build_state(&started.build_id) else { break };
        if s.phase == BuildPhase::Candidate && !approved {
            if let Some(program) = s.program.as_ref() {
                approved = core.approve_build_sources(&started.build_id, program).is_ok();
            }
        }
        if s.phase.is_terminal() {
            state = Some(s);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
    let Some(state) = state else { return format!("{out}  BUILD TIMED OUT") };
    if state.phase != BuildPhase::Ready {
        let why = state.failure.map(|f| f.message).unwrap_or_default();
        return format!("{out}  BUILD FAILED: {why}");
    }
    let hosts = state.program.as_ref().map(|p| p.web_sources.iter().map(|s| s.url.clone()).collect::<Vec<_>>().join(", ")).unwrap_or_default();
    out += &format!("  sources: {hosts}\n");
    if std::env::var("SCENARIO_DEBUG").is_ok() {
        if let Some(p) = state.program.as_ref() {
            out += &format!("  code: {}\n", p.code.chars().take(2500).collect::<String>());
        }
    }
    let tool = match core.save_tool(&state.build_id, "scenario").await {
        Ok(t) => t,
        Err(e) => return format!("{out}  SAVE FAILED: {e}"),
    };
    match core.preview_run_no_input(&tool.summary_row.id).await {
        Ok(StageResult::Staged(p)) => out += &format!("  RESULT: {}\n", p.preview_text.unwrap_or_default().trim()),
        Ok(StageResult::Failed(f)) => out += &format!("  RUN FAILED: {}\n", f.failure.message()),
        Err(e) => out += &format!("  RUN ERROR: {e}\n"),
    }
    out
}
