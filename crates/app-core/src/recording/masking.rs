use super::*;
use std::ops::Range;

const MAX_DETECTOR_BYTES: usize = 65_536;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("sensitive-text detection failed")]
pub struct DetectorError;

/// An adapter must return an error for incomplete or uncertain detection, never `Ok(false)`.
pub trait SensitiveDetector {
    fn is_sensitive(&self, text: &str) -> Result<bool, DetectorError>;

    /// UTF-8 byte ranges in the original input. Older adapters remain fail-closed
    /// by masking the whole input when their boolean detector finds a match.
    fn sensitive_ranges(&self, text: &str) -> Result<Vec<Range<usize>>, DetectorError> {
        Ok(if self.is_sensitive(text)? {
            vec![0..text.len()]
        } else {
            vec![]
        })
    }

    /// Detectors may require the full line as authority (for example an address
    /// bar whose numeric path fragments are harmless only in URL context).
    fn requires_line_context(&self, _text: &str) -> bool {
        false
    }

    /// Standalone field labels for spatial association with the next row.
    fn label_ranges(&self, _text: &str) -> Result<Vec<Range<usize>>, DetectorError> {
        Ok(vec![])
    }
}

mod detector;
mod temporal;
pub use detector::TextDetector;
pub(super) use temporal::MaskHistory;

#[cfg(test)]
mod corpus;
#[cfg(test)]
mod legacy;

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

/// A run keeps the byte provenance of every box, including in the compact variant.
fn run_text(
    boxes: &[OcrBox],
    run: &[usize],
    compact: bool,
) -> Result<(String, Vec<Range<usize>>), DropReason> {
    let bytes =
        run.iter().map(|&i| boxes[i].text.len()).sum::<usize>() + run.len().saturating_sub(1);
    if bytes > MAX_DETECTOR_BYTES {
        return Err(DropReason::DetectorError);
    }
    let mut text = String::with_capacity(bytes);
    let mut offsets = Vec::new();
    for (position, &i) in run.iter().enumerate() {
        if position > 0 && !compact {
            text.push(' ');
        }
        let start = text.len();
        text.push_str(&boxes[i].text);
        offsets.push(start..text.len());
    }
    Ok((text, offsets))
}

fn checked_ranges(
    detector: &impl SensitiveDetector,
    text: &str,
) -> Result<Vec<Range<usize>>, DropReason> {
    let ranges = detector
        .sensitive_ranges(text)
        .map_err(|_| DropReason::DetectorError)?;
    if ranges.iter().any(|r| {
        r.start > r.end
            || r.end > text.len()
            || !text.is_char_boundary(r.start)
            || !text.is_char_boundary(r.end)
    }) {
        return Err(DropReason::DetectorError);
    }
    Ok(ranges)
}

fn detect_run(
    boxes: &[OcrBox],
    run: &[usize],
    detector: &impl SensitiveDetector,
    detected: &mut [Vec<Range<usize>>],
) -> Result<(), DropReason> {
    let (text, offsets) = run_text(boxes, run, false)?;
    let spans = checked_ranges(detector, &text)?;
    let compact = spans.is_empty() && run.len() > 1;
    for span in spans {
        distribute(&span, run, &offsets, detected);
    }
    if compact {
        let (text, offsets) = run_text(boxes, run, true)?;
        for span in checked_ranges(detector, &text)? {
            distribute(&span, run, &offsets, detected);
        }
    }
    Ok(())
}

fn distribute(
    span: &Range<usize>,
    run: &[usize],
    offsets: &[Range<usize>],
    detected: &mut [Vec<Range<usize>>],
) {
    for (&i, offset) in run.iter().zip(offsets) {
        let start = span.start.max(offset.start);
        let end = span.end.min(offset.end);
        if start < end {
            detected[i].push(start - offset.start..end - offset.start);
        }
    }
}

fn sub_rect(b: &OcrBox, range: Range<usize>) -> Rect {
    let count = b.text.chars().count().max(1) as u32;
    let start = b.text[..range.start].chars().count() as u32;
    let end = b.text[..range.end].chars().count() as u32;
    let x = b.rect.x + b.rect.width * start / count;
    let right = b.rect.x + (b.rect.width * end).div_ceil(count);
    Rect {
        x,
        width: (right - x).max(1),
        ..b.rect
    }
}

fn line_rect(boxes: &[OcrBox], line: &[usize]) -> Rect {
    let x = line.iter().map(|&i| boxes[i].rect.x).min().unwrap();
    let y = line.iter().map(|&i| boxes[i].rect.y).min().unwrap();
    let right = line
        .iter()
        .map(|&i| boxes[i].rect.x + boxes[i].rect.width)
        .max()
        .unwrap();
    let bottom = line
        .iter()
        .map(|&i| boxes[i].rect.y + boxes[i].rect.height)
        .max()
        .unwrap();
    Rect {
        x,
        y,
        width: right - x,
        height: bottom - y,
    }
}

fn contextual_values(
    boxes: &[OcrBox],
    lines: &[Vec<usize>],
    detector: &impl SensitiveDetector,
    detected: &mut [Vec<Range<usize>>],
) -> Result<(), DropReason> {
    let mut labels = Vec::new();
    for b in boxes {
        if !detector
            .label_ranges(&b.text)
            .map_err(|_| DropReason::DetectorError)?
            .is_empty()
        {
            labels.push(b.rect);
        }
    }
    for line in lines {
        let (text, _) = run_text(boxes, line, false)?;
        if !detector
            .label_ranges(&text)
            .map_err(|_| DropReason::DetectorError)?
            .is_empty()
        {
            labels.push(line_rect(boxes, line));
        }
    }
    labels.sort_by_key(|r| (r.y, r.x, r.width, r.height));
    labels.dedup();
    for &label in &labels {
        // The closest aligned row below is the value. Another label or column
        // bounds the field; a distant row cannot inherit a sensitive label.
        let anchor = boxes
            .iter()
            .filter(|b| {
                let r = b.rect;
                r.y >= label.y + label.height
                    && r.y - (label.y + label.height) <= label.height * 2 + 12
                    && r.x.abs_diff(label.x) <= label.height * 2
            })
            .min_by_key(|b| b.rect.y);
        let Some(anchor) = anchor else {
            continue;
        };
        if !detector
            .label_ranges(&anchor.text)
            .map_err(|_| DropReason::DetectorError)?
            .is_empty()
        {
            continue;
        }
        let right = labels
            .iter()
            .filter(|r| r.y.abs_diff(label.y) <= label.height / 2 && r.x > label.x)
            .map(|r| r.x)
            .min()
            .unwrap_or(label.x + label.width.max(240));
        for (i, b) in boxes.iter().enumerate() {
            if b.rect.y.abs_diff(anchor.rect.y) <= anchor.rect.height / 2
                && b.rect.x >= anchor.rect.x.saturating_sub(2)
                && b.rect.x < right
                && detector
                    .label_ranges(&b.text)
                    .map_err(|_| DropReason::DetectorError)?
                    .is_empty()
            {
                detected[i].push(0..b.text.len());
            }
        }
    }
    Ok(())
}

pub(super) fn mask(
    mut frame: RecordedFrame,
    regions: &[MaskRegion],
    detector: &impl SensitiveDetector,
    history: &mut MaskHistory,
    level: MaskLevel,
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
    let off = level == MaskLevel::Off;
    let sensitive = |s: &str| {
        if off {
            return Ok(false);
        }
        detector
            .is_sensitive(s)
            .map_err(|_| DropReason::DetectorError)
    };
    let lines = ocr_lines(&frame.ocr_boxes);
    let mut detected = vec![Vec::new(); frame.ocr_boxes.len()];
    for line in lines.iter().filter(|_| !off) {
        let (joined, _) = run_text(&frame.ocr_boxes, line, false)?;
        let contextual = detector.requires_line_context(&joined);
        // Evaluate all adapters for errors even when their standalone decisions
        // are superseded by line context.
        let mut fallback = vec![Vec::new(); frame.ocr_boxes.len()];
        for &i in line {
            detect_run(&frame.ocr_boxes, &[i], detector, &mut fallback)?;
        }
        for pair in line.windows(2) {
            detect_run(&frame.ocr_boxes, pair, detector, &mut fallback)?;
        }
        if !contextual {
            for &i in line {
                detected[i].append(&mut fallback[i]);
            }
        }
        detect_run(&frame.ocr_boxes, line, detector, &mut detected)?;
    }
    if !off {
        contextual_values(&frame.ocr_boxes, &lines, detector, &mut detected)?;
    }
    let mut text_rects = Vec::new();
    for (b, spans) in frame.ocr_boxes.iter().zip(&mut detected) {
        spans.sort_by_key(|s| (s.start, s.end));
        let mut merged: Vec<Range<usize>> = Vec::new();
        for span in spans.drain(..).filter(|s| !s.is_empty()) {
            if let Some(last) = merged.last_mut().filter(|s| span.start <= s.end) {
                last.end = last.end.max(span.end);
            } else {
                merged.push(span);
            }
        }
        text_rects.extend(merged.into_iter().map(|span| sub_rect(b, span)));
    }
    // History is staged until all metadata detection and encoding have succeeded.
    let (text_rects, next_history) = history.smooth(&frame, &lines, text_rects)?;
    let masks: Vec<_> = regions
        .iter()
        .copied()
        .chain(text_rects.into_iter().map(|rect| MaskRegion {
            rect,
            reason: MaskReason::SensitiveText,
        }))
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
    *history = next_history;
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
