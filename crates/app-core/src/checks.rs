//! Host-owned checks for a candidate mapping (plan §7.4). The model never sees or
//! writes these; they run in trusted code against the user's selected input and
//! negative cases synthesized from the contract, and a candidate is only "ready"
//! when every check passes. Agent-authored tests cannot certify a tool.

use crate::drafts;
use crate::dto::{CheckResult, Comparison};
use macroscope_domain::{CsvMapping, DateFormat, TaskContract, Transform};
use macroscope_executor::{apply_mapping, require_columns, validate_mapping, Limits, MappingError};
use sha2::{Digest, Sha256};

pub struct CheckOutcome {
    pub checks: Vec<CheckResult>,
    pub comparison: Option<Comparison>,
}

impl CheckOutcome {
    pub fn all_passed(&self) -> bool {
        !self.checks.is_empty() && self.checks.iter().all(|c| c.passed)
    }
}

fn check(id: &str, label: &str, passed: bool, detail: impl Into<Option<String>>) -> CheckResult {
    CheckResult {
        id: id.into(),
        label: label.into(),
        passed,
        detail: detail.into(),
    }
}

fn sample_value(transform: Option<&Transform>) -> String {
    match transform {
        Some(Transform::Date { from, .. }) => match from {
            DateFormat::MmDdYy => "01/15/26".into(),
            DateFormat::MmDdYyyy => "01/15/2026".into(),
            DateFormat::DdMmYy => "15/01/26".into(),
            DateFormat::DdMmYyyy => "15/01/2026".into(),
            DateFormat::YyyyMmDd | DateFormat::Iso8601 => "2026-01-15".into(),
        },
        Some(Transform::Money { .. }) => "12.50".into(),
        _ => "sample".into(),
    }
}

fn csv_line(cells: &[String]) -> String {
    let mut w = csv::WriterBuilder::new()
        .terminator(csv::Terminator::Any(b'\n'))
        .from_writer(vec![]);
    let _ = w.write_record(cells);
    String::from_utf8(w.into_inner().unwrap_or_default()).unwrap_or_default()
}

/// One synthetic valid row for the contract's input columns.
fn synthetic_row(contract: &TaskContract) -> Vec<String> {
    contract
        .input_schema
        .iter()
        .map(|col| {
            let t = contract
                .mapping
                .output_columns
                .iter()
                .find(|c| &c.source == col)
                .map(|c| &c.transform);
            sample_value(t)
        })
        .collect()
}

fn with_override(contract: &TaskContract, column: &str, value: &str) -> String {
    let row: Vec<String> = contract
        .input_schema
        .iter()
        .zip(synthetic_row(contract))
        .map(|(c, v)| if c == column { value.to_string() } else { v })
        .collect();
    csv_line(&contract.input_schema) + &csv_line(&row)
}

fn rows_of(bytes: &[u8], limit: usize) -> (Vec<String>, Vec<Vec<String>>) {
    match drafts::parse(bytes, limit) {
        Ok(p) => (p.columns, p.rows),
        Err(_) => (vec![], vec![]),
    }
}

pub fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// At most this many characters of any one line appear in an example-check
/// failure detail. Shapes (`#` for numbers) carry the wording and layout
/// without raw fetched values or input rows.
pub(crate) const EXAMPLE_DETAIL_MAX_CHARS: usize = 200;

/// Shortens a normalized line for failure details: at most 200 characters.
fn truncate_detail_line(line: &str) -> String {
    line.chars().take(EXAMPLE_DETAIL_MAX_CHARS).collect()
}

/// Smallest edit distance between two normalized lines, in characters.
fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr = vec![0; b.len() + 1];
    for i in 1..=a.len() {
        curr[0] = i;
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b.len()]
}

/// The output line closest to a missing example line (both normalized).
/// Empty output yields an empty closest line.
fn closest_output_line(missing_norm: &str, actual_norms: &[String]) -> String {
    let mut best: &str = "";
    let mut best_dist: Option<usize> = None;
    for line in actual_norms {
        let dist = edit_distance(missing_norm, line);
        if best_dist.is_none_or(|best| dist < best) {
            best_dist = Some(dist);
            best = line;
        }
    }
    best.to_string()
}

/// Failure detail for a missing example line. Both lines are normalized
/// shapes (numbers are `#` in shape mode and normalized here as well), each
/// truncated to 200 characters.
fn example_failure_detail(expected_norm: &str, closest_norm: &str) -> String {
    format!(
        "Expected a line like \"{}\" — closest output line \"{}\".",
        truncate_detail_line(expected_norm),
        truncate_detail_line(closest_norm)
    )
}

/// Shape-mode line match: equal after normalization, or the example line is a
/// bare label ending in `:` (e.g. `High:`) and the output line is that label
/// followed by a value.
fn shape_line_matches(expected: &str, actual: &str) -> bool {
    expected == actual
        || (expected.ends_with(':')
            && actual
                .strip_prefix(expected)
                .is_some_and(|rest| rest.starts_with(' ')))
        || template_line_matches(expected, actual)
}

/// Splits `line` on `{placeholder}` spans, returning the literal pieces
/// around them, or `None` when the line has no placeholder.
fn template_literals(line: &str) -> Option<Vec<&str>> {
    let mut parts = Vec::new();
    let mut rest = line;
    while let Some(open) = rest.find('{') {
        let Some(len) = rest[open..].find('}') else {
            break;
        };
        parts.push(&rest[..open]);
        rest = &rest[open + len + 1..];
    }
    if parts.is_empty() {
        return None;
    }
    parts.push(rest);
    Some(parts)
}

/// Words an example line may use in place of a value (`Hottest name, temp`).
const PLACEHOLDER_WORDS: &[&str] = &[
    "name",
    "city",
    "place",
    "temp",
    "temperature",
    "value",
    "date",
    "time",
];

/// Rewrites bare placeholder words outside braces as `{}` so they match like
/// `{name}` does. Matching is case-insensitive and whole-word only.
fn expand_placeholder_words(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut word = String::new();
    let mut in_braces = false;
    let flush = |word: &mut String, out: &mut String, in_braces: bool| {
        if !in_braces && PLACEHOLDER_WORDS.contains(&word.to_lowercase().as_str()) {
            out.push_str("{}");
        } else {
            out.push_str(word);
        }
        word.clear();
    };
    for c in line.chars() {
        if c.is_alphabetic() {
            word.push(c);
            continue;
        }
        flush(&mut word, &mut out, in_braces);
        match c {
            '{' => in_braces = true,
            '}' => in_braces = false,
            _ => {}
        }
        out.push(c);
    }
    flush(&mut word, &mut out, in_braces);
    out
}

/// Template match: an example line with `{name}`-style placeholders matches
/// an output line whose text around them is identical and where each
/// placeholder stands for at least one character (it may span several words,
/// e.g. `{temp}` is `18.5 °C`).
fn template_line_matches(expected: &str, actual: &str) -> bool {
    let expanded = expand_placeholder_words(expected);
    let Some(parts) = template_literals(&expanded) else {
        return false;
    };
    // Without any fixed text left, any line would match. A list marker such as "#." or "-"
    // counts: "1. {first}" must match "1. A headline".
    if !parts.iter().any(|p| p.chars().any(|c| !c.is_whitespace())) {
        return false;
    }
    let (first, last) = (parts[0], parts[parts.len() - 1]);
    let Some(mut rest) = actual.strip_prefix(first) else {
        return false;
    };
    for lit in &parts[1..parts.len() - 1] {
        // A placeholder needs a character before the next literal.
        let mut chars = rest.chars();
        if chars.next().is_none() {
            return false;
        }
        let skip = rest.len() - chars.as_str().len();
        match rest[skip..].find(lit) {
            Some(at) => rest = &rest[skip + at + lit.len()..],
            None => return false,
        }
    }
    rest.len() > last.len() && rest.ends_with(last)
}

/// Normalizes one line to its shape for live-data example checks: every
/// number (integers, decimals, optional leading sign, thousands separators,
/// e.g. `-3`, `19.7`, `1,240.50`) becomes `#`, then whitespace runs collapse
/// and the line is trimmed.
fn normalize_shape_line(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::with_capacity(line.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let sign_starts_number =
            (c == '+' || c == '-') && i + 1 < chars.len() && chars[i + 1].is_ascii_digit();
        if c.is_ascii_digit() || sign_starts_number {
            if sign_starts_number {
                i += 1;
            }
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            loop {
                if i < chars.len()
                    && chars[i] == ','
                    && i + 1 < chars.len()
                    && chars[i + 1].is_ascii_digit()
                {
                    i += 1;
                    while i < chars.len() && chars[i].is_ascii_digit() {
                        i += 1;
                    }
                } else if i < chars.len()
                    && chars[i] == '.'
                    && i + 1 < chars.len()
                    && chars[i + 1].is_ascii_digit()
                {
                    i += 1;
                    while i < chars.len() && chars[i].is_ascii_digit() {
                        i += 1;
                    }
                    break;
                } else {
                    break;
                }
            }
            out.push('#');
        } else {
            out.push(c);
            i += 1;
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Runs every check. `read_input` re-reads the selected file so the source-unchanged
/// check compares digests from before and after the candidate ran.
pub fn run(
    contract: &TaskContract,
    candidate: &CsvMapping,
    read_input: &dyn Fn() -> Result<Vec<u8>, String>,
) -> CheckOutcome {
    let mut checks = Vec::new();
    if let Err(problems) = validate_mapping(candidate, contract) {
        checks.push(check(
            "contract",
            "Matches the task summary",
            false,
            problems.join("; "),
        ));
        return CheckOutcome {
            checks,
            comparison: None,
        };
    }
    // The finite mapping encodes confirmed business rules, not just a schema.
    // Successful parsing and preserved counts cannot establish their meaning.
    if candidate != &contract.mapping {
        checks.push(check(
            "confirmed_rules",
            "Preserves every confirmed mapping rule",
            false,
            Some(
                "The candidate changed the confirmed mapping. Edit the task and build again."
                    .into(),
            ),
        ));
    }
    let before = match read_input() {
        Ok(b) => b,
        Err(e) => {
            checks.push(check("input", "Selected CSV is readable", false, e));
            return CheckOutcome {
                checks,
                comparison: None,
            };
        }
    };
    let digest_before = digest(&before);
    let limits = Limits::default();
    let mut comparison = None;
    match require_columns(&before, candidate.skip_header_rows, &contract.input_schema)
        .and_then(|_| apply_mapping(candidate, &before, &limits))
    {
        Ok(out) => {
            let preserved = out.rows_in == out.rows_out;
            checks.push(check(
                "row_count",
                "Row count preserved",
                preserved,
                format!("{} → {}", out.rows_in, out.rows_out),
            ));
            let (input_columns, input_rows) = rows_of(&before, drafts::PREVIEW_ROWS);
            let (output_columns, output_rows) = rows_of(&out.bytes, drafts::PREVIEW_ROWS);
            comparison = Some(Comparison {
                input_text: None,
                output_text: None,
                input_columns,
                input_rows,
                output_columns,
                output_rows,
                rows_in: out.rows_in,
                rows_out: out.rows_out,
            });
        }
        Err(e) => {
            let detail = match &e {
                MappingError::Row {
                    row_number,
                    column,
                    reason,
                    ..
                } => {
                    format!("Row {row_number}, column {column}: {reason}")
                }
                other => other.to_string(),
            };
            checks.push(check(
                "selected_input",
                "Your CSV converts without errors",
                false,
                detail,
            ));
        }
    }

    let has =
        |pred: fn(&Transform) -> bool| candidate.output_columns.iter().find(|c| pred(&c.transform));
    if let Some(date_col) = has(|t| matches!(t, Transform::Date { .. })) {
        let bad = with_override(contract, &date_col.source, "13/45/26");
        let stopped = matches!(
            apply_mapping(candidate, bad.as_bytes(), &limits),
            Err(MappingError::Row { .. })
        );
        checks.push(check(
            "malformed_date",
            "Malformed date stops the run",
            stopped,
            Some("13/45/26 rejected".into()),
        ));
    }
    if let Some(money_col) = has(|t| matches!(t, Transform::Money { .. })) {
        for (id, label, value) in [
            (
                "malformed_amount",
                "Malformed amount stops the run",
                "12.5.0",
            ),
            ("missing_amount", "Missing amount stops the run", ""),
        ] {
            let bad = with_override(contract, &money_col.source, value);
            let stopped = matches!(
                apply_mapping(candidate, bad.as_bytes(), &limits),
                Err(MappingError::Row { .. })
            );
            checks.push(check(id, label, stopped, None));
        }
    }
    if let Some(first_source) = contract.input_schema.first().cloned() {
        let kept: Vec<String> = contract
            .input_schema
            .iter()
            .filter(|c| **c != first_source)
            .cloned()
            .collect();
        let row: Vec<String> = contract
            .input_schema
            .iter()
            .zip(synthetic_row(contract))
            .filter(|(c, _)| **c != first_source)
            .map(|(_, v)| v)
            .collect();
        let missing = csv_line(&kept) + &csv_line(&row);
        let stopped = matches!(
            require_columns(
                missing.as_bytes(),
                candidate.skip_header_rows,
                &contract.input_schema
            ),
            Err(MappingError::MissingColumns(_) | MappingError::Csv(_) | MappingError::EmptyInput)
        );
        checks.push(check(
            "missing_column",
            "Missing column stops the run",
            stopped,
            Some(format!("without {first_source}")),
        ));
    }
    let header_only = csv_line(&contract.input_schema);
    let stopped = matches!(
        apply_mapping(candidate, header_only.as_bytes(), &limits),
        Err(MappingError::EmptyInput)
    );
    checks.push(check(
        "header_only",
        "Header-only file stops the run",
        stopped,
        None,
    ));

    let unchanged = read_input()
        .map(|after| digest(&after) == digest_before)
        .unwrap_or(false);
    checks.push(check(
        "source_unchanged",
        "Source unchanged",
        unchanged,
        Some("Selected input is read only".into()),
    ));
    CheckOutcome { checks, comparison }
}

pub fn program_failure(message: String) -> CheckOutcome {
    CheckOutcome {
        checks: vec![check(
            "sandbox",
            "Runs inside sandbox limits without error",
            false,
            message,
        )],
        comparison: None,
    }
}

#[cfg(feature = "generated-runtime")]
fn program_execution_failure(
    program: &macroscope_builder::Program,
    message: String,
) -> CheckOutcome {
    // RunnerEngine preserves the SDK's host-owned failure, even if user code
    // catches the JavaScript exception. Arbitrary thrown errors cannot spoof it.
    let prefix = "generated runner failed: forbidden: unknown source name";
    let unknown = message.strip_prefix(prefix);
    if let Some(suffix) = unknown {
        let names: Vec<&str> = program
            .web_sources
            .iter()
            .map(|s| s.name.as_str())
            .chain(program.connection_sources.iter().map(|s| s.name.as_str()))
            .collect();
        let name = suffix
            .strip_prefix(": ")
            .and_then(|encoded| serde_json::from_str::<String>(encoded).ok());
        if name
            .as_ref()
            .is_none_or(|name| !names.contains(&name.as_str()))
        {
            let read = name.map_or_else(
                || "an undeclared source".into(),
                |n| format!("source '{n}'"),
            );
            return CheckOutcome {
                checks: vec![check(
                    "source_names",
                    "Every source read is declared",
                    false,
                    format!(
                        "The program reads {read} but declares only: {}",
                        if names.is_empty() {
                            "(none)".into()
                        } else {
                            names.join(", ")
                        }
                    ),
                )],
                comparison: None,
            };
        }
    }
    program_failure(message)
}

/// Host-owned program checks against the same bounded source snapshot, in two
/// separate fresh worker processes. No candidate-authored tests certify a tool.
#[cfg(feature = "generated-runtime")]
pub type ProgramExecutor<'a> = dyn Fn(
        &[u8],
        &str,
    ) -> Result<
        (
            Vec<macroscope_runner::Output>,
            Vec<macroscope_domain::PlannedCall>,
        ),
        String,
    > + 'a;

/// Executor variant that also passes host-fetched web source bodies as data.
/// The same fetched map is used for both runs, proving determinism on the
/// same fetched data.
#[cfg(feature = "generated-runtime")]
pub type ProgramExecutorWithSources<'a> = dyn Fn(
        &[u8],
        &std::collections::BTreeMap<String, String>,
        &str,
    ) -> Result<
        (
            Vec<macroscope_runner::Output>,
            Vec<macroscope_domain::PlannedCall>,
        ),
        String,
    > + 'a;

#[cfg(feature = "generated-runtime")]
pub fn run_program(
    draft: &crate::dto::TaskDraft,
    program: &macroscope_builder::Program,
    read_input: &dyn Fn() -> Result<Vec<u8>, String>,
    execute: &ProgramExecutor<'_>,
) -> CheckOutcome {
    run_program_with_sources(
        draft,
        program,
        &std::collections::BTreeMap::new(),
        None,
        read_input,
        &|input, _sources, code| execute(input, code),
    )
}

/// Program checks for tools with web sources. `sources` holds the host-fetched
/// response texts; `fetch_error` carries a host fetch failure, which fails the
/// `web_sources` check instead of running the program on missing data. With no
/// input file the empty snapshot is used, `macroscope.input` is unavailable,
/// and the source-unchanged check is skipped.
#[cfg(feature = "generated-runtime")]
pub fn run_program_with_sources(
    draft: &crate::dto::TaskDraft,
    program: &macroscope_builder::Program,
    sources: &std::collections::BTreeMap<String, String>,
    fetch_error: Option<String>,
    read_input: &dyn Fn() -> Result<Vec<u8>, String>,
    execute: &ProgramExecutorWithSources<'_>,
) -> CheckOutcome {
    let no_input = draft.no_input;
    let before = if no_input {
        Vec::new()
    } else {
        match read_input() {
            Ok(b) => b,
            Err(e) => return program_failure(e),
        }
    };
    let mut checks = Vec::new();
    if !program.web_sources.is_empty() || fetch_error.is_some() {
        let missing: Vec<&str> = program
            .web_sources
            .iter()
            .filter(|s| !sources.contains_key(&s.name))
            .map(|s| s.name.as_str())
            .collect();
        let passed = fetch_error.is_none() && missing.is_empty();
        let detail = fetch_error.or_else(|| {
            (!missing.is_empty()).then(|| format!("No fetched data for: {}.", missing.join(", ")))
        });
        checks.push(check(
            "web_sources",
            "Web sources fetched successfully",
            passed,
            detail,
        ));
        if !passed {
            return CheckOutcome {
                checks,
                comparison: None,
            };
        }
    }
    let (first, first_calls) = match execute(&before, sources, &program.code) {
        Ok(out) => out,
        Err(e) => return program_execution_failure(program, e),
    };
    let (second, second_calls) = match execute(&before, sources, &program.code) {
        Ok(out) => out,
        Err(e) => return program_execution_failure(program, e),
    };
    checks.push(check(
        "sandbox",
        "Runs inside sandbox limits without error",
        true,
        None,
    ));
    let calls_valid = crate::connections::validate_calls(&first_calls, &draft.required_connections);
    checks.push(check(
        "mcp_capabilities",
        "Every planned connection call is declared and validates against its input schema",
        calls_valid.is_ok(),
        calls_valid.err(),
    ));
    checks.push(check(
        "mcp_deterministic",
        "Two fresh runs plan exactly the same connection calls",
        first_calls == second_calls,
        None,
    ));
    let named = program.output_name == draft.output_name
        && first.len() <= 1
        && first.len() == second.len()
        && first.first().is_none_or(|o| o.name == draft.output_name)
        && second.first().is_none_or(|o| o.name == draft.output_name)
        && (!first.is_empty() || !first_calls.is_empty());
    checks.push(check(
        "one_output",
        "Plans connection actions or creates one output with the confirmed name",
        named,
        None,
    ));
    if !named {
        return CheckOutcome {
            checks,
            comparison: None,
        };
    }
    let output = first.first().map_or("", |o| o.text.as_str());
    let deterministic = output == second.first().map_or("", |o| o.text.as_str());
    checks.push(check(
        "deterministic",
        "Two fresh runs produce identical bytes",
        deterministic,
        None,
    ));
    let valid = if first.is_empty() && !first_calls.is_empty() {
        Ok(())
    } else {
        macroscope_effect_broker::validate_text_output(&draft.output_name, output.as_bytes())
    };
    checks.push(check(
        "output_format",
        "Any file output is nonempty UTF-8 and parses in its selected format",
        valid.is_ok(),
        valid.err().map(|e| e.to_string()),
    ));
    if no_input || !program.connection_sources.is_empty() || !program.web_sources.is_empty() {
        let empty = empty_labels(output);
        checks.push(check(
            "values_found",
            "The result has its values, not empty labels",
            empty.is_none(),
            empty.map(|label| format!("Most labelled values in the result are empty (for example \"{label}\"): the data that was read does not contain them. Read the full content (for an email search set includeBody to true so its text is returned) and extract the values, or throw an Error naming what is missing; never output empty values")),
        ));
    }
    if !program.web_sources.is_empty() {
        let markup = page_markup(output);
        checks.push(check(
            "clean_text",
            "The result is plain text, not page code",
            markup.is_none(),
            markup.map(|m| format!("The result contains page code (\"{m}\"), so the program copied raw HTML or wiki source instead of the text. Extract the readable value: use macroscope.html.text or macroscope.html.tables on HTML, and for wiki source take the cell text, not its markup")),
        ));
    }
    if let Some(example) = &draft.example_output {
        // Live-data tasks (web sources) and no-input tasks produce numbers
        // that legitimately differ from the pasted example, so compare line
        // shapes: every number becomes a placeholder and whitespace runs
        // collapse. File-input tasks stay exact (deterministic data).
        let shape_mode = !program.web_sources.is_empty() || no_input;
        if shape_mode {
            let expected: Vec<String> = example.lines().map(normalize_shape_line).collect();
            let actual: Vec<String> = output.lines().map(normalize_shape_line).collect();
            let mut need = 0;
            for line in &actual {
                if need < expected.len() && shape_line_matches(&expected[need], line) {
                    need += 1;
                }
            }
            let matches = need == expected.len();
            checks.push(check(
                "example",
                "Output follows the example's format",
                matches,
                if matches {
                    None
                } else {
                    let missing = expected.get(need).cloned().unwrap_or_default();
                    let closest = closest_output_line(&missing, &actual);
                    Some(example_failure_detail(&missing, &closest))
                },
            ));
        } else {
            let expected_raw: Vec<&str> = example.lines().collect();
            let actual_raw: Vec<&str> = output.lines().collect();
            let mut pos = 0;
            let mut missing_idx: Option<usize> = None;
            for (idx, expected_line) in expected_raw.iter().enumerate() {
                let mut found = false;
                for (j, actual_line) in actual_raw.iter().enumerate().skip(pos) {
                    if *actual_line == *expected_line {
                        pos = j + 1;
                        found = true;
                        break;
                    }
                }
                if !found {
                    missing_idx = Some(idx);
                    break;
                }
            }
            let matches = missing_idx.is_none();
            checks.push(check(
                "example",
                "Every example line appears in order",
                matches,
                if matches {
                    None
                } else {
                    let missing_raw = missing_idx
                        .and_then(|idx| expected_raw.get(idx))
                        .copied()
                        .unwrap_or_default();
                    let missing_norm = normalize_shape_line(missing_raw);
                    let actual_norms: Vec<String> = actual_raw
                        .iter()
                        .map(|line| normalize_shape_line(line))
                        .collect();
                    let closest = closest_output_line(&missing_norm, &actual_norms);
                    Some(example_failure_detail(&missing_norm, &closest))
                },
            ));
        }
    }
    // With no input file there is no source snapshot to re-read; the run plan
    // hash covers the source URLs and fetched-response digests instead.
    if !no_input {
        checks.push(check(
            "source_unchanged",
            "Source unchanged",
            read_input().is_ok_and(|b| digest(&b) == digest(&before)),
            None,
        ));
    }
    let input = std::str::from_utf8(&before).unwrap_or_default();
    CheckOutcome {
        checks,
        comparison: Some(Comparison {
            input_text: Some(drafts::text_preview(input)),
            output_text: Some(drafts::text_preview(output)),
            input_columns: vec![],
            input_rows: vec![],
            output_columns: vec![],
            output_rows: vec![],
            rows_in: input.lines().count() as u64,
            rows_out: output.lines().count() as u64,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contract() -> TaskContract {
        serde_json::from_str(include_str!("../../../fixtures/orders/task.json")).unwrap()
    }

    #[test]
    fn candidates_cannot_replace_confirmed_rules_even_when_rows_still_look_valid() {
        let c = contract();
        let input = b"order_id,created,amount,customer\nA,03/04/26,1.25,X\n";
        let mut swapped = c.mapping.clone();
        swapped.output_columns[1].transform = Transform::Date {
            from: DateFormat::DdMmYy,
            to: DateFormat::YyyyMmDd,
        };
        let mut copied = c.mapping.clone();
        copied.output_columns[1].transform = Transform::Copy;
        let mut constant = c.mapping.clone();
        constant.output_columns[0].transform = Transform::Constant {
            value: "wrong".into(),
        };
        for candidate in [swapped, copied, constant] {
            assert!(!run(&c, &candidate, &|| Ok(input.to_vec())).all_passed());
        }
    }

    #[test]
    fn confirmed_constant_outputs_and_single_column_inputs_can_pass_checks() {
        let mut c = contract();
        c.input_schema = vec!["id".into()];
        c.output_columns = vec!["ID".into()];
        c.mapping.output_columns = vec![macroscope_domain::OutputColumn {
            name: "ID".into(),
            source: "id".into(),
            transform: Transform::Copy,
        }];
        for transform in [
            Transform::Copy,
            Transform::Constant {
                value: "fixed".into(),
            },
        ] {
            c.mapping.output_columns[0].transform = transform;
            let outcome = run(&c, &c.mapping, &|| Ok(b"id\n1\n".to_vec()));
            assert!(outcome.all_passed(), "{:?}", outcome.checks);
        }
    }

    #[test]
    fn fixture_passes_every_check_and_compares_first_rows() {
        let c = contract();
        let input = include_bytes!("../../../fixtures/orders/input.csv").to_vec();
        let outcome = run(&c, &c.mapping, &|| Ok(input.clone()));
        assert!(outcome.all_passed(), "{:#?}", outcome.checks);
        let ids: Vec<_> = outcome.checks.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "row_count",
                "malformed_date",
                "malformed_amount",
                "missing_amount",
                "missing_column",
                "header_only",
                "source_unchanged"
            ]
        );
        let cmp = outcome.comparison.unwrap();
        assert_eq!(cmp.output_columns, ["Order", "Date", "Amount"]);
        assert_eq!(cmp.output_rows[0], ["ORD-001", "2026-01-05", "1240.00"]);
        assert_eq!(cmp.rows_in, 18);
    }

    #[test]
    fn wrong_date_meaning_fails_on_the_users_own_data() {
        let c = contract();
        let mut wrong = c.mapping.clone();
        wrong.output_columns[1].transform = Transform::Date {
            from: DateFormat::DdMmYy,
            to: DateFormat::YyyyMmDd,
        };
        let input = include_bytes!("../../../fixtures/orders/input.csv").to_vec();
        let mut c2 = c.clone();
        c2.mapping = wrong.clone();
        let outcome = run(&c2, &wrong, &|| Ok(input.clone()));
        assert!(!outcome.all_passed());
        assert!(outcome
            .checks
            .iter()
            .any(|k| k.id == "selected_input" && !k.passed));
    }

    #[test]
    fn a_changing_source_fails_the_unchanged_check() {
        let c = contract();
        let input = include_bytes!("../../../fixtures/orders/input.csv").to_vec();
        let calls = std::cell::Cell::new(0);
        let outcome = run(&c, &c.mapping, &|| {
            calls.set(calls.get() + 1);
            let mut b = input.clone();
            if calls.get() > 1 {
                b.extend_from_slice(b"ORD-999,01/01/26,$1.00,Late Row\n");
            }
            Ok(b)
        });
        assert!(outcome
            .checks
            .iter()
            .any(|k| k.id == "source_unchanged" && !k.passed));
    }

    #[test]
    fn candidate_outside_the_contract_is_rejected_before_running() {
        let c = contract();
        let mut bad = c.mapping.clone();
        bad.output_columns[0].source = "secret_column".into();
        let outcome = run(&c, &bad, &|| Ok(b"order_id\n1\n".to_vec()));
        assert_eq!(outcome.checks.len(), 1);
        assert_eq!(outcome.checks[0].id, "contract");
        assert!(!outcome.all_passed());
    }

    #[cfg(feature = "generated-runtime")]
    fn web_draft() -> crate::dto::TaskDraft {
        crate::dto::TaskDraft {
            access_plan: None,
            replaces_tool_id: None,
            kind: crate::dto::TaskKind::Program,
            example_output: None,
            suggested_name: None,
            synthetic_shape: String::new(),
            no_input: true,
            draft_id: "draft".into(),
            revision: 1,
            goal: "weather".into(),
            input: crate::dto::CsvPreview {
                text: None,
                handle: crate::dto::FileHandle {
                    handle_id: "no-input".into(),
                    display_name: "No input file".into(),
                    display_path: String::new(),
                },
                columns: vec![],
                rows: vec![],
                row_count: 0,
                byte_size: 0,
            },
            summary: vec![],
            mapping: contract().mapping,
            questions: vec![],
            mentioned_connectors: vec![],
            required_connections: vec![],
            output_name: "weather.txt".into(),
            ready_to_build: true,
            blockers: vec![],
        }
    }

    /// No-input program checks without any network or worker: the fake
    /// executor stands in for the host fetch plus sandbox execution.
    #[cfg(feature = "generated-runtime")]
    #[test]
    fn no_input_checks_serve_declared_sources_and_skip_source_unchanged() {
        use std::collections::BTreeMap;
        let draft = web_draft();
        let program = macroscope_builder::Program {
            connection_sources: vec![],
            model_step: None,
            summary: "weather".into(),
            output_name: "weather.txt".into(),
            code: "source".into(),
            web_sources: vec![macroscope_builder::WebSource {
                name: "forecast".into(),
                url: "https://api.open-meteo.com/v1".into(),
            }],
        };
        // The executor derives its output from the fetched map, proving the
        // program ran on the fetched data; both runs see the same map.
        let execute = |_: &[u8], sources: &BTreeMap<String, String>, _: &str| {
            let body = sources.get("forecast").cloned().unwrap_or_default();
            Ok((
                vec![macroscope_runner::Output {
                    name: "weather.txt".into(),
                    text: format!("got {} bytes", body.len()),
                }],
                vec![],
            ))
        };
        let sources = BTreeMap::from([("forecast".to_string(), "{\"t\":21}".to_string())]);
        let outcome = super::run_program_with_sources(
            &draft,
            &program,
            &sources,
            None,
            &|| panic!("no input file must not be re-read"),
            &execute,
        );
        assert!(outcome.all_passed(), "{:#?}", outcome.checks);
        let ids: Vec<_> = outcome.checks.iter().map(|c| c.id.as_str()).collect();
        assert!(ids.contains(&"web_sources"));
        assert!(!ids.contains(&"source_unchanged"));
        // Missing fetched data fails the web_sources check before any run.
        let calls = std::cell::Cell::new(0);
        let counting = |_: &[u8], _: &BTreeMap<String, String>, _: &str| {
            calls.set(calls.get() + 1);
            Ok((
                vec![macroscope_runner::Output {
                    name: "weather.txt".into(),
                    text: "x".into(),
                }],
                vec![],
            ))
        };
        let outcome = super::run_program_with_sources(
            &draft,
            &program,
            &BTreeMap::new(),
            Some("Web source 'forecast' could not be fetched: down.".into()),
            &|| panic!("no input file must not be re-read"),
            &counting,
        );
        assert!(!outcome.all_passed());
        assert_eq!(calls.get(), 0);
        assert!(outcome
            .checks
            .iter()
            .any(|c| c.id == "web_sources" && !c.passed));
    }

    /// `{name} {temp}` placeholders in the example stand for any value, even
    /// one spanning several words such as `18.5 °C`.
    #[test]
    fn template_placeholders_match_multi_word_values() {
        let m = |e: &str, a: &str| {
            super::shape_line_matches(
                &super::normalize_shape_line(e),
                &super::normalize_shape_line(a),
            )
        };
        assert!(m(
            "Hottest: {name} {temp}",
            "Hottest: Auckland 21.4 \u{b0}C"
        ));
        assert!(m(
            "Hottest: {name} {temp}",
            "Hottest: Palmerston North 9 \u{b0}C"
        ));
        assert!(!m(
            "Hottest: {name} {temp}",
            "Coldest: Auckland 21.4 \u{b0}C"
        ));
        assert!(!m("Hottest: {name} {temp}", "Hottest: Auckland"));
        assert!(!m("Hottest: {name}", "Hottest: "));
        // Bare placeholder words work too, whole-word and case-insensitive.
        assert!(m("Hottest name, temp", "Hottest Auckland, 18.5\u{b0}C"));
        assert!(m("Hottest Name, Temp", "Hottest Auckland, 18.5\u{b0}C"));
        assert!(!m("Hottest name, temp", "Coldest Auckland, 18.5\u{b0}C"));
        assert!(!m("Hottest names", "Hottest Auckland"));
        assert!(!m("name temp", "Auckland 18"));
    }

    /// A bare label line (`High:`) in the example means "label, then a value".
    #[cfg(feature = "generated-runtime")]
    #[test]
    fn web_source_example_bare_label_accepts_a_value() {
        use std::collections::BTreeMap;
        let mut draft = web_draft();
        draft.example_output = Some("High:\nLow:".into());
        let program = macroscope_builder::Program {
            connection_sources: vec![],
            model_step: None,
            summary: "weather".into(),
            output_name: "weather.txt".into(),
            code: "source".into(),
            web_sources: vec![macroscope_builder::WebSource {
                name: "forecast".into(),
                url: "https://api.open-meteo.com/v1".into(),
            }],
        };
        let sources = BTreeMap::from([("forecast".to_string(), "{\"t\":21}".to_string())]);
        let check = |text: &str| {
            let text = text.to_string();
            let execute = move |_: &[u8], _: &BTreeMap<String, String>, _: &str| {
                Ok((
                    vec![macroscope_runner::Output {
                        name: "weather.txt".into(),
                        text: text.clone(),
                    }],
                    vec![],
                ))
            };
            super::run_program_with_sources(
                &draft,
                &program,
                &sources,
                None,
                &|| panic!("no input file must not be re-read"),
                &execute,
            )
            .checks
            .into_iter()
            .find(|c| c.id == "example")
            .expect("example check")
            .passed
        };
        assert!(check("High: 20.5\u{b0}C\nLow: 12"));
        assert!(!check("Highest: 20\nLow: 12"));
        assert!(!check("Low: 12\nHigh: 20"));
    }

    /// Live-data example checks compare line shapes so differing weather
    /// numbers still pass when the wording and layout match.
    #[cfg(feature = "generated-runtime")]
    #[test]
    fn web_source_example_check_passes_with_different_numbers() {
        use std::collections::BTreeMap;
        let mut draft = web_draft();
        draft.example_output =
            Some("Gisborne high : 20, low : 12\nRain 1,240.50 mm, wind -3 m/s".into());
        let program = macroscope_builder::Program {
            connection_sources: vec![],
            model_step: None,
            summary: "weather".into(),
            output_name: "weather.txt".into(),
            code: "source".into(),
            web_sources: vec![macroscope_builder::WebSource {
                name: "forecast".into(),
                url: "https://api.open-meteo.com/v1".into(),
            }],
        };
        let execute = |_: &[u8], _: &BTreeMap<String, String>, _: &str| {
            Ok((
                vec![macroscope_runner::Output {
                    name: "weather.txt".into(),
                    text: "Gisborne   high : 19.7,  low : 8\nRain 987.25 mm, wind +4 m/s".into(),
                }],
                vec![],
            ))
        };
        let sources = BTreeMap::from([("forecast".to_string(), "{\"t\":21}".to_string())]);
        let outcome = super::run_program_with_sources(
            &draft,
            &program,
            &sources,
            None,
            &|| panic!("no input file must not be re-read"),
            &execute,
        );
        let example = outcome
            .checks
            .iter()
            .find(|c| c.id == "example")
            .expect("example check");
        assert!(example.passed, "{:#?}", outcome.checks);
        assert_eq!(example.label, "Output follows the example's format");
    }

    #[cfg(feature = "generated-runtime")]
    #[test]
    fn web_source_example_check_rejects_wrong_wording() {
        use std::collections::BTreeMap;
        let mut draft = web_draft();
        draft.example_output = Some("Gisborne high : 20, low : 12".into());
        let program = macroscope_builder::Program {
            connection_sources: vec![],
            model_step: None,
            summary: "weather".into(),
            output_name: "weather.txt".into(),
            code: "source".into(),
            web_sources: vec![macroscope_builder::WebSource {
                name: "forecast".into(),
                url: "https://api.open-meteo.com/v1".into(),
            }],
        };
        let execute = |_: &[u8], _: &BTreeMap<String, String>, _: &str| {
            Ok((
                vec![macroscope_runner::Output {
                    name: "weather.txt".into(),
                    text: "Gisborne low : 20, high : 12".into(),
                }],
                vec![],
            ))
        };
        let sources = BTreeMap::from([("forecast".to_string(), "{\"t\":21}".to_string())]);
        let outcome = super::run_program_with_sources(
            &draft,
            &program,
            &sources,
            None,
            &|| panic!("no input file must not be re-read"),
            &execute,
        );
        let example = outcome
            .checks
            .iter()
            .find(|c| c.id == "example")
            .expect("example check");
        assert!(!example.passed);
        assert_eq!(example.label, "Output follows the example's format");
        let detail = example.detail.as_deref().expect("detail names the diff");
        assert!(
            detail.contains("Gisborne high : #, low : #"),
            "missing expected shape in {detail}"
        );
        assert!(
            detail.contains("Gisborne low : #, high : #"),
            "missing closest output shape in {detail}"
        );
        assert!(detail.starts_with("Expected a line like \""), "{detail}");
        assert!(detail.contains("— closest output line \""), "{detail}");
    }

    #[cfg(feature = "generated-runtime")]
    #[test]
    fn web_source_example_check_requires_line_order() {
        use std::collections::BTreeMap;
        let mut draft = web_draft();
        draft.example_output = Some("alpha 1\nbeta 2".into());
        let program = macroscope_builder::Program {
            connection_sources: vec![],
            model_step: None,
            summary: "weather".into(),
            output_name: "weather.txt".into(),
            code: "source".into(),
            web_sources: vec![macroscope_builder::WebSource {
                name: "forecast".into(),
                url: "https://api.open-meteo.com/v1".into(),
            }],
        };
        let sources = BTreeMap::from([("forecast".to_string(), "{\"t\":21}".to_string())]);
        let swapped = |_: &[u8], _: &BTreeMap<String, String>, _: &str| {
            Ok((
                vec![macroscope_runner::Output {
                    name: "weather.txt".into(),
                    text: "beta 9\nalpha 8".into(),
                }],
                vec![],
            ))
        };
        let outcome = super::run_program_with_sources(
            &draft,
            &program,
            &sources,
            None,
            &|| panic!("no input file must not be re-read"),
            &swapped,
        );
        assert!(
            !outcome
                .checks
                .iter()
                .find(|c| c.id == "example")
                .expect("example check")
                .passed
        );
        let ordered = |_: &[u8], _: &BTreeMap<String, String>, _: &str| {
            Ok((
                vec![macroscope_runner::Output {
                    name: "weather.txt".into(),
                    text: "header\nbeta 9\nalpha 8\nbeta 7".into(),
                }],
                vec![],
            ))
        };
        draft.example_output = Some("beta 2\nalpha 3".into());
        let outcome = super::run_program_with_sources(
            &draft,
            &program,
            &sources,
            None,
            &|| panic!("no input file must not be re-read"),
            &ordered,
        );
        assert!(
            outcome
                .checks
                .iter()
                .find(|c| c.id == "example")
                .expect("example check")
                .passed,
            "{:#?}",
            outcome.checks
        );
    }

    /// File-input tasks keep the exact example comparison: different numbers
    /// fail, identical lines pass.
    #[cfg(feature = "generated-runtime")]
    #[test]
    fn file_input_example_check_stays_exact() {
        use std::collections::BTreeMap;
        let mut draft = web_draft();
        draft.no_input = false;
        draft.example_output = Some("Gisborne high : 20, low : 12".into());
        let program = macroscope_builder::Program {
            connection_sources: vec![],
            model_step: None,
            summary: "weather".into(),
            output_name: "weather.txt".into(),
            code: "source".into(),
            web_sources: vec![],
        };
        let read = || Ok(b"selected input".to_vec());
        let different = |_: &[u8], _: &BTreeMap<String, String>, _: &str| {
            Ok((
                vec![macroscope_runner::Output {
                    name: "weather.txt".into(),
                    text: "Gisborne high : 21, low : 12".into(),
                }],
                vec![],
            ))
        };
        let outcome = super::run_program_with_sources(
            &draft,
            &program,
            &BTreeMap::new(),
            None,
            &read,
            &different,
        );
        let example = outcome
            .checks
            .iter()
            .find(|c| c.id == "example")
            .expect("example check");
        assert!(!example.passed);
        assert_eq!(example.label, "Every example line appears in order");
        let exact = |_: &[u8], _: &BTreeMap<String, String>, _: &str| {
            Ok((
                vec![macroscope_runner::Output {
                    name: "weather.txt".into(),
                    text: "Gisborne high : 20, low : 12".into(),
                }],
                vec![],
            ))
        };
        let outcome = super::run_program_with_sources(
            &draft,
            &program,
            &BTreeMap::new(),
            None,
            &read,
            &exact,
        );
        assert!(
            outcome
                .checks
                .iter()
                .find(|c| c.id == "example")
                .expect("example check")
                .passed,
            "{:#?}",
            outcome.checks
        );
    }

    #[cfg(feature = "generated-runtime")]
    #[test]
    fn gisborne_comma_reports_expected_and_closest_shapes() {
        use std::collections::BTreeMap;
        let mut draft = web_draft();
        draft.example_output = Some("Gisborne high : 20, low : 12".into());
        let program = macroscope_builder::Program {
            connection_sources: vec![],
            model_step: None,
            summary: "weather".into(),
            output_name: "weather.txt".into(),
            code: "source".into(),
            web_sources: vec![macroscope_builder::WebSource {
                name: "forecast".into(),
                url: "https://api.open-meteo.com/v1".into(),
            }],
        };
        let execute = |_: &[u8], _: &BTreeMap<String, String>, _: &str| {
            Ok((
                vec![macroscope_runner::Output {
                    name: "weather.txt".into(),
                    text: "Gisborne, high: 19.7, low: 13.5".into(),
                }],
                vec![],
            ))
        };
        let outcome = super::run_program_with_sources(
            &draft,
            &program,
            &BTreeMap::from([("forecast".to_string(), "{\"t\":21}".to_string())]),
            None,
            &|| panic!("no input file must not be re-read"),
            &execute,
        );
        let detail = outcome
            .checks
            .iter()
            .find(|c| c.id == "example")
            .expect("example check")
            .detail
            .clone()
            .expect("detail");
        assert_eq!(
            detail,
            "Expected a line like \"Gisborne high : #, low : #\" — closest output line \"Gisborne, high: #, low: #\"."
        );
    }

    #[cfg(feature = "generated-runtime")]
    #[test]
    fn example_failure_details_show_first_missing_and_closest_in_both_modes() {
        use std::collections::BTreeMap;
        // Shape mode: the first missing line is named, with the closest shape.
        let mut draft = web_draft();
        draft.example_output = Some("alpha 1\nbeta 2\ngamma 3".into());
        let program = macroscope_builder::Program {
            connection_sources: vec![],
            model_step: None,
            summary: "weather".into(),
            output_name: "weather.txt".into(),
            code: "source".into(),
            web_sources: vec![macroscope_builder::WebSource {
                name: "forecast".into(),
                url: "https://api.open-meteo.com/v1".into(),
            }],
        };
        let execute = |_: &[u8], _: &BTreeMap<String, String>, _: &str| {
            Ok((
                vec![macroscope_runner::Output {
                    name: "weather.txt".into(),
                    text: "alpha 9\nbeta tweaked\ngamma 7".into(),
                }],
                vec![],
            ))
        };
        let outcome = super::run_program_with_sources(
            &draft,
            &program,
            &BTreeMap::from([("forecast".to_string(), "{\"t\":21}".to_string())]),
            None,
            &|| panic!("no input file must not be re-read"),
            &execute,
        );
        let detail = outcome
            .checks
            .iter()
            .find(|c| c.id == "example")
            .expect("example check")
            .detail
            .clone()
            .expect("detail");
        assert!(detail.contains("beta #"), "first missing shape in {detail}");
        assert!(
            detail.contains("closest output line"),
            "closest line in {detail}"
        );
        assert!(
            !detail.contains("19.7") && !detail.contains("987"),
            "raw numbers must not leak: {detail}"
        );
        // Exact (file-input) mode also reports normalized shapes.
        let mut file_draft = web_draft();
        file_draft.no_input = false;
        file_draft.example_output = Some("Gisborne high : 20, low : 12".into());
        let file_program = macroscope_builder::Program {
            connection_sources: vec![],
            model_step: None,
            summary: "weather".into(),
            output_name: "weather.txt".into(),
            code: "source".into(),
            web_sources: vec![],
        };
        let read = || Ok(b"selected input".to_vec());
        let wrong = |_: &[u8], _: &BTreeMap<String, String>, _: &str| {
            Ok((
                vec![macroscope_runner::Output {
                    name: "weather.txt".into(),
                    text: "Gisborne, high: 19.7, low: 13.5".into(),
                }],
                vec![],
            ))
        };
        let outcome = super::run_program_with_sources(
            &file_draft,
            &file_program,
            &BTreeMap::new(),
            None,
            &read,
            &wrong,
        );
        let detail = outcome
            .checks
            .iter()
            .find(|c| c.id == "example")
            .expect("example check")
            .detail
            .clone()
            .expect("detail");
        assert!(
            detail.contains("Gisborne high : #, low : #"),
            "normalized expected in {detail}"
        );
        assert!(
            detail.contains("Gisborne, high: #, low: #"),
            "normalized closest in {detail}"
        );
        assert!(
            !detail.contains("19.7") && !detail.contains(" 20"),
            "raw numbers must not leak: {detail}"
        );
    }

    #[cfg(feature = "generated-runtime")]
    #[test]
    fn numbered_placeholder_lines_match_numbered_output() {
        let expected = normalize_shape_line("1. {first}");
        assert!(shape_line_matches(&expected, &normalize_shape_line("1. Laura Kuenssberg: Healey risks breaking a promise")));
        assert!(shape_line_matches(&normalize_shape_line("- {item}"), &normalize_shape_line("- Milk")));
        assert!(!shape_line_matches(&normalize_shape_line("{first}"), &normalize_shape_line("anything at all")));
        assert!(!shape_line_matches(&expected, &normalize_shape_line("Today's top headlines 2026-11-01")));
    }

    #[test]
    fn example_failure_details_never_exceed_200_chars_per_line() {
        use std::collections::BTreeMap;
        let long_expected = format!("word {} end", "x".repeat(500));
        let long_actual = format!("other {} end", "y".repeat(500));
        let mut draft = web_draft();
        draft.example_output = Some(long_expected.clone());
        let program = macroscope_builder::Program {
            connection_sources: vec![],
            model_step: None,
            summary: "weather".into(),
            output_name: "weather.txt".into(),
            code: "source".into(),
            web_sources: vec![macroscope_builder::WebSource {
                name: "forecast".into(),
                url: "https://api.open-meteo.com/v1".into(),
            }],
        };
        let execute = move |_: &[u8], _: &BTreeMap<String, String>, _: &str| {
            Ok((
                vec![macroscope_runner::Output {
                    name: "weather.txt".into(),
                    text: long_actual.clone(),
                }],
                vec![],
            ))
        };
        let outcome = super::run_program_with_sources(
            &draft,
            &program,
            &BTreeMap::from([("forecast".to_string(), "{\"t\":21}".to_string())]),
            None,
            &|| panic!("no input file must not be re-read"),
            &execute,
        );
        let detail = outcome
            .checks
            .iter()
            .find(|c| c.id == "example")
            .expect("example check")
            .detail
            .clone()
            .expect("detail");
        // Extract the two quoted lines and assert the 200-char bound.
        let parts: Vec<&str> = detail.split('"').collect();
        assert!(parts.len() >= 5, "{detail}");
        assert!(parts[1].chars().count() <= 200, "expected truncated");
        assert!(parts[3].chars().count() <= 200, "closest truncated");
        assert_eq!(parts[1].chars().count(), 200);
        assert_eq!(parts[3].chars().count(), 200);
    }
}

/// Page code left in a result read from the web: HTML tags and attributes, wiki templates,
/// links and table cells, or undecoded entities. Returns the first fragment found.
pub(crate) fn page_markup(output: &str) -> Option<String> {
    static MARKUP: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(concat!(
            r#"(?i)\b(?:class|style|scope|href|src|align|data-[a-z-]+)\s*=\s*["']"#,
            r"|</?(?:div|span|td|tr|th|table|tbody|p|a|li|ul|ol|br|img|script|style|h[1-6])\b[^>]*>",
            r"|\{\{|\}\}|\[\[|\]\]|(?m)^\s*(?:\|-|\{\||\|\}|!\s*scope)",
            r"|&(?:nbsp|amp|quot|lt|gt|apos|#\d+|#x[0-9a-f]+);",
        ))
        .unwrap()
    });
    MARKUP.find(output).map(|m| m.as_str().chars().take(40).collect())
}

/// A result whose labelled lines are mostly empty ("Total:", "Due date: N/A") found nothing:
/// returns the first empty label when more than half of at least two labels are empty.
pub(crate) fn empty_labels(output: &str) -> Option<String> {
    let mut labelled = 0;
    let mut empty = Vec::new();
    for line in output.lines().map(str::trim) {
        let Some((label, value)) = line.split_once(':') else { continue };
        let label = label.trim().trim_start_matches(['-', '*', '•']).trim();
        if label.is_empty() || label.chars().count() > 60 || !label.chars().any(char::is_alphabetic) || value.starts_with("//") {
            continue;
        }
        labelled += 1;
        let v = value.trim().trim_matches(['"', '\'']).to_ascii_lowercase();
        if ["", "-", "—", "n/a", "na", "none", "null", "undefined", "unknown", "not found", "not available"].contains(&v.as_str()) {
            empty.push(format!("{label}:"));
        }
    }
    // An answer that is only one empty label ("Featured article tools:") is a heading, not a value.
    let only_a_label = labelled == 1 && empty.len() == 1 && output.trim().lines().count() == 1;
    ((labelled >= 2 && empty.len() * 2 > labelled) || only_a_label).then(|| empty.remove(0))
}

#[cfg(test)]
mod page_markup_tests {
    #[test]
    fn page_code_in_a_result_is_found_and_plain_text_passes() {
        let eclipse = "Date: August 2, 2027\nVisible from: ! scope=\"row\" class=\"nowrap\" | August 2, 2027 | Total";
        assert!(super::page_markup(eclipse).is_some());
        assert!(super::page_markup("Headline: <a href=\"/x\">Storm</a>").is_some());
        assert!(super::page_markup("Visible from: [[Morocco]], [[Spain]]").is_some());
        assert!(super::page_markup("Price: 5 &amp; rising").is_some());
        for plain in ["Silver: $31.20 per ounce", "Top: a < b > c, 3 | 4", "Wind: 25 km/h {gusting}", "Total: 2 & 3"] {
            assert!(super::page_markup(plain).is_none(), "{plain}");
        }
    }
}

#[cfg(test)]
mod empty_label_tests {
    #[test]
    fn mostly_empty_labels_fail_and_filled_ones_pass() {
        let bills = "Bill Summary\n============\nWatercare\nPrevious balance: \nCurrent charges: \nTotal: \nDue date: \n";
        assert_eq!(super::empty_labels(bills).as_deref(), Some("Previous balance:"));
        assert!(super::empty_labels("Total: $84.20\nDue date: 12 Nov\nNotes:\n").is_none());
        assert!(super::empty_labels("Weather 8:00 sunny\nSee https://x.example\n").is_none());
        assert!(super::empty_labels("Total: N/A\nDue: unknown\n").is_some());
        assert_eq!(super::empty_labels("Featured article tools:\n").as_deref(), Some("Featured article tools:"));
        assert!(super::empty_labels("Europejara\n").is_none());
    }
}
