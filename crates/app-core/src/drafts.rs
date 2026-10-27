//! Local task drafting: preview a selected CSV, propose a finite mapping, and ask the
//! rule questions the host cannot settle from the data (plan §2.2 "Does 03/04/26 mean
//! March 4 or April 3?"). Everything here runs on the device; nothing is sent anywhere.

use crate::dto::{CsvPreview, FileHandle, QuestionOption, RuleQuestion, TaskDraft};
use macroscope_domain::{
    CreatePermission, CsvMapping, DateFormat, InputKind, OnError, OutputColumn, PermissionCeiling,
    ReadPermission, TaskContract, TaskId, Transform,
};

pub const PREVIEW_ROWS: usize = 8;
pub const CELL_LIMIT: usize = 200;
pub const MAX_COLUMNS: usize = 1024;
pub const MAX_INPUT_BYTES: u64 = 10_000_000;
pub const DATE_QUESTION: &str = "date-format";

pub struct ParsedCsv {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub row_count: u64,
}

fn truncate(cell: &str) -> String {
    let mut out: String = cell.chars().take(CELL_LIMIT).collect();
    if cell.chars().count() > CELL_LIMIT {
        out.push('…');
    }
    out
}

/// Parses the header, keeps up to `sample` rows for analysis, and counts all rows.
pub fn parse(bytes: &[u8], sample: usize) -> Result<ParsedCsv, String> {
    let mut reader = csv::ReaderBuilder::new().flexible(false).from_reader(bytes);
    let columns: Vec<String> = reader
        .headers()
        .map_err(|e| format!("This file is not a readable CSV: {e}"))?
        .iter()
        .map(str::to_string)
        .collect();
    if columns.is_empty() || columns.iter().all(|c| c.trim().is_empty()) {
        return Err("The CSV has no header row.".into());
    }
    let mut names = std::collections::HashSet::new();
    if columns.len() > MAX_COLUMNS {
        return Err(format!("This CSV has more than {MAX_COLUMNS} columns."));
    }
    if columns
        .iter()
        .any(|c| c.trim().is_empty() || !names.insert(c))
    {
        return Err("CSV headers must be nonempty and unique.".into());
    }
    let mut rows = Vec::new();
    let mut row_count = 0u64;
    for record in reader.records() {
        let record = record.map_err(|e| format!("Row {} could not be read: {e}", row_count + 1))?;
        if rows.len() < sample {
            rows.push(record.iter().map(str::to_string).collect());
        }
        row_count += 1;
    }
    Ok(ParsedCsv {
        columns,
        rows,
        row_count,
    })
}

pub fn preview(handle: FileHandle, bytes: &[u8]) -> Result<CsvPreview, String> {
    let text =
        std::str::from_utf8(bytes).map_err(|_| "Choose a valid UTF-8 text file.".to_string())?;
    let parsed = match parse(bytes, PREVIEW_ROWS) {
        Ok(parsed)
            if parsed.columns.len() > 1
                || handle.display_name.to_ascii_lowercase().ends_with(".csv") =>
        {
            parsed
        }
        _ => {
            return Ok(CsvPreview {
                handle,
                columns: vec![],
                rows: vec![],
                row_count: text.lines().count() as u64,
                byte_size: bytes.len() as u64,
                text: Some(text_preview(text)),
            })
        }
    };
    Ok(CsvPreview {
        text: None,
        handle,
        columns: parsed.columns.clone(),
        rows: parsed
            .rows
            .iter()
            .map(|r| r.iter().map(|c| truncate(c)).collect())
            .collect(),
        row_count: parsed.row_count,
        byte_size: bytes.len() as u64,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SlashDate {
    /// Day/month order cannot be told apart from the values seen.
    Ambiguous {
        four_digit_year: bool,
    },
    MonthFirst {
        four_digit_year: bool,
    },
    DayFirst {
        four_digit_year: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    SlashDate(SlashDate),
    IsoDate,
    Money,
    Text,
}

fn slash_parts(v: &str) -> Option<(u32, u32, usize)> {
    let parts: Vec<&str> = v.split('/').collect();
    if parts.len() != 3
        || !parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    if parts[0].len() > 2 || parts[1].len() > 2 || !(parts[2].len() == 2 || parts[2].len() == 4) {
        return None;
    }
    Some((
        parts[0].parse().ok()?,
        parts[1].parse().ok()?,
        parts[2].len(),
    ))
}

fn is_iso_date(v: &str) -> bool {
    let b = v.as_bytes();
    v.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && v.bytes()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
}

fn is_money(v: &str) -> bool {
    let mut s = v.trim();
    if let Some(inner) = s.strip_prefix('(').and_then(|x| x.strip_suffix(')')) {
        s = inner;
    }
    s = s.strip_prefix('-').unwrap_or(s);
    s = s.trim_start_matches(['$', '€', '£']);
    let (whole, frac) = s.split_once('.').unwrap_or((s, ""));
    let groups: Vec<&str> = whole.split(',').collect();
    !whole.is_empty()
        && groups[0].len() <= 3.max(if groups.len() == 1 { usize::MAX } else { 3 })
        && groups
            .iter()
            .all(|g| !g.is_empty() && g.bytes().all(|b| b.is_ascii_digit()))
        && groups.iter().skip(1).all(|g| g.len() == 3)
        && frac.bytes().all(|b| b.is_ascii_digit())
        && frac.len() <= 4
        && (v.contains(['$', '€', '£']) || v.contains(',') || !frac.is_empty())
}

fn classify(values: &[&str]) -> Kind {
    let present: Vec<&str> = values
        .iter()
        .copied()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .collect();
    if present.is_empty() {
        return Kind::Text;
    }
    if present.iter().all(|v| is_iso_date(v)) {
        return Kind::IsoDate;
    }
    let slashes: Option<Vec<(u32, u32, usize)>> = present.iter().map(|v| slash_parts(v)).collect();
    if let Some(parts) = slashes {
        let four = parts.iter().all(|p| p.2 == 4);
        let two = parts.iter().all(|p| p.2 == 2);
        if four || two {
            let first_over = parts.iter().any(|p| p.0 > 12);
            let second_over = parts.iter().any(|p| p.1 > 12);
            return Kind::SlashDate(match (first_over, second_over) {
                (false, true) => SlashDate::MonthFirst {
                    four_digit_year: four,
                },
                (true, false) => SlashDate::DayFirst {
                    four_digit_year: four,
                },
                _ => SlashDate::Ambiguous {
                    four_digit_year: four,
                },
            });
        }
    }
    if present.iter().all(|v| is_money(v)) {
        return Kind::Money;
    }
    Kind::Text
}

fn currency_of(values: &[&str]) -> &'static str {
    if values.iter().any(|v| v.contains('€')) {
        "EUR"
    } else if values.iter().any(|v| v.contains('£')) {
        "GBP"
    } else {
        "USD"
    }
}

fn output_name_for(column: &str) -> String {
    let lower = column.to_ascii_lowercase();
    let pretty = match lower.as_str() {
        "order_id" | "orderid" | "order" | "order_number" => "Order",
        "created" | "created_at" | "date" | "order_date" => "Date",
        "amount" | "total" | "price" => "Amount",
        _ => "",
    };
    if !pretty.is_empty() {
        return pretty.into();
    }
    // Title-case snake/kebab names: "customer_name" -> "Customer name".
    let words = column.replace(['_', '-'], " ");
    let mut chars = words.trim().chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => column.to_string(),
    }
}

fn date_format(slash: SlashDate, day_first: bool) -> DateFormat {
    let four = match slash {
        SlashDate::Ambiguous { four_digit_year }
        | SlashDate::MonthFirst { four_digit_year }
        | SlashDate::DayFirst { four_digit_year } => four_digit_year,
    };
    match (day_first, four) {
        (false, false) => DateFormat::MmDdYy,
        (false, true) => DateFormat::MmDdYyyy,
        (true, false) => DateFormat::DdMmYy,
        (true, true) => DateFormat::DdMmYyyy,
    }
}

/// Free-text columns that look personal are left out of the proposed output by default:
/// the user can add them back. Keeps a first draft from copying names/emails onward.
fn looks_personal(column: &str) -> bool {
    let c = column.to_ascii_lowercase();
    ["customer", "name", "email", "phone", "address", "contact"]
        .iter()
        .any(|w| c.contains(w))
}

pub struct Proposal {
    pub mapping: CsvMapping,
    pub questions: Vec<RuleQuestion>,
}

pub fn propose(parsed: &ParsedCsv) -> Proposal {
    let mut output_columns = Vec::new();
    let mut questions = Vec::new();
    for (index, column) in parsed.columns.iter().enumerate() {
        let values: Vec<&str> = parsed
            .rows
            .iter()
            .filter_map(|r| r.get(index).map(String::as_str))
            .collect();
        let transform = match classify(&values) {
            Kind::IsoDate => Transform::Date {
                from: DateFormat::YyyyMmDd,
                to: DateFormat::YyyyMmDd,
            },
            Kind::SlashDate(slash) => {
                let day_first = matches!(slash, SlashDate::DayFirst { .. });
                if let SlashDate::Ambiguous { four_digit_year } = slash {
                    {
                        let example = values
                            .iter()
                            .find(|v| !v.trim().is_empty())
                            .copied()
                            .unwrap_or("03/04/26");
                        let mut question = date_question(column, example, four_digit_year);
                        if !questions.is_empty() {
                            question.id = format!("{DATE_QUESTION}:{index}");
                        }
                        questions.push(question);
                    }
                }
                Transform::Date {
                    from: date_format(slash, day_first),
                    to: DateFormat::YyyyMmDd,
                }
            }
            Kind::Money => Transform::Money {
                currency: currency_of(&values).into(),
                decimals: 2,
            },
            Kind::Text => {
                if looks_personal(column) {
                    continue;
                }
                Transform::Copy
            }
        };
        output_columns.push(OutputColumn {
            name: output_name_for(column),
            source: column.clone(),
            transform,
        });
    }
    Proposal {
        mapping: CsvMapping {
            output_columns,
            on_error: OnError::StopAndExplain,
            skip_header_rows: 0,
            formula_escape: true,
        },
        questions,
    }
}

fn date_question(column: &str, example: &str, four_digit_year: bool) -> RuleQuestion {
    let (a, b) = slash_parts(example).map(|p| (p.0, p.1)).unwrap_or((3, 4));
    let month = |m: u32| {
        [
            "January",
            "February",
            "March",
            "April",
            "May",
            "June",
            "July",
            "August",
            "September",
            "October",
            "November",
            "December",
        ]
        .get(m.saturating_sub(1) as usize)
        .copied()
        .unwrap_or("?")
    };
    let year = if four_digit_year { "YYYY" } else { "YY" };
    let (mf, df) = if four_digit_year {
        ("mm_dd_yyyy", "dd_mm_yyyy")
    } else {
        ("mm_dd_yy", "dd_mm_yy")
    };
    RuleQuestion {
        id: DATE_QUESTION.into(),
        column: column.into(),
        prompt: format!("Does {example} mean {} {b} or {} {a}?", month(a), month(b)),
        options: vec![
            QuestionOption {
                value: mf.into(),
                label: format!("{} {b} (MM/DD/{year})", month(a)),
            },
            QuestionOption {
                value: df.into(),
                label: format!("{} {a} (DD/MM/{year})", month(b)),
            },
        ],
        answer: None,
    }
}

pub fn parse_date_answer(answer: &str) -> Option<DateFormat> {
    serde_json::from_value(serde_json::Value::String(answer.into())).ok()
}

/// Plain-language task summary lines (plan §2.1 wording).
pub fn text_preview(text: &str) -> String {
    text.lines()
        .take(PREVIEW_ROWS)
        .map(|l| l.chars().take(500).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Conservative local routing: only the finite operations may choose mapping.
/// Unknown requests go to programs; the user can always switch explicitly.
pub fn fits_mapping(description: &str) -> bool {
    let text = description.to_ascii_lowercase();
    if [
        "extract",
        "filter",
        "containing",
        "group",
        "sum",
        "sort",
        "dedup",
        "duplicat",
        "blank rows",
        "empty rows",
        "blank lines",
        "empty lines",
        "remove rows",
        "delete rows",
        "calculat",
        "multiply",
        "divide",
        "subtract",
        "percent",
        "tax",
        "replace",
        "regex",
        "search",
        "find",
        "pivot",
        "split",
        "join",
        "merge",
        "count",
        "json",
        "markdown",
        "every line",
    ]
    .iter()
    .any(|w| text.contains(w))
    {
        return false;
    }
    text.trim().is_empty()
        || [
            "clean",
            "copy",
            "import",
            "map",
            "column",
            "date",
            "currency",
            "amount",
            "trim",
            "uppercase",
            "lowercase",
            "prepare",
            "rename",
        ]
        .iter()
        .any(|w| text.contains(w))
}

pub fn summary(draft: &TaskDraft) -> Vec<String> {
    if draft.kind == crate::dto::TaskKind::Program {
        return vec![
            draft.goal.clone(),
            "Program (sandboxed): read the selected text file and create one output.".into(),
            format!("Create {}; leave the source unchanged.", draft.output_name),
        ];
    }
    let mut lines = vec!["You described this task; recurrence unmeasured.".to_string()];
    let names: Vec<&str> = draft
        .mapping
        .output_columns
        .iter()
        .map(|c| c.name.as_str())
        .collect();
    lines.push(format!(
        "Map {} rows to {}.",
        draft.input.row_count,
        if names.is_empty() {
            "no columns yet".into()
        } else {
            names.join(", ")
        }
    ));
    let mut rules = Vec::new();
    for column in &draft.mapping.output_columns {
        match &column.transform {
            Transform::Date { to, .. } => {
                rules.push(format!("dates in {} to {}", column.name, format_label(*to)))
            }
            Transform::Money { currency, decimals } => rules.push(format!(
                "{} to decimal {currency} with {decimals} places",
                column.name
            )),
            _ => {}
        }
    }
    if !rules.is_empty() {
        lines.push(format!(
            "Convert {}; stop and explain malformed values.",
            rules.join(" and ")
        ));
    }
    lines.push(format!(
        "Create one new CSV ({}); leave the source unchanged.",
        draft.output_name
    ));
    lines
}

pub fn format_label(f: DateFormat) -> &'static str {
    match f {
        DateFormat::MmDdYy => "MM/DD/YY",
        DateFormat::MmDdYyyy => "MM/DD/YYYY",
        DateFormat::DdMmYy => "DD/MM/YY",
        DateFormat::DdMmYyyy => "DD/MM/YYYY",
        DateFormat::YyyyMmDd | DateFormat::Iso8601 => "YYYY-MM-DD",
    }
}

pub fn contract_for(draft: &TaskDraft) -> TaskContract {
    // Programs treat non-.csv files as text, even when commas let the preview
    // parser infer a table. CSV mappings and programs taught with a .csv input
    // retain the complete confirmed header; do not infer dependencies from JS.
    let input_kind = if draft.kind == crate::dto::TaskKind::Program
        && !draft
            .input
            .handle
            .display_name
            .to_ascii_lowercase()
            .ends_with(".csv")
    {
        InputKind::Text
    } else {
        InputKind::Csv
    };
    TaskContract {
        id: TaskId(draft.draft_id.clone()),
        revision: draft.revision,
        goal: if draft.goal.trim().is_empty() {
            "Transform the selected CSV into a new CSV.".into()
        } else {
            draft.goal.clone()
        },
        input_kind,
        input_schema: if input_kind == InputKind::Csv {
            draft.input.columns.clone()
        } else {
            vec![]
        },
        output_columns: if draft.kind == crate::dto::TaskKind::Program {
            vec![]
        } else {
            draft
                .mapping
                .output_columns
                .iter()
                .map(|c| c.name.clone())
                .collect()
        },
        mapping: draft.mapping.clone(),
        error_policy: OnError::StopAndExplain,
        acceptance_checks: vec![
            "Preserve every input row in order.".into(),
            "Stop and explain malformed dates, amounts, missing columns, and header-only files."
                .into(),
            "Keep the selected source unchanged; create one new output and never replace a file."
                .into(),
        ],
        permission_ceiling: PermissionCeiling {
            read: ReadPermission::SelectedInput,
            create: CreatePermission::OneOutputInSelectedRoot,
        },
        required_connections: draft.mentioned_connectors.clone(),
    }
}

/// Recomputes readiness after any change.
pub fn refresh(draft: &mut TaskDraft) {
    let mut blockers = Vec::new();
    if draft.kind == crate::dto::TaskKind::Program {
        if macroscope_effect_broker::validate_basename(&draft.output_name).is_err() {
            blockers
                .push("Use a .csv, .tsv, .txt, .md or .json output name, without folders.".into());
        }
        draft.summary = summary(draft);
        draft.ready_to_build = blockers.is_empty();
        draft.blockers = blockers;
        return;
    }
    for q in &draft.questions {
        if q.answer.is_none() {
            blockers.push(format!("Answer: {}", q.prompt));
        }
    }
    if draft.mapping.output_columns.is_empty() {
        blockers.push("Add at least one output column.".into());
    }
    let name = draft.output_name.trim();
    if name.is_empty()
        || !name.to_ascii_lowercase().ends_with(".csv")
        || name.contains(['/', '\\', ':'])
    {
        blockers.push("Use an output file name ending in .csv, without folders.".into());
    }
    if let Err(problems) =
        macroscope_executor::validate_mapping(&draft.mapping, &contract_for(draft))
    {
        blockers.extend(problems);
    }
    draft.summary = summary(draft);
    draft.ready_to_build = blockers.is_empty();
    draft.blockers = blockers;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(text: &str) -> ParsedCsv {
        parse(text.as_bytes(), 50).unwrap()
    }

    #[test]
    fn finite_routing_refuses_row_changes_arithmetic_and_search_even_with_cleaning_words() {
        for description in [
            "Clean CSV and remove duplicate rows",
            "Prepare the import file with blank rows removed",
            "Clean amounts and calculate tax",
            "Rename columns and sort by date",
            "Clean CSV and extract lines containing ERROR",
            "Copy columns and replace ID prefixes",
        ] {
            assert!(!fits_mapping(description), "{description}");
        }
        for description in [
            "Clean this orders CSV for import: Order, Date, Amount.",
            "Copy columns",
            "Trim spaces and uppercase codes",
            "Rename columns and convert dates",
        ] {
            assert!(fits_mapping(description), "{description}");
        }
    }

    #[test]
    fn headers_keep_exact_names_and_reject_empty_duplicate_or_excessive_width() {
        let p = parsed("\u{feff} id ,amount\r\nA,1\r\n");
        assert_eq!(p.columns, [" id ", "amount"]);
        for text in ["a,a\n1,2\n", "a,\n1,2\n", "a,   \n1,2\n"] {
            assert!(parse(text.as_bytes(), 8).is_err(), "{text:?}");
        }
        let wide = (0..1025)
            .map(|i| format!("c{i}"))
            .collect::<Vec<_>>()
            .join(",");
        assert!(parse(wide.as_bytes(), 8).is_err());
    }

    #[test]
    fn quoted_newlines_and_crlf_are_csv_records() {
        let p = parsed("a,b\r\n\"first\r\nsecond\",x\r\n");
        assert_eq!(p.row_count, 1);
        assert_eq!(p.rows[0], ["first\r\nsecond", "x"]);
    }

    #[test]
    fn every_ambiguous_date_column_gets_its_own_question() {
        let proposal = propose(&parsed("created,paid\n03/04/26,05/06/2026\n"));
        assert_eq!(proposal.questions.len(), 2);
        assert_ne!(proposal.questions[0].id, proposal.questions[1].id);
    }

    #[test]
    fn orders_fixture_proposal_matches_the_confirmed_task() {
        let p = parsed(include_str!("../../../fixtures/orders/input.csv"));
        assert_eq!(p.row_count, 18);
        let proposal = propose(&p);
        let names: Vec<_> = proposal
            .mapping
            .output_columns
            .iter()
            .map(|c| c.name.as_str())
            .collect();
        assert_eq!(
            names,
            ["Order", "Date", "Amount"],
            "customer is personal and left out"
        );
        assert!(matches!(
            proposal.mapping.output_columns[1].transform,
            Transform::Date {
                from: DateFormat::MmDdYy,
                ..
            }
        ));
        assert!(matches!(
            &proposal.mapping.output_columns[2].transform,
            Transform::Money { currency, decimals: 2 } if currency == "USD"
        ));
    }

    #[test]
    fn ambiguous_dates_ask_with_real_month_names() {
        let p = parsed("when,total\n03/04/26,$1.00\n05/06/26,$2.00\n");
        let proposal = propose(&p);
        let q = &proposal.questions[0];
        assert_eq!(q.prompt, "Does 03/04/26 mean March 4 or April 3?");
        assert_eq!(q.options[0].value, "mm_dd_yy");
        assert_eq!(q.options[1].value, "dd_mm_yy");
        assert_eq!(parse_date_answer("dd_mm_yy"), Some(DateFormat::DdMmYy));
    }

    #[test]
    fn unambiguous_orders_need_no_question() {
        let day_first = propose(&parsed("d\n25/12/2026\n01/02/2026\n"));
        assert!(day_first.questions.is_empty());
        assert!(matches!(
            day_first.mapping.output_columns[0].transform,
            Transform::Date {
                from: DateFormat::DdMmYyyy,
                ..
            }
        ));
        let iso = propose(&parsed("d\n2026-01-05\n"));
        assert!(iso.questions.is_empty());
    }

    #[test]
    fn money_detection_is_conservative() {
        for v in ["$1,240.00", "(5.10)", "€3.50", "12.5", "1,000"] {
            assert!(is_money(v), "{v}");
        }
        for v in ["1,2,3", "ORD-001", "12", "abc", "1.2.3", "12,34"] {
            assert!(!is_money(v), "{v}");
        }
    }

    #[test]
    fn preview_truncates_long_cells_and_counts_rows() {
        let long = "x".repeat(500);
        let text = format!("a,b\n{long},2\n3,4\n");
        let pv = preview(
            FileHandle {
                handle_id: "h".into(),
                display_name: "f.csv".into(),
                display_path: "/f.csv".into(),
            },
            text.as_bytes(),
        )
        .unwrap();
        assert_eq!(pv.row_count, 2);
        assert_eq!(pv.rows[0][0].chars().count(), CELL_LIMIT + 1);
        assert!(parse(b"", 1).is_err());
    }
}
