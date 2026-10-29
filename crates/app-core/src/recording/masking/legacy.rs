// Frozen pre-change detector, used only against fabricated test data.
use super::*;
use regex::Regex;
pub struct LegacyDetector {
    patterns: Vec<Regex>,
    cards: Regex,
}

impl LegacyDetector {
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

impl SensitiveDetector for LegacyDetector {
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
