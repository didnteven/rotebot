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

/// Host-owned program checks against the same bounded source snapshot, in two
/// separate fresh worker processes. No candidate-authored tests certify a tool.
#[cfg(feature = "generated-runtime")]
pub type ProgramExecutor<'a> =
    dyn Fn(&[u8], &str) -> Result<Vec<macroscope_runner::Output>, String> + 'a;

#[cfg(feature = "generated-runtime")]
pub fn run_program(
    draft: &crate::dto::TaskDraft,
    program: &macroscope_builder::Program,
    read_input: &dyn Fn() -> Result<Vec<u8>, String>,
    execute: &ProgramExecutor<'_>,
) -> CheckOutcome {
    let before = match read_input() {
        Ok(b) => b,
        Err(e) => return program_failure(e),
    };
    let first = match execute(&before, &program.code) {
        Ok(out) => out,
        Err(e) => return program_failure(e),
    };
    let second = match execute(&before, &program.code) {
        Ok(out) => out,
        Err(e) => return program_failure(e),
    };
    let mut checks = vec![check(
        "sandbox",
        "Runs inside sandbox limits without error",
        true,
        None,
    )];
    let named = program.output_name == draft.output_name
        && first.len() == 1
        && second.len() == 1
        && first[0].name == draft.output_name
        && second[0].name == draft.output_name;
    checks.push(check(
        "one_output",
        "Creates exactly one output with the confirmed name",
        named,
        None,
    ));
    if !named {
        return CheckOutcome {
            checks,
            comparison: None,
        };
    }
    let output = &first[0].text;
    let deterministic = output.as_bytes() == second[0].text.as_bytes();
    checks.push(check(
        "deterministic",
        "Two fresh runs produce identical bytes",
        deterministic,
        None,
    ));
    let valid =
        macroscope_effect_broker::validate_text_output(&draft.output_name, output.as_bytes());
    checks.push(check(
        "output_format",
        "Output is nonempty UTF-8 and parses in its selected format",
        valid.is_ok(),
        valid.err().map(|e| e.to_string()),
    ));
    if let Some(example) = &draft.example_output {
        let mut lines = output.lines();
        let matches = example
            .lines()
            .all(|expected| lines.by_ref().any(|line| line == expected));
        checks.push(check(
            "example",
            "Every example line appears in order",
            matches,
            if matches {
                None
            } else {
                Some("The output does not contain every expected example line in order.".into())
            },
        ));
    }
    checks.push(check(
        "source_unchanged",
        "Source unchanged",
        read_input().is_ok_and(|b| digest(&b) == digest(&before)),
        None,
    ));
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
}
