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

/// How much sensitive text is hidden after a recording. Chosen by the person in the review step.
/// Password-field regions the system reports are always hidden, whatever the level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaskLevel {
    /// Nothing is hidden except password fields the system reports.
    #[default]
    Off,
    /// Only clear secrets: card numbers, keys and tokens, and values next to labels like "password" or "PIN".
    Low,
    /// Also emails, bank and tax numbers, phone numbers and long random strings.
    Standard,
    /// Everything in Standard, plus any long number, and emails or phone numbers inside web addresses.
    Strict,
}

// Raw content deliberately has neither Serialize nor Debug. The gate consumes it.
#[derive(Clone)]
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

#[derive(Clone)]
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

/// A frame as captured, kept in memory only until the recording is sent or discarded so the
/// person can change the masking level after recording. It has no Serialize or Debug.
#[derive(Clone)]
struct RawFrame {
    seq: u64,
    mono_ms: u64,
    app_name: String,
    window_title: String,
    width: u32,
    height: u32,
    png: Vec<u8>,
    ax_labels: Vec<String>,
    ocr_boxes: Vec<OcrBox>,
    regions: Vec<MaskRegion>,
    uncertain: bool,
    secure_input_gap: bool,
}

impl RawFrame {
    fn capture(frame: &RecordedFrame, regions: &[MaskRegion]) -> Option<Self> {
        let valid = frame.width > 0
            && frame.height > 0
            && frame.width <= 1600
            && frame.height <= 1600
            && u64::from(frame.width) * u64::from(frame.height) * 4 == frame.rgba.len() as u64;
        if !valid {
            return None;
        }
        let mut png = Vec::new();
        let mut encoder = png::Encoder::new(&mut png, frame.width, frame.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Fast);
        let mut writer = encoder.write_header().ok()?;
        writer.write_image_data(&frame.rgba).ok()?;
        writer.finish().ok()?;
        Some(Self {
            seq: frame.seq,
            mono_ms: frame.mono_ms,
            app_name: frame.app_name.clone(),
            window_title: frame.window_title.clone(),
            width: frame.width,
            height: frame.height,
            png,
            ax_labels: frame.ax_labels.clone(),
            ocr_boxes: frame.ocr_boxes.clone(),
            regions: regions.to_vec(),
            uncertain: frame.uncertain,
            secure_input_gap: frame.secure_input_gap,
        })
    }

    fn decode(&self) -> Option<RecordedFrame> {
        let mut reader = png::Decoder::new(std::io::Cursor::new(self.png.as_slice()))
            .read_info()
            .ok()?;
        let mut rgba = vec![0; reader.output_buffer_size().unwrap_or(0)];
        let info = reader.next_frame(&mut rgba).ok()?;
        rgba.truncate(info.buffer_size());
        Some(RecordedFrame {
            seq: self.seq,
            mono_ms: self.mono_ms,
            app_name: self.app_name.clone(),
            window_title: self.window_title.clone(),
            width: self.width,
            height: self.height,
            rgba,
            ax_labels: self.ax_labels.clone(),
            ocr_boxes: self.ocr_boxes.clone(),
            uncertain: self.uncertain,
            secure_input_gap: self.secure_input_gap,
        })
    }
}

#[derive(Clone)]
pub struct RecordingSession {
    id: String,
    frames: Vec<MaskedFrame>,
    raw: Vec<RawFrame>,
    level: MaskLevel,
    admission_dropped: Vec<DroppedFrame>,
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
    /// The library default hides everything Standard finds. The app starts recordings at Off.
    pub fn new() -> Self {
        Self::with_level(MaskLevel::Standard)
    }

    pub fn with_level(level: MaskLevel) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            frames: Vec::new(),
            raw: Vec::new(),
            level,
            admission_dropped: Vec::new(),
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
        let mut admitted = false;
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
            admitted = true;
            if let Some(raw) = RawFrame::capture(&frame, regions) {
                self.raw.push(raw);
            }
            masking::mask(frame, regions, detector, &mut self.mask_history, self.level)
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
                if !admitted && self.admission_dropped.len() < 150 {
                    self.admission_dropped.push(DroppedFrame { seq, reason });
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
        self.raw.retain(|f| f.seq != seq);
        self.frames.len() != before
    }

    pub fn level(&self) -> MaskLevel {
        self.level
    }

    /// Masks every kept frame again from its captured pixels at another level.
    pub fn set_level(&mut self, level: MaskLevel, detector: &impl SensitiveDetector) {
        self.level = level;
        self.mask_history = masking::MaskHistory::default();
        self.frames.clear();
        self.dropped = self.admission_dropped.clone();
        let raw = std::mem::take(&mut self.raw);
        for raw_frame in &raw {
            let seq = raw_frame.seq;
            let result = raw_frame
                .decode()
                .ok_or(DropReason::EncodingError)
                .and_then(|frame| {
                    masking::mask(
                        frame,
                        &raw_frame.regions,
                        detector,
                        &mut self.mask_history,
                        level,
                    )
                });
            match result {
                Ok(frame) => self.frames.push(frame),
                Err(reason) => {
                    if self.dropped.len() < 150 {
                        self.dropped.push(DroppedFrame { seq, reason });
                    }
                }
            }
        }
        self.raw = raw;
    }

    pub fn dropped(&self) -> &[DroppedFrame] {
        &self.dropped
    }
}

#[cfg(test)]
mod tests;
