//! Host-owned checks for a candidate mapping (plan §7.4). The model never sees or
//! writes these; they run in trusted code against the user's selected input and
//! negative cases synthesized from the contract, and a candidate is only "ready"
//! when every check passes. Agent-authored tests cannot certify a tool.

use crate::drafts;
use crate::dto::{CheckResult, Comparison};
use macroscope_domain::{CsvMapping, DateFormat, TaskContract, Transform};
use macroscope_executor::{apply_mapping, validate_mapping, Limits, MappingError};
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
    match apply_mapping(candidate, &before, &limits) {
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
    if let Some(first_source) = candidate
        .output_columns
        .iter()
        .map(|c| c.source.clone())
        .next()
    {
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
            apply_mapping(candidate, missing.as_bytes(), &limits),
            Err(MappingError::MissingColumns(_))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn contract() -> TaskContract {
        serde_json::from_str(include_str!("../../../fixtures/orders/task.json")).unwrap()
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
}
