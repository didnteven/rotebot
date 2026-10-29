use super::*;
use regex::Regex;

const MAX_DETECTOR_BYTES: usize = 65_536;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("sensitive-text detection failed")]
pub struct DetectorError;

/// An adapter must return an error for incomplete or uncertain detection, never `Ok(false)`.
pub trait SensitiveDetector {
    fn is_sensitive(&self, text: &str) -> Result<bool, DetectorError>;
}

pub struct TextDetector {
    patterns: Vec<Regex>,
    cards: Regex,
}

impl TextDetector {
    pub fn new() -> Result<Self, DetectorError> {
        let patterns = [
            r"(?i)[a-z0-9.!#$%&'*+/=?^_`{|}~-]+@[a-z0-9-]+(?:\.[a-z0-9-]+)+",
            r"\b[0-9]{3}[- ][0-9]{2}[- ][0-9]{4}\b",
            r"(?i)\b[A-Z]{2}[0-9]{2}(?:[ -]?[A-Z0-9]){11,30}\b",
            r"(?:\+[0-9][0-9 ().-]{5,}[0-9]|\(?[0-9]{2,4}\)?[ .-][0-9]{3,4}[ .-][0-9]{3,4}|\b[0-9]{10,15}\b)",
            r"[A-Za-z0-9_+/=-]{24,}",
            r"\b(?:AKIA|ASIA)[A-Z0-9]{16}\b",
            r"(?i)\b(?:sk|pk|ghp|gho|github_pat)[_-][A-Za-z0-9_-]{12,}\b",
        ]
        .into_iter()
        .map(Regex::new)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| DetectorError)?;
        Ok(Self {
            patterns,
            cards: Regex::new(r"\b[0-9](?:[ -]?[0-9]){12,18}\b").map_err(|_| DetectorError)?,
        })
    }
}

impl SensitiveDetector for TextDetector {
    fn is_sensitive(&self, text: &str) -> Result<bool, DetectorError> {
        if text.len() > MAX_DETECTOR_BYTES || text.contains('\0') {
            return Err(DetectorError);
        }
        let cards: Vec<_> = self.cards.find_iter(text).collect();
        if cards.iter().any(|m| luhn(m.as_str())) {
            return Ok(true);
        }
        for (index, pattern) in self.patterns.iter().enumerate() {
            for found in pattern.find_iter(text) {
                let candidate = found.as_str();
                if index == 3 {
                    // A formatted card must not be mistaken for a shorter phone substring.
                    if cards
                        .iter()
                        .any(|m| m.start() < found.end() && found.start() < m.end())
                    {
                        continue;
                    }
                    let digits = candidate.bytes().filter(u8::is_ascii_digit).count();
                    // Short dates and card-sized digit runs are not phone numbers.
                    if !(7..=15).contains(&digits) || candidate.len() <= digits && digits < 10 {
                        continue;
                    }
                }
                if index == 4
                    && !(candidate.bytes().any(|b| b.is_ascii_alphabetic())
                        && candidate.bytes().any(|b| b.is_ascii_digit()))
                {
                    continue;
                }
                return Ok(true);
            }
        }
        Ok(false)
    }
}

fn luhn(text: &str) -> bool {
    let digits: Vec<u32> = text
        .bytes()
        .filter(u8::is_ascii_digit)
        .map(|b| u32::from(b - b'0'))
        .collect();
    (13..=19).contains(&digits.len())
        && digits.iter().any(|&d| d != 0)
        && digits
            .iter()
            .rev()
            .enumerate()
            .map(|(i, &d)| {
                let n = if i % 2 == 1 { d * 2 } else { d };
                if n > 9 {
                    n - 9
                } else {
                    n
                }
            })
            .sum::<u32>()
            % 10
            == 0
}

fn padded_rect(rect: Rect, width: u32, height: u32) -> Rect {
    // Geometry has already been validated against the (at most 1600px) frame.
    let margin = 3.max((rect.height * 15).div_ceil(100));
    let x = rect.x.saturating_sub(margin);
    let y = rect.y.saturating_sub(margin);
    Rect {
        x,
        y,
        width: (rect.x + rect.width + margin).min(width) - x,
        height: (rect.y + rect.height + margin).min(height) - y,
    }
}

fn ocr_lines(boxes: &[OcrBox]) -> Vec<Vec<usize>> {
    let mut indices: Vec<_> = (0..boxes.len()).collect();
    indices.sort_by_key(|&i| (2 * boxes[i].rect.y + boxes[i].rect.height, boxes[i].rect.x));
    let mut lines: Vec<Vec<usize>> = Vec::new();
    for i in indices {
        let rect = boxes[i].rect;
        // Compare to a stable anchor so small offsets cannot chain across separate lines.
        let line = lines.iter_mut().find(|line| {
            let anchor = boxes[line[0]].rect;
            let overlap = (rect.y + rect.height)
                .min(anchor.y + anchor.height)
                .saturating_sub(rect.y.max(anchor.y));
            let centre_distance = (2 * rect.y + rect.height).abs_diff(2 * anchor.y + anchor.height);
            overlap * 2 >= rect.height.min(anchor.height)
                || centre_distance <= rect.height.min(anchor.height)
        });
        if let Some(line) = line {
            line.push(i);
        } else {
            lines.push(vec![i]);
        }
    }
    for line in &mut lines {
        line.sort_by_key(|&i| (boxes[i].rect.x, boxes[i].rect.y));
    }
    lines
}

fn sensitive_run(
    boxes: &[OcrBox],
    run: &[usize],
    detector: &impl SensitiveDetector,
) -> Result<bool, DropReason> {
    let bytes = run.iter().map(|&i| boxes[i].text.len()).sum::<usize>() + run.len() - 1;
    if bytes > MAX_DETECTOR_BYTES {
        return Err(DropReason::DetectorError);
    }
    let mut joined = String::with_capacity(bytes);
    for (position, &i) in run.iter().enumerate() {
        if position > 0 {
            joined.push(' ');
        }
        joined.push_str(&boxes[i].text);
    }
    if detector
        .is_sensitive(&joined)
        .map_err(|_| DropReason::DetectorError)?
    {
        return Ok(true);
    }
    // An inserted space can break an email split at '@' or within its domain.
    // This variant is shorter than the bounded, space-joined run above.
    let compact: String = run.iter().map(|&i| boxes[i].text.as_str()).collect();
    detector
        .is_sensitive(&compact)
        .map_err(|_| DropReason::DetectorError)
}

pub(super) fn mask(
    mut frame: RecordedFrame,
    regions: &[MaskRegion],
    detector: &impl SensitiveDetector,
) -> Result<MaskedFrame, DropReason> {
    // A doubtful frame is kept, masked as far as it could be checked, and flagged for the person.
    let unverified = frame.uncertain;
    if frame.secure_input_gap {
        return Err(DropReason::SecureInputGap);
    }
    if frame.width == 0
        || frame.height == 0
        || frame.width > 1600
        || frame.height > 1600
        || u64::from(frame.width) * u64::from(frame.height) * 4 != frame.rgba.len() as u64
    {
        return Err(DropReason::InvalidPixels);
    }
    let metadata_bytes = frame
        .app_name
        .len()
        .saturating_add(frame.window_title.len())
        .saturating_add(frame.ax_labels.iter().map(String::len).sum::<usize>())
        .saturating_add(frame.ocr_boxes.iter().map(|b| b.text.len()).sum::<usize>());
    if metadata_bytes > MAX_DETECTOR_BYTES
        || frame.ocr_boxes.len() > 1024
        || frame.ax_labels.len() > 1024
        || regions.len() > 1024
    {
        return Err(DropReason::DetectorError);
    }
    if regions
        .iter()
        .any(|m| !m.rect.fits(frame.width, frame.height))
        || frame
            .ocr_boxes
            .iter()
            .any(|b| !b.rect.fits(frame.width, frame.height))
    {
        return Err(DropReason::InvalidGeometry);
    }
    let sensitive = |s: &str| {
        detector
            .is_sensitive(s)
            .map_err(|_| DropReason::DetectorError)
    };
    let mut detected = Vec::with_capacity(frame.ocr_boxes.len());
    for b in &frame.ocr_boxes {
        detected.push(sensitive(&b.text)?);
    }
    for line in ocr_lines(&frame.ocr_boxes) {
        if line.len() < 2 {
            continue;
        }
        if sensitive_run(&frame.ocr_boxes, &line, detector)? {
            for &i in &line {
                detected[i] = true;
            }
        }
        for pair in line.windows(2) {
            if sensitive_run(&frame.ocr_boxes, pair, detector)? {
                for &i in pair {
                    detected[i] = true;
                }
            }
        }
    }
    let masks: Vec<_> = regions
        .iter()
        .copied()
        .chain(
            frame
                .ocr_boxes
                .iter()
                .zip(detected)
                .filter_map(|(b, detected)| {
                    detected.then_some(MaskRegion {
                        rect: b.rect,
                        reason: MaskReason::SensitiveText,
                    })
                }),
        )
        .map(|mask| MaskRegion {
            rect: padded_rect(mask.rect, frame.width, frame.height),
            ..mask
        })
        .collect();
    let repeats_masked_text = frame.ocr_boxes.iter().any(|b| {
        !b.text.is_empty()
            && masks.iter().any(|m| m.rect.intersects(b.rect))
            && frame.app_name.contains(&b.text)
    });
    let app_name = if sensitive(&frame.app_name)? || repeats_masked_text {
        "[redacted]".into()
    } else {
        frame.app_name
    };
    // Labels have no geometry. With any mask, none can be safely associated with visible pixels.
    let mut ax_labels = Vec::new();
    for label in frame.ax_labels {
        if !sensitive(&label)? && masks.is_empty() {
            ax_labels.push(label);
        }
    }
    let ocr_boxes = frame
        .ocr_boxes
        .into_iter()
        .filter(|b| !masks.iter().any(|m| m.rect.intersects(b.rect)))
        .map(|b| ClearedOcrBox {
            text: b.text,
            rect: b.rect,
        })
        .collect();
    for mask in &masks {
        for y in mask.rect.y..mask.rect.y + mask.rect.height {
            for x in mask.rect.x..mask.rect.x + mask.rect.width {
                let offset = ((y as usize * frame.width as usize) + x as usize) * 4;
                frame.rgba[offset..offset + 4].copy_from_slice(&[0, 0, 0, 255]);
            }
        }
    }
    let mut png = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png, frame.width, frame.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|_| DropReason::EncodingError)?;
        writer
            .write_image_data(&frame.rgba)
            .map_err(|_| DropReason::EncodingError)?;
        writer.finish().map_err(|_| DropReason::EncodingError)?;
    }
    Ok(MaskedFrame {
        text: FrameText {
            seq: frame.seq,
            mono_ms: frame.mono_ms,
            app_name,
            window_title: None,
            ax_labels,
            ocr_boxes,
            masks,
        },
        width: frame.width,
        height: frame.height,
        png,
        unverified,
    })
}
