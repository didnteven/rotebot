//! Memory-only session lifecycle and the desktop review boundary.
use super::*;
use crate::{dto::*, lock, AppCore, CoreError, CoreResult, PolicyState};
use base64_impl::encode;
use serde::de::{self, SeqAccess, Visitor};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap},
    fmt,
    time::Duration,
};

const IDLE_MS: i64 = 30 * 60 * 1000;
const MAX_FRAMES: usize = 60;
const MAX_BYTES: usize = 64 * 1024 * 1024;
const MAX_RGBA: usize = 1600 * 1600 * 4;

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordingSource {
    Sample,
    Window,
}

/// Raw ingress has no Serialize or Debug implementation. Bound allocation while decoding.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecordedFrameDto {
    pub seq: u64,
    pub mono_ms: u64,
    pub app_name: String,
    pub window_title: String,
    pub width: u32,
    pub height: u32,
    #[serde(deserialize_with = "bounded_rgba")]
    pub rgba: Vec<u8>,
    pub ax_labels: Vec<String>,
    pub ocr_boxes: Vec<OcrBoxDto>,
    pub masks: Vec<IngressMask>,
    pub uncertain: bool,
    pub secure_input_gap: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OcrBoxDto {
    pub text: String,
    pub rect: Rect,
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IngressMaskReason {
    SecureField,
    Manual,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IngressMask {
    pub rect: Rect,
    pub reason: IngressMaskReason,
}
fn bounded_rgba<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
    struct Bytes;
    impl<'de> Visitor<'de> for Bytes {
        type Value = Vec<u8>;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "at most {MAX_RGBA} RGBA bytes")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Vec<u8>, A::Error> {
            if seq.size_hint().is_some_and(|n| n > MAX_RGBA) {
                return Err(de::Error::custom("frame too large"));
            }
            let mut bytes = Vec::with_capacity(seq.size_hint().unwrap_or(0).min(MAX_RGBA));
            while let Some(b) = seq.next_element::<u8>()? {
                if bytes.len() == MAX_RGBA {
                    return Err(de::Error::custom("frame too large"));
                }
                bytes.push(b);
            }
            Ok(bytes)
        }
    }
    d.deserialize_seq(Bytes)
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MaskedFrameDto {
    pub seq: u64,
    pub png_base64: String,
    pub width: u32,
    pub height: u32,
    pub masks: Vec<MaskRegion>,
    pub unverified: bool,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingFrames {
    pub frames: Vec<MaskedFrameDto>,
    pub dropped: Vec<DroppedFrame>,
    pub stopped: bool,
    pub mask_level: MaskLevel,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingStatus {
    pub capture_available: bool,
    pub helper_available: bool,
    pub screen_recording_granted: bool,
    pub accessibility_granted: bool,
    pub picker_state: String,
    pub app_name: Option<String>,
    pub reason: Option<String>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingPreview {
    pub session_id: String,
    pub frames: Vec<MaskedFrameDto>,
    pub dropped: Vec<DroppedFrame>,
    pub cleared_text_json: String,
    pub system_instructions: String,
    pub schema_json: String,
    /// Exact textual fields sent; image bytes are displayed separately and never logged to disk.
    pub payload_text: String,
    pub total_bytes: usize,
    pub recipient: String,
    pub model: String,
    pub cost: CostEstimate,
    pub payload_hash: String,
    pub sample: bool,
}
struct Review {
    payload: RecordingPayload,
    dto: RecordingPreview,
    identity: String,
}
pub(crate) struct RecordingSlot {
    touched: i64,
    capture: Option<RecordingSession>,
    pub(super) frames: RecordingFrames,
    review: Option<Review>,
    draft: Option<TaskDraftV2>,
    clarified: Option<ClarifiedTask>,
    raw_bytes: usize,
    attempts: usize,
    native_epoch: Option<u64>,
    sample: bool,
    analyzing: bool,
    cancel: tokio::sync::watch::Sender<bool>,
}
struct DispatchGuard(Option<macroscope_policy::EpochGuard>);
impl Drop for DispatchGuard {
    fn drop(&mut self) {
        if let Some(guard) = self.0.take() {
            guard.acknowledge(true);
        }
    }
}
impl Drop for RecordingSlot {
    fn drop(&mut self) {
        let _ = self.cancel.send(true);
    }
}
pub(crate) type RecordingSlots = HashMap<String, RecordingSlot>;
fn invalid(s: impl Into<String>) -> CoreError {
    CoreError::Invalid(s.into())
}
fn err(e: impl fmt::Display) -> CoreError {
    invalid(e.to_string())
}
fn rates() -> EstimateRates {
    EstimateRates {
        input_micros_per_million_tokens: 1_250_000,
        output_micros_per_million_tokens: 4_250_000,
        estimated_image_tokens_per_frame: 1600,
        max_output_tokens: 16000,
    }
}
fn frames(session: &RecordingSession, stopped: bool) -> RecordingFrames {
    RecordingFrames {
        frames: session
            .frames()
            .iter()
            .map(|f| MaskedFrameDto {
                unverified: f.unverified(),
                seq: f.text().seq,
                png_base64: encode(f.png()),
                width: f.dimensions().0,
                height: f.dimensions().1,
                masks: f.text().masks.clone(),
            })
            .collect(),
        dropped: session.dropped().to_vec(),
        stopped,
        mask_level: session.level(),
    }
}
fn slot<'a>(
    slots: &'a mut RecordingSlots,
    id: &str,
    now: i64,
) -> CoreResult<&'a mut RecordingSlot> {
    slots.retain(|_, s| now.saturating_sub(s.touched) < IDLE_MS);
    let s = slots.get_mut(id).ok_or_else(|| {
        invalid("This recording expired or was discarded. Start a new recording.")
    })?;
    s.touched = now;
    Ok(s)
}
impl AppCore {
    pub fn recording_status(&self) -> RecordingStatus {
        self.native_recording_status()
    }
    pub fn start_recording(&self, source: RecordingSource) -> CoreResult<String> {
        let _admission = self.admit_local()?;
        let now = self.now();
        let mut slots = lock(&self.inner.recordings);
        slots.retain(|_, s| now.saturating_sub(s.touched) < IDLE_MS);
        if slots.len() >= 4 {
            return Err(invalid(
                "Discard an earlier recording before starting another.",
            ));
        }
        let capture = RecordingSession::with_level(MaskLevel::Off);
        let id = capture.id.clone();
        slots.insert(
            id.clone(),
            RecordingSlot {
                touched: now,
                frames: frames(&capture, false),
                capture: Some(capture),
                review: None,
                draft: None,
                clarified: None,
                raw_bytes: 0,
                attempts: 0,
                native_epoch: matches!(source, RecordingSource::Window)
                    .then(|| self.inner.policy.epoch()),
                sample: matches!(source, RecordingSource::Sample),
                analyzing: false,
                cancel: tokio::sync::watch::channel(false).0,
            },
        );
        drop(slots);
        // Sync Tauri commands may have no current Tokio handle. One weak worker per
        // AppCore still evicts idle memory without another API call or owning the core.
        if !self
            .inner
            .recording_expiry_started
            .swap(true, std::sync::atomic::Ordering::SeqCst)
        {
            let weak = std::sync::Arc::downgrade(&self.inner);
            std::thread::spawn(move || loop {
                std::thread::sleep(Duration::from_secs(30));
                let Some(inner) = weak.upgrade() else { break };
                let now = (inner.cfg.now_ms)();
                lock(&inner.recordings).retain(|_, s| now.saturating_sub(s.touched) < IDLE_MS);
            });
        }
        match source {
            RecordingSource::Sample => {
                for seq in 1..=3 {
                    self.record_frame(&id, sample_frame(seq), true)?;
                }
            }
            RecordingSource::Window => {
                if let Err(error) = self.start_native_capture(&id) {
                    self.discard_recording(&id)?;
                    return Err(error);
                }
            }
        }
        Ok(id)
    }
    pub fn submit_frame(
        &self,
        session_id: &str,
        dto: RecordedFrameDto,
    ) -> CoreResult<RecordingFrames> {
        let _admission = self.admit_local()?;
        self.record_frame(session_id, dto, false)
    }
    fn record_frame(
        &self,
        session_id: &str,
        dto: RecordedFrameDto,
        synthetic: bool,
    ) -> CoreResult<RecordingFrames> {
        let mut slots = lock(&self.inner.recordings);
        let s = slot(&mut slots, session_id, self.now())?;
        if s.native_epoch.is_some_and(|epoch| {
            epoch != self.inner.policy.epoch()
                || self.inner.policy.state() != PolicyState::CloudAllowed
        }) {
            s.frames.stopped = true;
            return Err(invalid(
                "Your privacy settings changed. Start a new recording.",
            ));
        }
        if s.capture.is_none() || s.frames.stopped {
            return Err(invalid("This recording has stopped."));
        }
        let expected = (dto.width as usize)
            .checked_mul(dto.height as usize)
            .and_then(|n| n.checked_mul(4));
        if dto.width == 0
            || dto.height == 0
            || dto.width > 1600
            || dto.height > 1600
            || expected != Some(dto.rgba.len())
            || dto.rgba.len() > MAX_RGBA
        {
            return Err(invalid(
                "The frame has invalid dimensions or too many pixels.",
            ));
        }
        if s.attempts >= 150
            || s.frames.frames.len() >= MAX_FRAMES
            || dto.rgba.len() > MAX_BYTES.saturating_sub(s.raw_bytes)
        {
            s.frames.stopped = true;
            return Err(invalid(
                "The recording limit was reached. Stop and review this recording.",
            ));
        }
        // External ingress can never acquire the built-in sample's synthetic authority.
        s.sample &= synthetic;
        s.raw_bytes += dto.rgba.len();
        s.attempts += 1;
        let regions = dto
            .masks
            .into_iter()
            .map(|m| MaskRegion {
                rect: m.rect,
                reason: match m.reason {
                    IngressMaskReason::SecureField => MaskReason::SecureField,
                    IngressMaskReason::Manual => MaskReason::Manual,
                },
            })
            .collect::<Vec<_>>();
        let frame = RecordedFrame {
            seq: dto.seq,
            mono_ms: dto.mono_ms,
            app_name: dto.app_name,
            window_title: dto.window_title,
            width: dto.width,
            height: dto.height,
            rgba: dto.rgba,
            ax_labels: dto.ax_labels,
            ocr_boxes: dto
                .ocr_boxes
                .into_iter()
                .map(|b| OcrBox {
                    text: b.text,
                    rect: b.rect,
                })
                .collect(),
            uncertain: dto.uncertain,
            secure_input_gap: dto.secure_input_gap,
        };
        let capture = s.capture.as_mut().unwrap();
        let detector = TextDetector::with_level(capture.level()).map_err(err)?;
        let result = capture.record(frame, &regions, &detector);
        s.frames = frames(capture, matches!(result, Err(DropReason::SessionLimit)));
        Ok(s.frames.clone())
    }
    pub fn list_recording_frames(&self, session_id: &str) -> CoreResult<RecordingFrames> {
        Ok(
            slot(&mut lock(&self.inner.recordings), session_id, self.now())?
                .frames
                .clone(),
        )
    }
    pub fn stop_recording(&self, session_id: &str) -> CoreResult<RecordingFrames> {
        self.stop_native_capture(Some(session_id), "user");
        let mut slots = lock(&self.inner.recordings);
        let s = slot(&mut slots, session_id, self.now())?;
        s.frames.stopped = true;
        Ok(s.frames.clone())
    }
    /// Leaves one frame out of a stopped recording and clears the review so it is rebuilt without it.
    pub fn remove_recording_frame(
        &self,
        session_id: &str,
        seq: u64,
    ) -> CoreResult<RecordingFrames> {
        let mut slots = lock(&self.inner.recordings);
        let s = slot(&mut slots, session_id, self.now())?;
        if !s.frames.stopped {
            return Err(invalid("Stop the recording before removing frames."));
        }
        if s.analyzing || s.draft.is_some() {
            return Err(invalid("This recording has already been analyzed."));
        }
        let capture = s
            .capture
            .as_mut()
            .ok_or_else(|| invalid("This recording can no longer be changed."))?;
        if !capture.remove_frame(seq) {
            return Err(invalid("That frame is no longer in the recording."));
        }
        s.review = None;
        s.frames = frames(capture, true);
        Ok(s.frames.clone())
    }
    /// Hides more or less sensitive text in a stopped recording by masking its frames again.
    pub fn set_recording_mask_level(
        &self,
        session_id: &str,
        level: MaskLevel,
    ) -> CoreResult<()> {
        let mut slots = lock(&self.inner.recordings);
        let s = slot(&mut slots, session_id, self.now())?;
        if !s.frames.stopped {
            return Err(invalid("Stop the recording before changing what is hidden."));
        }
        if s.analyzing || s.draft.is_some() {
            return Err(invalid("This recording has already been analyzed."));
        }
        let capture = s
            .capture
            .as_mut()
            .ok_or_else(|| invalid("This recording can no longer be changed."))?;
        let detector = TextDetector::with_level(level).map_err(err)?;
        capture.set_level(level, &detector);
        s.review = None;
        s.frames = frames(capture, true);
        Ok(())
    }
    /// The masked PNG exactly as it will be sent, for the person to save and inspect.
    pub fn recording_frame_png(&self, session_id: &str, seq: u64) -> CoreResult<Vec<u8>> {
        let mut slots = lock(&self.inner.recordings);
        let s = slot(&mut slots, session_id, self.now())?;
        let capture = s
            .capture
            .as_ref()
            .ok_or_else(|| invalid("This recording can no longer be changed."))?;
        capture
            .frames()
            .iter()
            .find(|f| f.text().seq == seq)
            .map(|f| f.png().to_vec())
            .ok_or_else(|| invalid("That frame is no longer in the recording."))
    }
    pub fn discard_recording(&self, session_id: &str) -> CoreResult<()> {
        self.stop_native_capture(Some(session_id), "discarded");
        lock(&self.inner.recordings).remove(session_id);
        Ok(())
    }
    pub fn discard_all_recordings(&self) {
        self.stop_native_capture(None, "discarded");
        lock(&self.inner.recordings).clear();
    }
    pub async fn preview_recording(&self, session_id: &str) -> CoreResult<RecordingPreview> {
        let access = self.model_access().await;
        let route = access.effective_route.unwrap_or(ModelRoute::OfflineDemo);
        let epoch = self.inner.policy.epoch();
        let identity = self.recording_identity(&access, epoch);
        let mut slots = lock(&self.inner.recordings);
        let s = slot(&mut slots, session_id, self.now())?;
        if !s.frames.stopped {
            return Err(invalid("Stop the recording before reviewing it."));
        }
        if s.analyzing || s.draft.is_some() {
            return Err(invalid("This recording has already been analyzed."));
        }
        if let Some(r) = &s.review {
            if r.identity != identity {
                return Err(invalid(
                    "Your model settings changed. Discard this recording and start again.",
                ));
            }
            return Ok(r.dto.clone());
        }
        if s.frames.frames.is_empty() {
            return Err(invalid("No frames were safe to keep. Try recording again."));
        }
        // Work from a copy so the person can still remove frames while reviewing.
        let payload = s
            .capture
            .clone()
            .ok_or_else(|| invalid("Start a new recording."))?
            .preview(rates())
            .map_err(err)?;
        let payload_text = serde_json::to_string_pretty(&serde_json::json!({
            "system": payload.system(), "response_schema_json": payload.response_schema_json(),
            "untrusted_data_json": payload.untrusted_data_json(),
        }))
        .map_err(err)?;
        let hash = hex::encode(Sha256::digest(format!("{}\n{}", payload.hash(), identity)));
        let mut cost = payload.cost().clone();
        if route != ModelRoute::ApiKey {
            cost.caveat.push_str(" Selected route pricing is unknown; this illustration uses Meta standard rates, not your provider's price.");
        }
        if s.sample && route == ModelRoute::OfflineDemo {
            cost.estimated_micros = 0;
            cost.caveat =
                "Sample walkthrough: analyzed on this Mac with no model call or charge.".into();
        }
        let dto = RecordingPreview {
            session_id: session_id.into(),
            frames: s.frames.frames.clone(),
            dropped: s.frames.dropped.clone(),
            cleared_text_json: payload.untrusted_data_json().into(),
            system_instructions: payload.system().into(),
            schema_json: payload.response_schema_json().into(),
            payload_text,
            total_bytes: payload.total_bytes(),
            recipient: if s.sample && route == ModelRoute::OfflineDemo {
                "Nothing leaves this Mac (sample)".into()
            } else {
                self.route_destination(route)
            },
            model: if s.sample && route == ModelRoute::OfflineDemo {
                "Sample walkthrough".into()
            } else {
                access.model.clone()
            },
            cost,
            payload_hash: hash,
            sample: s.sample,
        };
        s.review = Some(Review {
            payload,
            dto: dto.clone(),
            identity,
        });
        Ok(dto)
    }
    fn recording_identity(&self, access: &ModelAccessStatus, epoch: u64) -> String {
        serde_json::json!([
            access.selected_route,
            access.effective_route,
            access.model,
            access.egress,
            self.custom_endpoint_status().base_url,
            epoch
        ])
        .to_string()
    }
    pub async fn consent_and_analyze(
        &self,
        session_id: &str,
        payload_hash: &str,
    ) -> CoreResult<TaskDraftV2> {
        let access = self.model_access().await;
        if self
            .inner
            .deleting
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            return Err(invalid(
                "Your data is being deleted. Start again afterwards.",
            ));
        }
        if self.inner.policy.state() != PolicyState::CloudAllowed {
            return Err(invalid("Local only is on. Turn it off in Settings before sending a recording for analysis."));
        }
        let epoch = self.inner.policy.epoch();
        let identity = self.recording_identity(&access, epoch);
        let route = access.effective_route.unwrap_or(ModelRoute::OfflineDemo);
        let (request, dto, sample, mut cancelled, restore) = {
            let mut slots = lock(&self.inner.recordings);
            let s = slot(&mut slots, session_id, self.now())?;
            let r = s
                .review
                .as_ref()
                .ok_or_else(|| invalid("Review what will be sent first."))?;
            if r.dto.payload_hash != payload_hash || r.identity != identity {
                return Err(invalid(
                    "This consent doesn't match the reviewed recording and model. Review it again.",
                ));
            }
            if s.analyzing || s.draft.is_some() {
                return Err(invalid("This recording has already been sent."));
            }
            if !(s.sample && route == ModelRoute::OfflineDemo)
                && !matches!(route, ModelRoute::ApiKey | ModelRoute::CustomEndpoint)
            {
                return Err(invalid("Choose an API key or image-capable custom endpoint in Settings. Muse plan recording analysis isn't available yet."));
            }
            let r = s.review.take().unwrap();
            let inner_hash = r.payload.hash().to_string();
            let request = r
                .payload
                .consent(ExplicitConsent::Approved {
                    preview_hash: inner_hash,
                })
                .map_err(err)?
                .into_model_request()
                .map_err(err)?;
            // Sending closes the review: the stored frames are released now. They are only kept in
            // memory here so that a failed attempt can be reviewed and sent again.
            let restore = (s.capture.take(), s.frames.clone());
            s.analyzing = true;
            // No frame bytes survive in the stored session draft.
            s.frames.frames.clear();
            (request, r.dto, s.sample, s.cancel.subscribe(), restore)
        };
        let result = if sample && route == ModelRoute::OfflineDemo {
            request.parse_draft(&sample_draft_json()).map_err(err)
        } else {
            let admission = DispatchGuard(Some(
                self.inner
                    .policy
                    .admit(macroscope_policy::Job { epoch })
                    .map_err(err)?
                    .guard,
            ));
            let result = tokio::select! {
                biased;
                _ = cancelled.changed() => Err(invalid("Recording discarded. Nothing more will be sent.")),
                _ = self.inner.policy.revoked(epoch) => Err(invalid("Cloud analysis stopped. Local only is on.")),
                result = self.analyze_recording_packet(&request, &dto, route) => result,
            };
            drop(admission);
            result
        };
        let mut slots = lock(&self.inner.recordings);
        let s = slot(&mut slots, session_id, self.now())?;
        s.analyzing = false;
        let draft = match result {
            Ok(draft) => draft,
            Err(e) => {
                // Nothing was analysed: bring the recording back so the person can review and retry.
                if !*s.cancel.subscribe().borrow() {
                    s.capture = restore.0;
                    s.frames = restore.1;
                    s.review = None;
                }
                return Err(e);
            }
        };
        s.draft = Some(draft.clone());
        Ok(draft)
    }
    pub fn answer_recording_questions(
        &self,
        session_id: &str,
        answers: BTreeMap<String, String>,
    ) -> CoreResult<String> {
        let mut slots = lock(&self.inner.recordings);
        let s = slot(&mut slots, session_id, self.now())?;
        let draft = s
            .draft
            .as_ref()
            .ok_or_else(|| invalid("Analyze the recording first."))?;
        s.clarified = None;
        let clarified = apply_answers(draft, &answers).map_err(err)?;
        let goal = clarified.description.clone();
        s.clarified = Some(clarified);
        Ok(goal)
    }
    /// Applies the person's wording changes to the analyzed plan; answers must be given again.
    pub fn edit_recording_draft(
        &self,
        session_id: &str,
        edit: DraftEdit,
    ) -> CoreResult<TaskDraftV2> {
        let mut slots = lock(&self.inner.recordings);
        let s = slot(&mut slots, session_id, self.now())?;
        let draft = s
            .draft
            .as_ref()
            .ok_or_else(|| invalid("Analyze the recording first."))?;
        let edited = draft.edited(&edit).map_err(err)?;
        s.clarified = None;
        s.draft = Some(edited.clone());
        Ok(edited)
    }
    pub fn approve_recording_draft(&self, session_id: &str) -> CoreResult<TaskDraft> {
        let clarified = {
            let mut slots = lock(&self.inner.recordings);
            slot(&mut slots, session_id, self.now())?
                .clarified
                .clone()
                .ok_or_else(|| invalid("Answer every question before creating the tool."))?
        };
        // A format example read off a screen is a guess; the plan's output description carries the
        // format, so the builder is not held to it as a literal line.
        let draft = self.draft_task_no_input(&clarified.description)?;
        self.discard_recording(session_id)?;
        Ok(draft)
    }
    async fn analyze_recording_packet(
        &self,
        request: &ModelRequest,
        preview: &RecordingPreview,
        route: ModelRoute,
    ) -> CoreResult<TaskDraftV2> {
        transport::analyze(self, request, preview, route).await
    }
}

// Stage B's explicit sample only. Stage C must replace Window admission with native authority.
fn sample_frame(seq: u64) -> RecordedFrameDto {
    // Bundled, fabricated fixtures only; native/user pixels are never read from a file.
    let index = (seq.saturating_sub(1) % 3) as usize;
    let bytes: &[u8] = [
        include_bytes!("service/sample-orders.png").as_slice(),
        include_bytes!("service/sample-summary.png").as_slice(),
        include_bytes!("service/sample-message.png").as_slice(),
    ][index];
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes))
        .read_info()
        .expect("bundled sample PNG");
    let mut rgba = vec![
        0;
        decoder
            .output_buffer_size()
            .expect("bounded bundled sample")
    ];
    decoder
        .next_frame(&mut rgba)
        .expect("bundled sample pixels");
    RecordedFrameDto {
        seq,
        mono_ms: seq * 1000,
        app_name: ["Sample spreadsheet", "Sample editor", "Sample mail"][index].into(),
        window_title: "Synthetic sample".into(),
        width: 320,
        height: 160,
        rgba,
        ax_labels: vec![],
        ocr_boxes: vec![OcrBoxDto {
            text: [
                "Orders - sample",
                "Weekly order summary",
                "Draft team message",
            ][index]
                .into(),
            rect: Rect {
                x: 10,
                y: 10,
                width: 200,
                height: 20,
            },
        }],
        masks: vec![IngressMask {
            rect: Rect {
                x: 10,
                y: 60,
                width: 200,
                height: 30,
            },
            reason: IngressMaskReason::SecureField,
        }],
        uncertain: false,
        secure_input_gap: false,
    }
}
pub(super) fn sample_draft_json() -> String {
    serde_json::json!({"schema_version":2,"goal":"Create a weekly order summary", "steps":[
        {"id":"read","order":1,"app":"Spreadsheet","action":"Read the order list you select","capability":"file_read","evidence_frames":[1]},
        {"id":"save","order":2,"app":"Rotebot","action":"Create a new summary file","capability":"output_file","evidence_frames":[2]},
        {"id":"send","order":3,"app":"Mail","action":"Send the summary to your team","capability":"manual","evidence_frames":[3]}],
        "variables":[{"name":"Order list","description":"Choose the current week's file"}],"constants":[],
        "output_description":"One new weekly summary file", "trigger":"When you choose to run it", "unsupported_steps":[{"step_id":"send","reason":"Rotebot cannot control Mail or send messages"}],
        "ambiguities":[],"questions":[
            {"id":"format","prompt":"Which summary format?","evidence_frames":[1],"default":"text","options":[{"id":"text","label":"Plain text","any_is_fine":false},{"id":"any","label":"Any is fine","any_is_fine":true}]},
            {"id":"group","prompt":"How should orders be grouped?","evidence_frames":[2],"default":"date","options":[{"id":"date","label":"By date","any_is_fine":false},{"id":"any","label":"Any is fine","any_is_fine":true}]},
            {"id":"detail","prompt":"How much detail?","evidence_frames":[3],"default":"short","options":[{"id":"short","label":"A short overview","any_is_fine":false},{"id":"any","label":"Any is fine","any_is_fine":true}]}]}).to_string()
}

// Kept local to avoid changing dependency manifests outside the authorized paths.
mod base64_impl {
    pub fn encode(bytes: &[u8]) -> String {
        const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut result = String::with_capacity(bytes.len().div_ceil(3) * 4);
        for chunk in bytes.chunks(3) {
            let n = (u32::from(chunk[0]) << 16)
                | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
                | u32::from(*chunk.get(2).unwrap_or(&0));
            for i in 0..4 {
                result.push(if i > chunk.len() {
                    '='
                } else {
                    ALPHABET[((n >> (18 - 6 * i)) & 63) as usize] as char
                });
            }
        }
        result
    }
}
#[cfg(test)]
mod tests;
mod transport;
