//! Headless privacy gate, task drafting and memory-only recording service. Capture is external.
pub(crate) mod capture;
mod draft;
mod masking;
mod payload;
mod service;
pub use service::*;

pub use draft::*;
pub use masking::{DetectorError, SensitiveDetector, TextDetector};
pub use payload::*;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    fn fits(self, width: u32, height: u32) -> bool {
        self.width > 0
            && self.height > 0
            && self.x.checked_add(self.width).is_some_and(|x| x <= width)
            && self.y.checked_add(self.height).is_some_and(|y| y <= height)
    }

    fn intersects(self, other: Self) -> bool {
        u64::from(self.x) < u64::from(other.x) + u64::from(other.width)
            && u64::from(other.x) < u64::from(self.x) + u64::from(self.width)
            && u64::from(self.y) < u64::from(other.y) + u64::from(other.height)
            && u64::from(other.y) < u64::from(self.y) + u64::from(self.height)
    }
}

// Raw content deliberately has neither Serialize nor Debug. The gate consumes it.
pub struct OcrBox {
    pub text: String,
    pub rect: Rect,
}

pub struct RecordedFrame {
    pub seq: u64,
    pub mono_ms: u64,
    pub app_name: String,
    pub window_title: String,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub ax_labels: Vec<String>,
    pub ocr_boxes: Vec<OcrBox>,
    pub uncertain: bool,
    pub secure_input_gap: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MaskReason {
    SecureField,
    Manual,
    SensitiveText,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct MaskRegion {
    pub rect: Rect,
    pub reason: MaskReason,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClearedOcrBox {
    pub text: String,
    pub rect: Rect,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FrameText {
    pub seq: u64,
    pub mono_ms: u64,
    pub app_name: String,
    /// Always omitted: a title is sensitive context, not a safe label.
    pub window_title: Option<String>,
    pub ax_labels: Vec<String>,
    pub ocr_boxes: Vec<ClearedOcrBox>,
    pub masks: Vec<MaskRegion>,
}

pub struct MaskedFrame {
    text: FrameText,
    width: u32,
    height: u32,
    png: Vec<u8>,
    unverified: bool,
}

impl MaskedFrame {
    /// True when the capture could not fully check the window; shown to the person as a warning.
    pub fn unverified(&self) -> bool {
        self.unverified
    }

    pub fn text(&self) -> &FrameText {
        &self.text
    }

    pub fn png(&self) -> &[u8] {
        &self.png
    }

    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DropReason {
    Uncertain,
    SecureInputGap,
    InvalidPixels,
    InvalidGeometry,
    DetectorError,
    EncodingError,
    OutOfOrder,
    SessionLimit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct DroppedFrame {
    pub seq: u64,
    pub reason: DropReason,
}

pub struct RecordingSession {
    id: String,
    frames: Vec<MaskedFrame>,
    dropped: Vec<DroppedFrame>,
    last: Option<(u64, u64)>,
    attempts: usize,
    closed: bool,
    mask_history: masking::MaskHistory,
}

impl Default for RecordingSession {
    fn default() -> Self {
        Self::new()
    }
}

impl RecordingSession {
    pub fn new() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            frames: Vec::new(),
            dropped: Vec::new(),
            last: None,
            attempts: 0,
            closed: false,
            mask_history: masking::MaskHistory::default(),
        }
    }

    /// At most 150 candidate attempts and 300 seconds per session. Exhaustion closes admission.
    pub fn record(
        &mut self,
        frame: RecordedFrame,
        regions: &[MaskRegion],
        detector: &impl SensitiveDetector,
    ) -> Result<(), DropReason> {
        let seq = frame.seq;
        let result = if self.closed || self.attempts >= 150 || frame.mono_ms > 300_000 {
            self.closed = true;
            Err(DropReason::SessionLimit)
        } else if self
            .last
            .is_some_and(|(s, t)| seq <= s || frame.mono_ms < t)
        {
            Err(DropReason::OutOfOrder)
        } else {
            self.last = Some((seq, frame.mono_ms));
            masking::mask(frame, regions, detector, &mut self.mask_history)
        };
        self.attempts = self.attempts.saturating_add(1);
        match result {
            Ok(frame) => {
                self.frames.push(frame);
                Ok(())
            }
            Err(reason) => {
                if self.dropped.len() < 150 {
                    self.dropped.push(DroppedFrame { seq, reason });
                }
                Err(reason)
            }
        }
    }

    pub fn frames(&self) -> &[MaskedFrame] {
        &self.frames
    }

    /// The person decided to leave this frame out; it is removed before anything can be sent.
    pub fn remove_frame(&mut self, seq: u64) -> bool {
        let before = self.frames.len();
        self.frames.retain(|f| f.text().seq != seq);
        self.frames.len() != before
    }

    pub fn dropped(&self) -> &[DroppedFrame] {
        &self.dropped
    }
}

#[cfg(test)]
mod tests;
