use super::{line_rect, run_text};
use crate::recording::{DropReason, RecordedFrame, Rect};

#[derive(Default, Clone)]
pub(in crate::recording) struct MaskHistory {
    // Raw OCR history is bounded, memory-only, has no Debug/Serialize, and dies
    // with the recording session. Never shared across sessions or sent in previews.
    lines: Vec<RememberedLine>,
    dimensions: (u32, u32),
    app: String,
}

#[derive(Clone)]
struct RememberedLine {
    text: String,
    rect: Rect,
    masks: Vec<Rect>,
    detected_ms: u64,
    detected_seq: u64,
}

impl MaskHistory {
    pub(super) fn smooth(
        &self,
        frame: &RecordedFrame,
        lines: &[Vec<usize>],
        mut masks: Vec<Rect>,
    ) -> Result<(Vec<Rect>, Self), DropReason> {
        let mut next = Self {
            lines: Vec::new(),
            dimensions: (frame.width, frame.height),
            app: frame.app_name.clone(),
        };
        let usable = self.dimensions == next.dimensions && self.app == next.app;
        let mut output = Vec::new();
        for line in lines {
            let rect = line_rect(&frame.ocr_boxes, line);
            let (text, _) = run_text(&frame.ocr_boxes, line, false)?;
            let text = canonical(&text);
            let mut own: Vec<_> = masks
                .iter()
                .copied()
                .filter(|m| m.intersects(rect))
                .collect();
            let current = own.clone();
            let previous = usable
                .then(|| {
                    self.lines.iter().find(|old| {
                        frame.mono_ms.saturating_sub(old.detected_ms) <= 3_000
                            && frame.seq.saturating_sub(old.detected_seq) <= 3
                            && rect.x.abs_diff(old.rect.x) <= 6
                            && rect.y.abs_diff(old.rect.y) <= 6
                            && rect.width.abs_diff(old.rect.width) <= 12
                            && rect.height.abs_diff(old.rect.height) <= 6
                            && similar(&text, &old.text)
                    })
                })
                .flatten();
            // A stable envelope prevents sub-box/OCR jitter from uncovering an
            // edge of a previously sensitive region. No carry onto changed pages.
            if let Some(old) = previous {
                for &m in &old.masks {
                    let dx = i64::from(rect.x) - i64::from(old.rect.x);
                    let dy = i64::from(rect.y) - i64::from(old.rect.y);
                    let x = i64::from(m.x).min(i64::from(m.x) + dx).max(0) as u32;
                    let y = i64::from(m.y).min(i64::from(m.y) + dy).max(0) as u32;
                    let right = (i64::from(m.x + m.width).max(i64::from(m.x + m.width) + dx)
                        as u32)
                        .min(frame.width);
                    let bottom = (i64::from(m.y + m.height).max(i64::from(m.y + m.height) + dy)
                        as u32)
                        .min(frame.height);
                    own.push(Rect {
                        x,
                        y,
                        width: right - x,
                        height: bottom - y,
                    });
                }
            }
            if own.is_empty() {
                continue;
            }
            let freshly_detected = masks.iter().any(|m| m.intersects(rect));
            let (detected_ms, detected_seq) = if freshly_detected {
                (frame.mono_ms, frame.seq)
            } else {
                let old = previous.unwrap();
                (old.detected_ms, old.detected_seq)
            };
            // Merge overlapping envelopes but retain separate boxes for existing
            // per-box geometry and to avoid masking the space between fields.
            let mut unique: Vec<Rect> = Vec::new();
            for r in own {
                if let Some(prior) = unique.iter_mut().find(|p| p.intersects(r)) {
                    let right = (prior.x + prior.width).max(r.x + r.width);
                    let bottom = (prior.y + prior.height).max(r.y + r.height);
                    prior.x = prior.x.min(r.x);
                    prior.y = prior.y.min(r.y);
                    prior.width = right - prior.x;
                    prior.height = bottom - prior.y;
                } else {
                    unique.push(r);
                }
            }
            output.extend(unique.iter().copied());
            // Fresh detections replace the stored geometry, so the smoothing
            // envelope cannot grow cumulatively during a long static recording.
            let remembered = if freshly_detected { current } else { unique };
            next.lines.push(RememberedLine {
                text,
                rect,
                masks: remembered,
                detected_ms,
                detected_seq,
            });
        }
        // Use each line's smoothed regions; masks outside text rows are never inferred.
        masks.clear();
        masks.extend(output);
        masks.sort_by_key(|r| (r.y, r.x, r.width, r.height));
        masks.dedup();
        Ok((masks, next))
    }
}

fn canonical(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'o' => '0',
            'l' | 'i' => '1',
            _ => c,
        })
        .collect()
}

fn similar(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    // Bounded edit distance tolerates a few OCR substitutions/insertions. Very
    // short strings require equality to avoid sticking PINs onto unrelated text.
    let a: Vec<_> = a.chars().collect();
    let b: Vec<_> = b.chars().collect();
    let limit = (a.len().min(b.len()) / 10).min(4);
    if limit == 0 || a.len().abs_diff(b.len()) > limit {
        return false;
    }
    let mut prior: Vec<_> = (0..=b.len()).collect();
    let mut row = vec![limit + 1; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        row[0] = i + 1;
        let from = (i + 1).saturating_sub(limit).max(1);
        let to = (i + 1 + limit).min(b.len());
        if from > 1 {
            row[from - 1] = limit + 1;
        }
        if to < b.len() {
            row[to + 1] = limit + 1;
        }
        for j in from..=to {
            row[j] = (row[j - 1] + 1)
                .min(prior[j] + 1)
                .min(prior[j - 1] + usize::from(*ca != b[j - 1]));
        }
        if row[from..=to].iter().copied().min().unwrap().min(row[0]) > limit {
            return false;
        }
        std::mem::swap(&mut prior, &mut row);
    }
    prior[b.len()] <= limit
}
