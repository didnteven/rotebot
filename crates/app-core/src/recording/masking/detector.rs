use super::{DetectorError, SensitiveDetector, MAX_DETECTOR_BYTES};
use regex::Regex;
use std::ops::Range;

/// Rules operate on whitespace-normalised text, then map byte spans back to the OCR input.
/// No language models, downloads, logging, or content persistence are involved.
pub struct TextDetector {
    email: Regex,
    keys: Regex,
    iban: Regex,
    numbers: Regex,
    token: Regex,
    url: Regex,
    assignment: Regex,
    label: Regex,
}

impl TextDetector {
    pub fn new() -> Result<Self, DetectorError> {
        let re = |s| Regex::new(s).map_err(|_| DetectorError);
        Ok(Self {
            email: re(
                r"(?i)[\p{L}\p{N}.!#$%&'*+/=?^_`{|}~-]+\s*@\s*[\p{L}\p{N}-]+(?:\s*\.\s*[\p{L}\p{N}-]+)+",
            )?,
            keys: re(
                r"\b(?:(?:AKIA|ASIA)[A-Z0-9]{16}|(?:sk|pk|ghp|gho|ghs|ghu|github_pat)[_-][A-Za-z0-9_-]{12,}|eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+)\b",
            )?,
            iban: re(r"(?i)\b[A-Z]{2}[0-9OIl]{2}(?:[ -]?[A-Z0-9]){11,30}\b")?,
            // Maximal numeric runs prevent phone substrings in invalid cards/dates/amounts.
            numbers: re(r"(?:[+(] *[0-9OIl]|[0-9]|[OIl][0-9])[0-9OIl () .-]*[0-9OIl]")?,
            token: re(r"[A-Za-z0-9_+/=-]{24,}")?,
            url: re(
                r#"(?i)(?:[a-z][a-z0-9+.-]*://|www\.|\b(?:[a-z0-9-]+\.)+[a-z]{2,63}\b|(?:^|\s)[/~])[^\s<>"']*"#,
            )?,
            assignment: re(
                r"(?i)(?:^|[?&#;/\s])(?:[a-z0-9_]*[_-])?(?:token|key|secret|password|session|auth|authorization|code|sig|signature|apikey|accesstoken|refreshtoken|clientsecret|sessionid|authcode)(?:[_-][a-z0-9_]+)?\s*[=:]\s*([^\s&#;]+)",
            )?,
            label: re(
                r"(?i)\b(?:account\s*(?:number|no\.?)|(?:phone|mobile|telephone)\s*number|password|passcode|pin|cvv|cvc|card\s*(?:number|no\.?)|routing(?:\s*number)?|iban|swift(?:\s*(?:code|bic))?|date\s*of\s*birth|dob|tax\s*(?:number|id)|ird(?:\s*number)?|ssn|social\s*security(?:\s*number)?)\b\s*[:=]?\s*",
            )?,
        })
    }

    fn detect(&self, input: &str) -> Result<Vec<Range<usize>>, DetectorError> {
        if input.len() > MAX_DETECTOR_BYTES || input.contains('\0') {
            return Err(DetectorError);
        }
        let (text, map) = normalise(input, self.requires_line_context(input));
        let mut spans = Vec::new();
        for m in self.email.find_iter(&text) {
            // URL query/path delimiters are not part of an email local-part.
            let at = m.as_str().find('@').unwrap();
            let prefix = &m.as_str()[..at];
            let in_url = self
                .url
                .find_iter(&text)
                .any(|u| u.start() <= m.start() && m.end() <= u.end());
            let skip = if in_url {
                prefix.rfind(['/', '=', '?', '&']).map_or(0, |i| i + 1)
            } else {
                0
            };
            spans.push(m.start() + skip..m.end());
        }
        spans.extend(self.keys.find_iter(&text).map(|m| m.range()));
        for m in self.iban.find_iter(&text) {
            // Structural IBAN plus country-specific lengths. OCR can damage the checksum;
            // requiring mod-97 here would create privacy misses.
            let s: String = m
                .as_str()
                .chars()
                .filter(|c| c.is_ascii_alphanumeric())
                .collect::<String>()
                .to_ascii_uppercase();
            let length = match &s[..2] {
                "GB" | "IE" => 22,
                "NZ" => 0,
                "DE" => 22,
                "FR" => 27,
                "NL" => 18,
                "ES" => 24,
                "IT" => 27,
                "CH" => 21,
                "BE" => 16,
                "AT" => 20,
                "AU" | "US" => 0,
                _ => {
                    if s.bytes().filter(u8::is_ascii_digit).count() >= 6 {
                        s.len()
                    } else {
                        0
                    }
                }
            };
            if length >= 15 && s.len() >= length {
                // The greedy expression can include trailing words; take only the IBAN.
                let end = m.start()
                    + m.as_str()
                        .char_indices()
                        .filter(|(_, c)| c.is_ascii_alphanumeric())
                        .nth(length - 1)
                        .unwrap()
                        .0
                    + 1;
                spans.push(m.start()..end);
            }
        }
        for m in self.numbers.find_iter(&text) {
            let raw = m.as_str().trim();
            let start = m.start() + m.as_str().find(raw).unwrap_or(0);
            let end = start + raw.len();
            if raw.is_empty() || !raw.bytes().any(|b| b.is_ascii_digit()) {
                continue;
            }
            // OCR confusions are repaired only inside strongly numeric runs, never prose/URLs.
            let digits: String = raw
                .chars()
                .filter_map(|c| match c {
                    '0'..='9' => Some(c),
                    'O' => Some('0'),
                    'I' | 'l' => Some('1'),
                    _ => None,
                })
                .collect();
            let n = digits.len();
            let groups: Vec<_> = raw
                .split(|c: char| !c.is_ascii_alphanumeric())
                .filter(|s| !s.is_empty())
                .map(str::len)
                .collect();
            let inside_url = self
                .url
                .find_iter(&text)
                .any(|u| u.start() < end && start < u.end());
            let currency = text[..start].trim_end();
            let amount = currency.ends_with(['$', '£', '€', '¥'])
                || ["NZD", "USD", "EUR", "GBP", "AUD"]
                    .iter()
                    .any(|c| currency.ends_with(c))
                || start > 0 && matches!(text.as_bytes()[start - 1], b'$' | b',')
                || raw.contains('.') && !raw.starts_with('+') && groups.last() == Some(&2);
            let date = groups.len() == 3
                && (groups[0] == 4 && groups[1] <= 2 && groups[2] <= 2
                    || groups[2] == 4 && groups[0] <= 2 && groups[1] <= 2);
            let nz_bank = groups == [2, 4, 7, 2] || groups == [2, 4, 7, 3];
            let ssn = groups == [3, 2, 4];
            let ird = groups == [3, 3, 3]
                || groups == [2, 3, 3]
                || (n == 8 || n == 9) && groups.len() == 1 && valid_ird(&digits);
            let phone = (7..=15).contains(&n)
                && (raw.starts_with('+')
                    || raw.starts_with('(')
                    || n >= 10 && n <= 15 && groups.len() == 1
                    || n >= 9 && digits.starts_with('0')
                    || groups == [3, 4]
                    || groups.len() == 3 && groups[0] == 3 && groups[1] == 3 && groups[2] == 4
                    || groups.len() == 3 && groups[0] == 2 && groups[1] == 3 && groups[2] == 4);
            if !amount
                && !date
                && (nz_bank || ssn || luhn(&digits) || !inside_url && (ird || phone))
            {
                spans.push(start..end);
            }
        }
        for m in self.token.find_iter(&text) {
            // URL/path separators delimit components. Long readable slugs and ordinary words
            // with a year suffix are not secrets. Hex and high-diversity encoded values are.
            let in_url = self
                .url
                .find_iter(&text)
                .any(|u| u.start() <= m.start() && m.end() <= u.end());
            for (offset, component) in components(m.as_str(), !in_url) {
                if token_like(component) {
                    spans.push(m.start() + offset..m.start() + offset + component.len());
                }
            }
        }
        for captures in self.assignment.captures_iter(&text) {
            let m = captures.get(1).unwrap();
            spans.push(m.range());
        }
        let labels: Vec<_> = self
            .label
            .find_iter(&text)
            .filter(|m| text[..m.start()].trim().is_empty() || m.as_str().contains([':', '=']))
            .collect();
        for (i, label) in labels.iter().enumerate() {
            let end = labels.get(i + 1).map_or(text.len(), |m| m.start());
            let rest = text[label.end()..end].trim_end();
            let another_label = self
                .label
                .find(rest)
                .is_some_and(|m| m.start() == 0 && m.end() == rest.len());
            if !rest.is_empty() && !navigation_value(rest) && !another_label {
                spans.push(label.end()..label.end() + rest.len());
            }
        }
        let mut mapped: Vec<_> = spans
            .into_iter()
            .filter(|s| !s.is_empty())
            .map(|s| map[s.start].start..map[s.end - 1].end)
            .collect();
        mapped.sort_by_key(|r| (r.start, r.end));
        let mut merged: Vec<Range<usize>> = Vec::new();
        for span in mapped {
            if let Some(last) = merged.last_mut().filter(|last| span.start <= last.end) {
                last.end = last.end.max(span.end);
            } else {
                merged.push(span);
            }
        }
        Ok(merged)
    }
}

impl SensitiveDetector for TextDetector {
    fn is_sensitive(&self, text: &str) -> Result<bool, DetectorError> {
        Ok(!self.detect(text)?.is_empty())
    }

    fn sensitive_ranges(&self, text: &str) -> Result<Vec<Range<usize>>, DetectorError> {
        self.detect(text)
    }

    fn requires_line_context(&self, text: &str) -> bool {
        self.url
            .find(text.trim_start())
            .is_some_and(|m| m.start() == 0)
    }

    fn label_ranges(&self, text: &str) -> Result<Vec<Range<usize>>, DetectorError> {
        if text.len() > MAX_DETECTOR_BYTES || text.contains('\0') {
            return Err(DetectorError);
        }
        Ok(self
            .label
            .find(text)
            .filter(|m| text[..m.start()].trim().is_empty() && text[m.end()..].trim().is_empty())
            .map(|m| vec![m.range()])
            .unwrap_or_default())
    }
}

fn navigation_value(text: &str) -> bool {
    matches!(
        text.trim().to_ascii_lowercase().as_str(),
        "settings"
            | "reset"
            | "help"
            | "policy"
            | "requirements"
            | "manager"
            | "strength"
            | "required"
            | "changed"
            | "saved"
            | "details"
            | "information"
            | "overview"
            | "guide"
            | "support"
            | "and security"
    )
}

fn components(text: &str, allow_whole: bool) -> Vec<(usize, &str)> {
    // Preserve '+' and '=' inside base64. Slash can occur in base64 too, so also
    // examine the entire run when it has no URL-like punctuation/path words.
    let mut result = Vec::new();
    let mut offset = 0;
    for s in text.split('/') {
        result.push((offset, s));
        offset += s.len() + 1;
    }
    if allow_whole && text.contains('/') && !text.contains('-') {
        result.push((0, text));
    }
    result
}

fn token_like(text: &str) -> bool {
    let s = text.trim_end_matches('=');
    if s.len() < 24 {
        return false;
    }
    let bytes = s.as_bytes();
    let digits = bytes.iter().filter(|b| b.is_ascii_digit()).count();
    let letters = bytes.iter().filter(|b| b.is_ascii_alphabetic()).count();
    let hex = bytes.iter().all(u8::is_ascii_hexdigit);
    let upper = bytes.iter().filter(|b| b.is_ascii_uppercase()).count();
    let lower = bytes.iter().filter(|b| b.is_ascii_lowercase()).count();
    let mut seen = [false; 128];
    for &b in bytes {
        if b < 128 {
            seen[b as usize] = true;
        }
    }
    let distinct = seen.iter().filter(|&&b| b).count();
    // Keep the conservative mixed-token rule, but require distributed digits:
    // ordinary report names ending in 2026 should not trigger it.
    let quarters_with_digits = bytes
        .chunks(bytes.len().div_ceil(4))
        .filter(|part| part.iter().any(u8::is_ascii_digit))
        .count();
    (hex && letters > 0)
        || distinct >= 18 && upper * 3 >= bytes.len() && lower * 3 >= bytes.len()
        || letters > 0 && digits >= 3 && quarters_with_digits >= 3
        || letters > 0
            && distinct >= 18
            && (s.contains('+') || s.contains('/') || text.ends_with('='))
}

fn valid_ird(digits: &str) -> bool {
    let padded = format!("{digits:0>9}");
    let d: Vec<u32> = padded.bytes().map(|b| u32::from(b - b'0')).collect();
    if d.len() != 9 || d.iter().all(|&n| n == 0) {
        return false;
    }
    let check = |w: &[u32]| {
        let n = 11 - d[..8].iter().zip(w).map(|(d, w)| d * w).sum::<u32>() % 11;
        if n == 11 {
            0
        } else {
            n
        }
    };
    let first = check(&[3, 2, 7, 6, 5, 4, 3, 2]);
    let expected = if first == 10 {
        check(&[7, 4, 3, 2, 5, 2, 7, 6])
    } else {
        first
    };
    expected == d[8]
}

fn luhn(text: &str) -> bool {
    let d: Vec<u32> = text
        .bytes()
        .filter(u8::is_ascii_digit)
        .map(|b| u32::from(b - b'0'))
        .collect();
    (13..=19).contains(&d.len())
        && d.iter().any(|&n| n != 0)
        && d.iter()
            .rev()
            .enumerate()
            .map(|(i, &n)| {
                let n = if i % 2 == 1 { n * 2 } else { n };
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

fn normalise(text: &str, compact_url: bool) -> (String, Vec<Range<usize>>) {
    let mut output = String::with_capacity(text.len());
    let mut map: Vec<Range<usize>> = Vec::with_capacity(text.len());
    let mut chars = text.char_indices();
    while let Some((i, c)) = chars.next() {
        // Decode printable ASCII URL escapes while retaining their exact input
        // span. This finds %40 emails and escaped secret names without fetching
        // a URL, and never interprets an escape as a control character.
        if compact_url && c == '%' {
            let mut lookahead = chars.clone();
            if let (Some((_, a)), Some((last, b))) = (lookahead.next(), lookahead.next()) {
                if let (Some(a), Some(b)) = (a.to_digit(16), b.to_digit(16)) {
                    let byte = (a * 16 + b) as u8;
                    if byte.is_ascii_graphic() {
                        output.push(char::from(byte));
                        map.push(i..last + 1);
                        chars.next();
                        chars.next();
                        continue;
                    }
                }
            }
        }
        let end = i + c.len_utf8();
        if c.is_whitespace() {
            if compact_url {
                continue;
            }
            if output.ends_with(' ') {
                map.last_mut().unwrap().end = end;
            } else {
                output.push(' ');
                map.push(i..end);
            }
        } else {
            output.push(c);
            map.extend((0..c.len_utf8()).map(|_| i..end));
        }
    }
    (output, map)
}
