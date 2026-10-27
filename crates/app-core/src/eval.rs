//! Scoring for the private generated-execution eval suite (plan §9.3).
//!
//! Pure functions only: revisioned output comparison, hidden checks, short
//! diff summaries, failure classification, and gate verdicts. The
//! `eval_suite` example drives `AppCore` and calls these; the maintained
//! tests call them directly with recorded replay answers. Scoring never
//! stores input rows or outputs beyond a pass/fail and a short diff summary.

use crate::dto::BuildFailureKind;
use serde::{Deserialize, Serialize};

/// Planning price constants from plan §9.3: Spark $1.20 per fresh
/// generation + $0.016 per contract. Spend-confirmation constants, not
/// measured charges.
pub const PRICE_FRESH_GENERATION_USD: f64 = 1.20;
/// Planning contract price from plan §9.3.
pub const PRICE_CONTRACT_USD: f64 = 0.016;
/// Planning per-attempt price: 1.20 + 0.016.
pub const PRICE_PER_ATTEMPT_USD: f64 = 1.216;

/// Planned maximum spend for a run: tasks × reps × $1.216.
pub fn planned_max_spend(tasks: usize, reps: usize) -> f64 {
    tasks as f64 * reps as f64 * PRICE_PER_ATTEMPT_USD
}

/// Extra checks beyond the exact byte match, from each task's `hidden.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HiddenChecks {
    /// Data rows excluding the header.
    pub row_count: usize,
    /// Exact header columns in order.
    pub columns: Vec<String>,
    /// Substrings that must not appear anywhere in the output bytes.
    pub must_not_contain: Vec<String>,
}

/// Attempt failure classes. Infrastructure failures (route unavailable,
/// timeout, usage limit) are labeled separately and still counted in the
/// denominator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureClass {
    BuildFailed,
    ChecksFailed,
    WrongOutput,
    ForbiddenEffect,
    Infrastructure,
}

impl FailureClass {
    /// Stable string for results files.
    pub fn as_str(self) -> &'static str {
        match self {
            FailureClass::BuildFailed => "build_failed",
            FailureClass::ChecksFailed => "checks_failed",
            FailureClass::WrongOutput => "wrong_output",
            FailureClass::ForbiddenEffect => "forbidden_effect",
            FailureClass::Infrastructure => "infrastructure",
        }
    }
}

/// Parse CSV bytes into header columns and data-row count.
pub fn parse_csv(bytes: &[u8]) -> Result<(Vec<String>, usize), String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "output is not valid UTF-8".to_string())?;
    let mut reader = csv::ReaderBuilder::new()
        .flexible(false)
        .from_reader(text.as_bytes());
    let columns: Vec<String> = reader
        .headers()
        .map_err(|e| format!("output has no header: {e}"))?
        .iter()
        .map(str::to_string)
        .collect();
    if columns.is_empty() {
        return Err("output has no header".to_string());
    }
    let mut count = 0usize;
    for record in reader.records() {
        record.map_err(|e| format!("output row is malformed: {e}"))?;
        count += 1;
    }
    Ok((columns, count))
}

/// First differing line between two byte strings, truncated to 200 chars.
/// Contains at most the one differing line, never full outputs.
pub fn diff_summary(staged: &[u8], expected: &[u8]) -> String {
    let a = String::from_utf8_lossy(staged);
    let b = String::from_utf8_lossy(expected);
    let a_lines: Vec<&str> = a.split('\n').collect();
    let b_lines: Vec<&str> = b.split('\n').collect();
    let n = a_lines.len().max(b_lines.len());
    for i in 0..n {
        let x = a_lines.get(i).copied().unwrap_or("<missing>");
        let y = b_lines.get(i).copied().unwrap_or("<missing>");
        if x != y {
            let mut s = format!("line {} differs", i + 1);
            let pair = format!(" expected {y:?} got {x:?}");
            s.push_str(&pair);
            return truncate(&s, 200);
        }
    }
    truncate("bytes differ without a differing line", 200)
}

fn truncate(s: &str, max: usize) -> String {
    let mut out: String = s.chars().take(max).collect();
    if s.chars().count() > max {
        out.push('…');
    }
    out
}

/// Score staged output bytes against the expected bytes and hidden checks.
/// Exact byte equality comes first; hidden checks run only on exact matches.
/// Every mismatch is `WrongOutput` with a short diff summary.
pub fn score_output(
    staged: &[u8],
    expected: &[u8],
    hidden: &HiddenChecks,
) -> Result<(), (FailureClass, String)> {
    score_output_revision(staged, expected, hidden, 1)
}

/// Revision 1 compares exact bytes. Revision 2 removes at most one final LF
/// from each side; CR, spaces, interior newlines and further LFs still differ.
/// Unknown revisions fail closed rather than silently changing historical scores.
pub fn score_output_revision(
    staged: &[u8],
    expected: &[u8],
    hidden: &HiddenChecks,
    revision: u64,
) -> Result<(), (FailureClass, String)> {
    let (actual, wanted) = match revision {
        1 => (staged, expected),
        2 => (
            staged.strip_suffix(b"\n").unwrap_or(staged),
            expected.strip_suffix(b"\n").unwrap_or(expected),
        ),
        _ => {
            return Err((
                FailureClass::WrongOutput,
                "unsupported manifest revision".into(),
            ))
        }
    };
    if actual != wanted {
        return Err((FailureClass::WrongOutput, diff_summary(actual, wanted)));
    }
    let (columns, row_count) =
        parse_csv(staged).map_err(|e| (FailureClass::WrongOutput, truncate(&e, 200)))?;
    if row_count != hidden.row_count {
        return Err((
            FailureClass::WrongOutput,
            truncate(
                &format!("row_count {row_count} != {}", hidden.row_count),
                200,
            ),
        ));
    }
    if columns != hidden.columns {
        return Err((
            FailureClass::WrongOutput,
            truncate("columns differ from hidden.json", 200),
        ));
    }
    let text = String::from_utf8_lossy(staged);
    for banned in &hidden.must_not_contain {
        if text.contains(banned.as_str()) {
            return Err((
                FailureClass::WrongOutput,
                truncate("output contains a hidden must_not_contain string", 200),
            ));
        }
    }
    Ok(())
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    let lower = haystack.to_lowercase();
    needles.iter().any(|n| lower.contains(n))
}

/// Classify a build failure into an eval failure class.
pub fn classify_build_failure(kind: BuildFailureKind, message: &str) -> FailureClass {
    if contains_any(message, &["forbidden"]) {
        return FailureClass::ForbiddenEffect;
    }
    // Host checks rejected the generated program; the sandbox message can
    // contain outage words ("unavailable") that must not relabel it.
    if kind == BuildFailureKind::Checks
        || contains_any(
            message,
            &[
                "checks failed",
                "generated runner failed",
                "javascript execution failed",
            ],
        )
    {
        return FailureClass::ChecksFailed;
    }
    if contains_any(
        message,
        &[
            "quota",
            "usage limit",
            "rate limit",
            "rate-limit",
            "timed out",
            "timeout",
            "deadline exceeded",
            "unavailable",
            "not signed in",
            "not installed",
            "sign-in",
            "sign in",
            "local only",
            "stopping",
            "turnfailed",
            "turn failed",
            "transport",
            "billing",
            "budget",
            "no model",
            "route",
            "connection",
        ],
    ) {
        return FailureClass::Infrastructure;
    }
    match kind {
        BuildFailureKind::Quota
        | BuildFailureKind::Budget
        | BuildFailureKind::NoModelAccess
        | BuildFailureKind::Auth => FailureClass::Infrastructure,
        BuildFailureKind::Checks => FailureClass::ChecksFailed,
        BuildFailureKind::Malformed => FailureClass::BuildFailed,
        BuildFailureKind::Internal => FailureClass::BuildFailed,
    }
}

/// True when a build failure is a usage-limit failure: the whole run stops
/// and reports INCOMPLETE so the missing denominator is never hidden.
pub fn is_usage_limit(kind: BuildFailureKind, message: &str) -> bool {
    kind == BuildFailureKind::Quota
        || contains_any(
            message,
            &["quota", "usage limit", "rate limit", "rate-limit"],
        )
}

/// Gate verdict over a run. INCOMPLETE when any attempt is missing; otherwise
/// PASS when the pass rate meets the threshold, FAIL below it.
pub fn verdict(passed: usize, total: usize, missing: usize, threshold: f64) -> &'static str {
    if missing > 0 {
        return "INCOMPLETE";
    }
    if total == 0 {
        return "INCOMPLETE";
    }
    let rate = passed as f64 / total as f64;
    if rate >= threshold {
        "PASS"
    } else {
        "FAIL"
    }
}

fn infra_keywords(message: &str) -> bool {
    let lower = message.to_lowercase();
    [
        "quota",
        "usage limit",
        "rate limit",
        "timed out",
        "timeout",
        "unavailable",
        "not signed in",
        "not installed",
        "sign in",
        "local only",
        "stopping",
        "transport",
        "billing",
        "budget",
        "no model",
    ]
    .iter()
    .any(|k| lower.contains(k))
}

/// Classify a failed attempt. A program that the sandbox or the host checks
/// reject is a generation failure, even when the sandbox's message happens to
/// contain an outage word such as "unavailable".
pub fn classify_failure(message: &str, stage: &str) -> FailureClass {
    if message.to_lowercase().contains("forbidden") {
        return FailureClass::ForbiddenEffect;
    }
    let lower = message.to_lowercase();
    let generated = [
        "checks failed",
        "generated runner failed",
        "javascript execution failed",
    ]
    .iter()
    .any(|k| lower.contains(k));
    if !generated && infra_keywords(message) {
        return FailureClass::Infrastructure;
    }
    if stage == "run" {
        FailureClass::ChecksFailed
    } else {
        FailureClass::BuildFailed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_match_passes_hidden_checks() {
        let hidden = HiddenChecks {
            row_count: 1,
            columns: vec!["id".into(), "v".into()],
            must_not_contain: vec!["banned-zzz".into()],
        };
        assert!(score_output(b"id,v\nA-001,1\n", b"id,v\nA-001,1\n", &hidden).is_ok());
    }

    #[test]
    fn byte_mismatch_reports_first_line_within_200_chars() {
        let hidden = HiddenChecks {
            row_count: 1,
            columns: vec!["id".into()],
            must_not_contain: vec![],
        };
        let (class, summary) = score_output(b"id\nA-001\n", b"id\nA-002\n", &hidden).unwrap_err();
        assert_eq!(class, FailureClass::WrongOutput);
        assert!(summary.contains("line 2"), "{summary}");
        assert!(summary.chars().count() <= 201, "{summary}");
    }
}
