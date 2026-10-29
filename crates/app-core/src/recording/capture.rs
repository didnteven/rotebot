//! Native-only memory ingress. The webview has no command accepting raw frames.
use super::{RecordedFrameDto, RecordingStatus};
use crate::{lock, AppCore, CoreError, CoreResult, PolicyState};
use serde::Deserialize;
use serde_json::json;
use std::{
    io::{self, Read, Write},
    path::PathBuf,
    process::{Child, ChildStdin, Command, Stdio},
    sync::Arc,
    time::{Duration, Instant},
};

const HEADER_LIMIT: usize = 65_536;
const PAYLOAD_LIMIT: usize = 8 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Header {
    v: u32,
    #[serde(rename = "type")]
    kind: String,
    session_id: String,
    policy_epoch: u64,
    payload_bytes: usize,
    capture_supported: Option<bool>,
    screen_recording_granted: Option<bool>,
    accessibility_granted: Option<bool>,
    reason: Option<String>,
    app_name: Option<String>,
    seq: Option<u64>,
    mono_ms: Option<u64>,
    width: Option<u32>,
    height: Option<u32>,
    ax_labels: Option<Vec<String>>,
    ocr_boxes: Option<Vec<super::OcrBoxDto>>,
    masks: Option<Vec<super::IngressMask>>,
    uncertain: Option<bool>,
    secure_input_gap: Option<bool>,
}
struct Packet {
    header: Header,
    rgba: Vec<u8>,
}
enum CaptureAction {
    Continue,
    End,
    Submit(String, RecordedFrameDto),
}
fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "Invalid capture packet")
}
fn decode(reader: &mut impl Read) -> io::Result<Packet> {
    let mut length = [0; 4];
    reader.read_exact(&mut length)?;
    let length = u32::from_be_bytes(length) as usize;
    if length == 0 || length > HEADER_LIMIT {
        return Err(invalid());
    }
    let mut json = vec![0; length];
    reader.read_exact(&mut json)?;
    let header: Header = serde_json::from_slice(&json).map_err(|_| invalid())?;
    if header.v != 1 || header.payload_bytes > PAYLOAD_LIMIT - length {
        return Err(invalid());
    }
    if header.kind == "frame" {
        let (Some(width), Some(height)) = (header.width, header.height) else {
            return Err(invalid());
        };
        if width == 0
            || height == 0
            || width > 1600
            || height > 1600
            || u64::from(width) * u64::from(height) * 4 != header.payload_bytes as u64
            || header.seq.is_none()
            || header.mono_ms.is_none()
            || header.app_name.is_none()
            || header.ax_labels.is_none()
            || header.ocr_boxes.is_none()
            || header.masks.is_none()
            || header.uncertain.is_none()
            || header.secure_input_gap.is_none()
        {
            return Err(invalid());
        }
    } else if header.payload_bytes != 0
        || !matches!(header.kind.as_str(), "ready" | "picked" | "gap" | "stopped")
    {
        return Err(invalid());
    }
    let mut rgba = vec![0; header.payload_bytes];
    reader.read_exact(&mut rgba)?;
    Ok(Packet { header, rgba })
}
fn command(input: &mut ChildStdin, value: serde_json::Value) -> io::Result<()> {
    let bytes = serde_json::to_vec(&value).map_err(|_| invalid())?;
    input.write_all(&(bytes.len() as u32).to_be_bytes())?;
    input.write_all(&bytes)
}

struct Process {
    child: Child,
    stdin: ChildStdin,
}
impl Drop for Process {
    fn drop(&mut self) {
        // Closing admission happens under the manager lock before this kill/reap barrier.
        let _ = command(
            &mut self.stdin,
            json!({"v":1,"type":"stop","payloadBytes":0,"sessionId":"","policyEpoch":0}),
        );
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
pub(crate) struct CaptureHost {
    helper: Option<PathBuf>,
    process: Option<Process>,
    generation: u64,
    session: Option<String>,
    epoch: u64,
    started: Instant,
    probed: Option<Instant>,
    requesting: bool,
    ready_count: u32,
    status: RecordingStatus,
}
impl Default for CaptureHost {
    fn default() -> Self {
        Self {
            helper: None,
            process: None,
            generation: 0,
            session: None,
            epoch: 0,
            started: Instant::now(),
            probed: None,
            requesting: false,
            ready_count: 0,
            status: RecordingStatus {
                capture_available: false,
                helper_available: false,
                screen_recording_granted: false,
                accessibility_granted: false,
                picker_state: "unavailable".into(),
                app_name: None,
                reason: None,
            },
        }
    }
}
impl CaptureHost {
    fn stop(&mut self, reason: &str) -> Option<String> {
        self.generation += 1;
        let session = self.session.take();
        self.process.take(); // Drop kills and reaps before callers may review.
        self.requesting = false;
        if session.is_some() {
            self.status.picker_state = "stopped".into();
            self.status.reason = Some(reason.into());
        }
        session
    }
}
impl Drop for CaptureHost {
    fn drop(&mut self) {
        self.stop("quit");
    }
}

impl AppCore {
    /// Trusted host configuration; never renderer-controlled. No permission prompt or capture.
    pub fn configure_capture_helper(&self, helper: Option<PathBuf>) {
        self.stop_native_capture(None, "reconfigured");
        let mut host = lock(&self.inner.capture_host);
        let generation = host.generation;
        *host = CaptureHost::default();
        host.generation = generation;
        host.helper = helper.filter(|p| p.is_file());
        host.status.helper_available = host.helper.is_some() && cfg!(target_os = "macos");
        host.status.picker_state = if host.status.helper_available {
            "idle"
        } else {
            "unavailable"
        }
        .into();
        host.probed = None;
    }
    pub(crate) fn native_recording_status(&self) -> RecordingStatus {
        let mut host = lock(&self.inner.capture_host);
        if host.status.helper_available
            && host.process.is_none()
            && host
                .probed
                .is_none_or(|p| p.elapsed() >= Duration::from_secs(2))
        {
            let _ = self.spawn_capture(&mut host, None, false);
        }
        host.status.clone()
    }
    pub fn request_recording_permission(&self) -> CoreResult<RecordingStatus> {
        let _admission = self.admit_local()?;
        let mut host = lock(&self.inner.capture_host);
        if host.session.is_some() {
            return Err(CoreError::Invalid("Stop the recording first.".into()));
        }
        host.stop("permission_request");
        self.spawn_capture(&mut host, None, true)?;
        Ok(host.status.clone())
    }
    pub(crate) fn start_native_capture(&self, id: &str) -> CoreResult<()> {
        let mut host = lock(&self.inner.capture_host);
        if self.inner.policy.state() != PolicyState::CloudAllowed {
            return Err(CoreError::Invalid(
                "Turn off Local only before starting window recording.".into(),
            ));
        }
        if host.session.is_some() {
            return Err(CoreError::Invalid(
                "Stop the current recording first.".into(),
            ));
        }
        if !host.status.capture_available {
            return Err(CoreError::Invalid("Screen recording isn't set up on this Mac yet. Grant Screen Recording and Accessibility access, then press Try again.".into()));
        }
        host.stop("restart");
        self.spawn_capture(&mut host, Some(id.to_owned()), false)
    }
    pub(crate) fn stop_native_capture(&self, id: Option<&str>, reason: &str) {
        let mut host = lock(&self.inner.capture_host);
        if id.is_some() && host.session.as_deref() != id {
            return;
        }
        if let Some(id) = host.stop(reason) {
            self.mark_capture_stopped(&id);
        }
    }
    fn mark_capture_stopped(&self, id: &str) {
        // Lock ordering is always host -> recordings. No native result can cross this barrier.
        if let Some(slot) = lock(&self.inner.recordings).get_mut(id) {
            slot.frames.stopped = true;
        }
    }
    fn spawn_capture(
        &self,
        host: &mut CaptureHost,
        session: Option<String>,
        requesting: bool,
    ) -> CoreResult<()> {
        let path = host
            .helper
            .as_ref()
            .filter(|p| p.is_file())
            .ok_or_else(|| {
                CoreError::Invalid(
                    "This build doesn't include the capture helper. Try the sample recording."
                        .into(),
                )
            })?;
        let mut child = Command::new(path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| CoreError::Invalid("The capture helper couldn't start.".into()))?;
        let mut input = child.stdin.take().expect("piped stdin");
        let mut output = child.stdout.take().expect("piped stdout");
        let epoch = self.inner.policy.epoch();
        let control = if let Some(id) = &session {
            Some(json!({"v":1,"type":"start","payloadBytes":0,"sessionId":id,"policyEpoch":epoch}))
        } else if requesting {
            Some(
                json!({"v":1,"type":"request_permission","payloadBytes":0,"sessionId":"","policyEpoch":0}),
            )
        } else {
            None
        };
        if let Some(control) = control {
            if command(&mut input, control).is_err() {
                let _ = child.kill();
                let _ = child.wait();
                return Err(CoreError::Invalid(
                    "The capture helper disconnected.".into(),
                ));
            }
        }
        host.generation += 1;
        let generation = host.generation;
        host.session = session;
        host.epoch = epoch;
        host.started = Instant::now();
        host.probed = Some(Instant::now());
        host.requesting = requesting;
        host.ready_count = 0;
        if host.session.is_some() || host.requesting || host.status.picker_state != "stopped" {
            host.status.app_name = None;
            host.status.reason = None;
            host.status.picker_state = if host.session.is_some() {
                "picking"
            } else {
                "checking_permissions"
            }
            .into();
        }
        host.process = Some(Process {
            child,
            stdin: input,
        });
        let weak = Arc::downgrade(&self.inner);
        std::thread::spawn(move || loop {
            let packet = decode(&mut output);
            let Some(inner) = weak.upgrade() else { break };
            let core = AppCore { inner };
            let action = {
                let mut host = lock(&core.inner.capture_host);
                if host.generation != generation {
                    break;
                }
                if core.ensure_active().is_err() {
                    if let Some(id) = host.stop("discarded") {
                        core.mark_capture_stopped(&id);
                    }
                    break;
                }
                let Ok(packet) = packet else {
                    if host.ready_count == 0 {
                        host.status.capture_available = false;
                        host.status.helper_available = false;
                        host.status.picker_state = "unavailable".into();
                    }
                    if let Some(id) = host.stop("helper_disconnected") {
                        core.mark_capture_stopped(&id);
                    }
                    break;
                };
                core.handle_capture_packet(&mut host, packet)
            };
            match action {
                CaptureAction::Continue => {}
                CaptureAction::End => break,
                CaptureAction::Submit(id, dto) => {
                    // Never hold the host lock while acquiring lifecycle admission.
                    // Stop marks the slot closed under the recording lock: it either
                    // waits for this masking call, or this call rejects stopped ingress.
                    let result = core.submit_frame(&id, dto);
                    let mut host = lock(&core.inner.capture_host);
                    if host.generation != generation {
                        break;
                    }
                    if result.is_err() || result.is_ok_and(|f| f.stopped || f.frames.len() >= 60) {
                        host.stop("recording_limit");
                        core.mark_capture_stopped(&id);
                        break;
                    }
                }
            }
        });
        let weak = Arc::downgrade(&self.inner);
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_millis(50));
            let Some(inner) = weak.upgrade() else { break };
            let core = AppCore { inner };
            let mut host = lock(&core.inner.capture_host);
            if host.generation != generation || host.process.is_none() {
                break;
            }
            let changed = host.session.is_some()
                && (host.epoch != core.inner.policy.epoch()
                    || core.inner.policy.state() != PolicyState::CloudAllowed);
            let expired = host.started.elapsed()
                >= Duration::from_secs(if host.session.is_some() {
                    600
                } else if host.requesting {
                    120
                } else {
                    5
                });
            let exited = host
                .process
                .as_mut()
                .is_some_and(|p| !matches!(p.child.try_wait(), Ok(None)));
            if changed || expired || exited {
                let reason = if changed {
                    "policy_changed"
                } else if expired {
                    "max_duration"
                } else {
                    "helper_disconnected"
                };
                if let Some(id) = host.stop(reason) {
                    core.mark_capture_stopped(&id);
                }
                break;
            }
        });
        Ok(())
    }
    fn handle_capture_packet(&self, host: &mut CaptureHost, packet: Packet) -> CaptureAction {
        let Packet { header: h, rgba } = packet;
        if h.kind == "ready" {
            host.ready_count += 1;
            host.status.screen_recording_granted = h.screen_recording_granted == Some(true);
            host.status.accessibility_granted = h.accessibility_granted == Some(true);
            host.status.capture_available = host.status.helper_available
                && h.capture_supported == Some(true)
                && host.status.screen_recording_granted
                && host.status.accessibility_granted;
            if host.session.is_none() && (!host.requesting || host.ready_count >= 2) {
                if h.capture_supported != Some(true) {
                    host.status.reason = Some("picker_identity_unavailable".into());
                }
                if host.status.picker_state != "stopped" {
                    host.status.picker_state = if h.capture_supported != Some(true) {
                        "unavailable"
                    } else if host.status.capture_available {
                        "idle"
                    } else {
                        "permission_required"
                    }
                    .into();
                }
                host.stop("probe_complete");
                return CaptureAction::End;
            }
            return CaptureAction::Continue;
        }
        let Some(id) = host.session.clone() else {
            host.stop("protocol_error");
            return CaptureAction::End;
        };
        if h.session_id != id
            || h.policy_epoch != host.epoch
            || host.epoch != self.inner.policy.epoch()
            || self.inner.policy.state() != PolicyState::CloudAllowed
        {
            host.stop("policy_changed");
            self.mark_capture_stopped(&id);
            return CaptureAction::End;
        }
        match h.kind.as_str() {
            "picked" => {
                if host.status.picker_state != "picking" {
                    host.stop("selection_changed");
                    self.mark_capture_stopped(&id);
                    return CaptureAction::End;
                }
                host.status.picker_state = "recording".into();
                host.status.app_name = h.app_name.map(|s| s.chars().take(256).collect());
            }
            "frame" => {
                if host.status.picker_state != "recording" {
                    host.stop("protocol_error");
                    self.mark_capture_stopped(&id);
                    return CaptureAction::End;
                }
                let dto = RecordedFrameDto {
                    seq: h.seq.unwrap(),
                    mono_ms: h.mono_ms.unwrap(),
                    app_name: h.app_name.unwrap(),
                    window_title: String::new(),
                    width: h.width.unwrap(),
                    height: h.height.unwrap(),
                    rgba,
                    ax_labels: h.ax_labels.unwrap(),
                    ocr_boxes: h.ocr_boxes.unwrap(),
                    masks: h.masks.unwrap(),
                    uncertain: h.uncertain.unwrap(),
                    secure_input_gap: h.secure_input_gap.unwrap(),
                };
                return CaptureAction::Submit(id, dto);
            }
            "gap" => {
                host.status.reason = Some(safe_reason(h.reason.as_deref()).into());
            }
            "stopped" => {
                host.stop(safe_reason(h.reason.as_deref()));
                self.mark_capture_stopped(&id);
                return CaptureAction::End;
            }
            _ => {
                host.stop("protocol_error");
                self.mark_capture_stopped(&id);
                return CaptureAction::End;
            }
        }
        CaptureAction::Continue
    }
}
fn safe_reason(reason: Option<&str>) -> &'static str {
    match reason {
        Some("uncertain_ax_untrusted") => "uncertain_ax_untrusted",
        Some("uncertain_stale_frame") => "uncertain_stale_frame",
        Some("uncertain_window_changed") => "uncertain_window_changed",
        Some("uncertain_too_slow") => "uncertain_too_slow",
        Some("uncertain_text_scan_failed") => "uncertain_text_scan_failed",
        Some("uncertain_window_id") => "uncertain_window_id",
        Some("uncertain_window_bounds") => "uncertain_window_bounds",
        Some("uncertain_bounds_differ") => "uncertain_bounds_differ",
        Some("uncertain_a1") => "uncertain_a1",
        Some("uncertain_a2") => "uncertain_a2",
        Some("uncertain_a3") => "uncertain_a3",
        Some("uncertain_a4") => "uncertain_a4",
        Some("uncertain_a5") => "uncertain_a5",
        Some("uncertain_a6") => "uncertain_a6",
        Some("uncertain_a7") => "uncertain_a7",
        Some("uncertain_a8") => "uncertain_a8",
        Some("uncertain_a9") => "uncertain_a9",
        Some("uncertain_a10") => "uncertain_a10",
        Some("uncertain_a11") => "uncertain_a11",
        Some("uncertain_a12") => "uncertain_a12",
        Some("uncertain_a13") => "uncertain_a13",
        Some("private_or_unknown_window") => "private_or_unknown_window",
        Some("excluded") => "excluded",
        Some("picker_cancelled") => "picker_cancelled",
        Some("picker_identity_unavailable") => "picker_identity_unavailable",
        Some("screen_recording_denied") => "screen_recording_denied",
        Some("secure_input_gap") => "secure_input_gap",
        Some("session_inactive") => "session_inactive",
        Some("user") => "user",
        Some("paused") => "paused",
        Some("eof") => "eof",
        Some("max_duration") => "max_duration",
        Some("frame_limit") => "frame_limit",
        _ => "capture_stopped",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn packet(mut h: serde_json::Value, raw: &[u8]) -> Vec<u8> {
        h["payloadBytes"] = json!(raw.len());
        let json = serde_json::to_vec(&h).unwrap();
        let mut bytes = (json.len() as u32).to_be_bytes().to_vec();
        bytes.extend(json);
        bytes.extend(raw);
        bytes
    }
    fn frame() -> serde_json::Value {
        json!({"v":1,"type":"frame","sessionId":"x","policyEpoch":1,"seq":1,"monoMs":1,
            "appName":"TextEdit","width":1,"height":1,"axLabels":[],"ocrBoxes":[],"masks":[],"uncertain":false,"secureInputGap":false})
    }
    #[test]
    fn decode_binary_frame_and_truncated_input() {
        let bytes = packet(frame(), &[1, 2, 3, 255]);
        let p = decode(&mut &bytes[..]).unwrap();
        assert_eq!(p.rgba, [1, 2, 3, 255]);
        for n in 0..bytes.len() {
            assert!(decode(&mut &bytes[..n]).is_err());
        }
    }
    #[test]
    fn reject_size_version_title_and_dimension_mismatch_before_payload() {
        assert!(decode(&mut &(65_537u32.to_be_bytes())[..]).is_err());
        for (key, value) in [
            ("v", json!(2)),
            ("width", json!(1601)),
            ("height", json!(0)),
            ("windowTitle", json!("forbidden")),
        ] {
            let mut h = frame();
            h[key] = value;
            assert!(decode(&mut &packet(h, &[0; 4])[..]).is_err());
        }
        let h = json!({"v":1,"type":"frame","sessionId":"x","policyEpoch":1,"payloadBytes":8*1024*1024});
        let bytes = serde_json::to_vec(&h).unwrap();
        let mut wire = (bytes.len() as u32).to_be_bytes().to_vec();
        wire.extend(bytes);
        assert!(decode(&mut &wire[..]).is_err());
        assert!(decode(&mut &packet(frame(), &[0; 3])[..]).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn process_drop_kills_and_reaps() {
        let mut child = Command::new("/bin/cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let id = child.id();
        drop(Process { child, stdin });
        let result = Command::new("/bin/kill")
            .args(["-0", &id.to_string()])
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert!(!result.success());
    }
}
