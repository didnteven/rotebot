use super::*;
use crate::{CoreConfig, MuseProbe};
use macroscope_muse_client::PlanStatus;
use macroscope_vault::MemoryStore;
use std::sync::{
    atomic::{AtomicI64, Ordering},
    Arc,
};

struct NoMuse;
impl MuseProbe for NoMuse {
    fn version(&self) -> Option<String> {
        None
    }
    fn plan_status(&self) -> crate::BoxFuture<'_, PlanStatus> {
        Box::pin(async { PlanStatus::MuseMissing })
    }
}
fn core(base: String) -> (AppCore, tempfile::TempDir, Arc<AtomicI64>) {
    let dir = tempfile::tempdir().unwrap();
    let now = Arc::new(AtomicI64::new(1_800_000_000_000));
    let clock = now.clone();
    let core = AppCore::new(CoreConfig {
        data_dir: dir.path().join("data"),
        home: dir.path().join("home"),
        vault: Arc::new(MemoryStore::default()),
        muse: Arc::new(NoMuse),
        gateway_base_url: base,
        app_version: "test".into(),
        platform: "test".into(),
        now_ms: Arc::new(move || clock.load(Ordering::SeqCst)),
    })
    .unwrap();
    (core, dir, now)
}
async fn preview(core: &AppCore) -> (String, RecordingPreview) {
    let id = core.start_recording(RecordingSource::Sample).unwrap();
    core.stop_recording(&id).unwrap();
    let p = core.preview_recording(&id).await.unwrap();
    (id, p)
}
#[tokio::test]
async fn wrong_hash_local_only_route_change_and_replay_are_refused() {
    let (c, _, _) = core("http://127.0.0.1:9".into());
    c.set_local_only(false).await.unwrap();
    let (id, p) = preview(&c).await;
    assert!(c
        .consent_and_analyze(&id, "wrong")
        .await
        .err()
        .unwrap()
        .to_string()
        .contains("consent"));
    assert!(c.list_outbound().unwrap().is_empty());
    c.set_local_only(true).await.unwrap();
    assert!(c
        .consent_and_analyze(&id, &p.payload_hash)
        .await
        .err()
        .unwrap()
        .to_string()
        .contains("Local only"));
    c.set_local_only(false).await.unwrap();
    assert!(c.consent_and_analyze(&id, &p.payload_hash).await.is_err()); // epoch changed
    c.discard_recording(&id).unwrap();
    let (id, p) = preview(&c).await;
    c.consent_and_analyze(&id, &p.payload_hash).await.unwrap();
    assert!(c.consent_and_analyze(&id, &p.payload_hash).await.is_err());
}
#[tokio::test]
async fn questions_are_required_and_approved_goal_uses_existing_draft_path_without_frames() {
    let (c, _dir, _) = core("http://127.0.0.1:9".into());
    c.set_local_only(false).await.unwrap();
    let (id, p) = preview(&c).await;
    let draft = c.consent_and_analyze(&id, &p.payload_hash).await.unwrap();
    assert!(c.approve_recording_draft(&id).is_err());
    assert!(c.answer_recording_questions(&id, BTreeMap::new()).is_err());
    let answers = draft
        .questions
        .iter()
        .map(|q| (q.id.clone(), q.default.clone()))
        .collect();
    let goal = c.answer_recording_questions(&id, answers).unwrap();
    assert!(goal.contains("Manual only (unsupported)"));
    assert!(goal.contains("Changes per run"));
    {
        let slots = lock(&c.inner.recordings);
        let s = &slots[&id];
        assert!(s.capture.is_none());
        assert!(s.review.is_none());
        assert!(s.frames.frames.is_empty());
        let json = serde_json::to_string(s.draft.as_ref().unwrap()).unwrap();
        assert!(!json.contains("png"));
        assert!(!json.contains("Weekly order summary"));
    }
    let normal = c.approve_recording_draft(&id).unwrap();
    assert_eq!(normal.goal, goal);
    assert!(normal.no_input);
    assert_eq!(normal.kind, TaskKind::Program);
    assert_eq!(c.load_draft(&normal.draft_id).unwrap().goal, goal);
    assert!(c.list_recording_frames(&id).is_err());
}
#[tokio::test]
async fn idle_expiry_discards_frames_and_previews_and_access_refreshes_idle_clock() {
    let (c, _dir, now) = core("http://127.0.0.1:9".into());
    let (id, _) = preview(&c).await;
    now.fetch_add(IDLE_MS - 1, Ordering::SeqCst);
    assert!(c.list_recording_frames(&id).is_ok());
    now.fetch_add(IDLE_MS, Ordering::SeqCst);
    assert!(c.list_recording_frames(&id).is_err());
    assert!(lock(&c.inner.recordings).is_empty());
}
#[test]
fn caps_frames_bytes_sessions_and_pixels_and_stopped_ingress() {
    let (c, _dir, _) = core("http://127.0.0.1:9".into());
    assert!(!c.recording_status().capture_available);
    assert!(c.start_recording(RecordingSource::Window).is_err());
    let id = c.start_recording(RecordingSource::Sample).unwrap();
    for seq in 4..=60 {
        c.submit_frame(&id, sample_frame(seq)).unwrap();
    }
    assert!(c.submit_frame(&id, sample_frame(61)).is_err());
    assert_eq!(c.list_recording_frames(&id).unwrap().frames.len(), 60);
    c.discard_recording(&id).unwrap();
    let id = c.start_recording(RecordingSource::Sample).unwrap();
    lock(&c.inner.recordings).get_mut(&id).unwrap().raw_bytes = MAX_BYTES - 1;
    assert!(c.submit_frame(&id, sample_frame(4)).is_err());
    c.discard_recording(&id).unwrap();
    let id = c.start_recording(RecordingSource::Sample).unwrap();
    let mut frame = sample_frame(4);
    frame.rgba.push(0);
    assert!(c.submit_frame(&id, frame).is_err());
    c.stop_recording(&id).unwrap();
    assert!(c.submit_frame(&id, sample_frame(4)).is_err());
    for _ in 0..3 {
        c.start_recording(RecordingSource::Sample).unwrap();
    }
    assert!(c.start_recording(RecordingSource::Sample).is_err());
    c.discard_all_recordings();
    assert!(lock(&c.inner.recordings).is_empty());
}
#[test]
fn dto_emits_only_masked_pixels_and_drop_reasons() {
    let (c, _dir, _) = core("http://127.0.0.1:9".into());
    let id = c.start_recording(RecordingSource::Sample).unwrap();
    let mut f = sample_frame(4);
    f.uncertain = true;
    c.submit_frame(&id, f).unwrap();
    let dto = c.list_recording_frames(&id).unwrap();
    assert_eq!(dto.dropped[0].reason, DropReason::Uncertain);
    let json = serde_json::to_string(&dto).unwrap();
    assert!(!json.contains("windowTitle"));
    assert!(!json.contains("rgba"));
    let slots = lock(&c.inner.recordings);
    let png = slots[&id].capture.as_ref().unwrap().frames()[0].png();
    let mut decoder = png::Decoder::new(std::io::Cursor::new(png))
        .read_info()
        .unwrap();
    let mut pixels = vec![0; decoder.output_buffer_size().unwrap()];
    decoder.next_frame(&mut pixels).unwrap();
    assert_eq!(
        &pixels[(65 * 320 + 20) * 4..(65 * 320 + 20) * 4 + 4],
        &[0, 0, 0, 255]
    );
    assert_eq!(base64_impl::encode(b"f"), "Zg==");
    assert_eq!(base64_impl::encode(b"fo"), "Zm8=");
    assert_eq!(base64_impl::encode(b"foo"), "Zm9v");
}
#[tokio::test]
async fn gateway_sends_exact_reviewed_images_roles_and_schema_and_logs_exact_text_only() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};
    let server = MockServer::start().await;
    Mock::given(method("POST")).and(path("/responses")).respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
        "status":"completed","output":[{"content":[{"type":"output_text","text":sample_draft_json()}]}]
    }))).mount(&server).await;
    let (c, _dir, _) = core(server.uri());
    c.save_api_key("recording-test-key").await.unwrap();
    let (id, p) = preview(&c).await;
    c.consent_and_analyze(&id, &p.payload_hash).await.unwrap();
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(body["input"][0]["content"], p.system_instructions);
    assert_eq!(body["input"][1]["content"][0]["text"], p.cleared_text_json);
    assert_eq!(
        body["input"][1]["content"][1]["image_url"],
        format!("data:image/png;base64,{}", p.frames[0].png_base64)
    );
    assert_eq!(
        body["text"]["format"]["schema"],
        serde_json::from_str::<serde_json::Value>(&p.schema_json).unwrap()
    );
    assert_eq!(body["max_output_tokens"], 8000);
    assert!(body.get("tools").is_none());
    let rows = c.list_outbound().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].purpose, "Recording analysis");
    assert_eq!(rows[0].destination, p.recipient);
    assert_eq!(
        rows[0].payload_text.as_deref(),
        Some(p.payload_text.as_str())
    );
    assert!(!rows[0]
        .payload_text
        .as_ref()
        .unwrap()
        .contains(&p.frames[0].png_base64));
}

#[tokio::test]
async fn outbound_attempt_logs_exact_reviewed_text_even_when_transport_cannot_start() {
    // Invalid URL: exercises logging and the real adapter without opening any socket.
    let (c, _dir, _) = core("not a URL".into());
    c.save_api_key("synthetic-recording-test-key")
        .await
        .unwrap();
    let (id, p) = preview(&c).await;
    assert!(c.list_outbound().unwrap().is_empty());
    assert!(c.consent_and_analyze(&id, &p.payload_hash).await.is_err());
    let rows = c.list_outbound().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].purpose, "Recording analysis");
    assert_eq!(rows[0].destination, p.recipient);
    assert_eq!(
        rows[0].payload_text.as_deref(),
        Some(p.payload_text.as_str())
    );
    assert!(!rows[0]
        .payload_text
        .as_ref()
        .unwrap()
        .contains(&p.frames[0].png_base64));
    let slots = lock(&c.inner.recordings);
    assert!(slots[&id].capture.is_none());
    assert!(slots[&id].review.is_none());
    assert!(slots[&id].frames.frames.is_empty());
}

#[tokio::test]
async fn external_ingress_cannot_claim_builtin_sample_authority() {
    let (c, _dir, _) = core("not a URL".into());
    c.set_local_only(false).await.unwrap();
    let id = c.start_recording(RecordingSource::Sample).unwrap();
    c.submit_frame(&id, sample_frame(4)).unwrap();
    c.stop_recording(&id).unwrap();
    let p = c.preview_recording(&id).await.unwrap();
    assert!(!p.sample);
    assert!(c.consent_and_analyze(&id, &p.payload_hash).await.is_err());
    assert!(c.list_outbound().unwrap().is_empty());
}

#[test]
fn rgba_decoder_rejects_oversize_before_allocating_the_declared_length() {
    let input = serde::de::value::SeqDeserializer::<_, serde::de::value::Error>::new(
        std::iter::repeat(0u8).take(MAX_RGBA + 1),
    );
    assert!(bounded_rgba(input).is_err());
}
