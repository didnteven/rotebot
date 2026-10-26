//! Deterministic, finite host-owned CSV interpreter. No generated code is executed.

use macroscope_domain::{CsvMapping, DateFormat, TaskContract, Transform};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::io::{self, Write};

/// CSV budgets, separate from the manifest's process/runtime budgets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub max_input_bytes: u64,
    pub max_rows: u64,
    pub max_output_bytes: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_input_bytes: 10_000_000,
            max_rows: 1_000_000,
            max_output_bytes: 20_000_000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MappingOutput {
    pub bytes: Vec<u8>,
    pub rows_in: u64,
    pub rows_out: u64,
    /// Lowercase hex SHA-256 of `bytes`.
    pub sha256: String,
}

#[derive(Debug, thiserror::Error)]
pub enum MappingError {
    #[error("missing input columns: {0:?}")]
    MissingColumns(Vec<String>),
    #[error("input has no data rows")]
    EmptyInput,
    #[error("invalid mapping: {0}")]
    InvalidMapping(String),
    #[error("CSV error: {0}")]
    Csv(String),
    #[error("{kind} limit exceeded ({limit})")]
    Limit { kind: &'static str, limit: u64 },
    #[error("row {row_number}, column {column}: {reason} ({value_preview})")]
    Row {
        row_number: u64,
        column: String,
        value_preview: String,
        reason: String,
    },
}

/// Validate an untrusted candidate against the host's contract. The current
/// permission enums allow only pure transforms over the selected input and one
/// create effect; none of the finite Transform variants adds authority.
pub fn validate_mapping(mapping: &CsvMapping, contract: &TaskContract) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    let mut names = HashSet::new();
    for column in &mapping.output_columns {
        if column.name.trim().is_empty() || !names.insert(&column.name) {
            errors.push(format!("empty or duplicate output name: {:?}", column.name));
        }
        if !matches!(column.transform, Transform::Constant { .. })
            && !contract.input_schema.contains(&column.source)
        {
            errors.push(format!(
                "source column outside contract: {:?}",
                column.source
            ));
        }
        if let Transform::Money { currency, .. } = &column.transform {
            if !matches!(currency.as_str(), "USD" | "EUR" | "GBP") {
                errors.push(format!("unsupported currency: {currency}"));
            }
        }
    }
    if mapping
        .output_columns
        .iter()
        .map(|c| &c.name)
        .collect::<Vec<_>>()
        != contract.output_columns.iter().collect::<Vec<_>>()
    {
        errors.push("output columns must match contract in order".into());
    }
    if contract.mapping.formula_escape && !mapping.formula_escape {
        errors.push("contract requires formula escaping".into());
    }
    if mapping.on_error != contract.error_policy {
        errors.push("error policy must match contract".into());
    }
    // Exhaustive matching makes any future authority-bearing ceiling require review.
    match (
        contract.permission_ceiling.read,
        contract.permission_ceiling.create,
    ) {
        (
            macroscope_domain::ReadPermission::SelectedInput,
            macroscope_domain::CreatePermission::OneOutputInSelectedRoot,
        ) => {}
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn reader(input: &[u8]) -> csv::Reader<&[u8]> {
    csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_reader(input)
}

fn header(reader: &mut csv::Reader<&[u8]>, skip: u32) -> Result<csv::StringRecord, MappingError> {
    let mut record = csv::StringRecord::new();
    for _ in 0..=skip {
        if !reader
            .read_record(&mut record)
            .map_err(|e| MappingError::Csv(e.to_string()))?
        {
            return Err(MappingError::EmptyInput);
        }
    }
    let mut names = HashSet::new();
    if record
        .iter()
        .any(|name| name.is_empty() || !names.insert(name.to_owned()))
    {
        return Err(MappingError::Csv("empty or duplicate input header".into()));
    }
    Ok(record)
}

/// Check all contract-required fields, including fields not emitted by a mapping.
/// `skip_header_rows` counts CSV records preceding the real header.
pub fn require_columns(
    input: &[u8],
    skip_header_rows: u32,
    columns: &[String],
) -> Result<(), MappingError> {
    let headers = header(&mut reader(input), skip_header_rows)?;
    let missing: Vec<_> = columns
        .iter()
        .filter(|c| !headers.iter().any(|h| h == c.as_str()))
        .cloned()
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(MappingError::MissingColumns(missing))
    }
}

/// Interpret a mapping with RFC 4180 quoting and LF record terminators.
/// Required sources must exist. Two-digit dates mean 2000–2099; ISO8601 here
/// means the fixed date-only form YYYY-MM-DD. Money uses string arithmetic and
/// rounds half away from zero, never binary floating point. A failure returns
/// no output, including when a later row fails. Blank/header-only input fails.
pub fn apply_mapping(
    mapping: &CsvMapping,
    input: &[u8],
    limits: &Limits,
) -> Result<MappingOutput, MappingError> {
    if input.len() as u64 > limits.max_input_bytes {
        return Err(MappingError::Limit {
            kind: "input bytes",
            limit: limits.max_input_bytes,
        });
    }
    if !matches!(mapping.on_error, macroscope_domain::OnError::StopAndExplain) {
        return Err(MappingError::InvalidMapping(
            "unsupported error policy".into(),
        ));
    }
    let mut names = HashSet::new();
    if mapping.output_columns.is_empty()
        || mapping
            .output_columns
            .iter()
            .any(|c| c.name.trim().is_empty() || !names.insert(&c.name))
    {
        return Err(MappingError::InvalidMapping(
            "output names must be nonempty and unique".into(),
        ));
    }
    let mut reader = reader(input);
    let headers = header(&mut reader, mapping.skip_header_rows)?;
    let missing: Vec<_> = mapping
        .output_columns
        .iter()
        .filter(|c| {
            !matches!(c.transform, Transform::Constant { .. })
                && !headers.iter().any(|h| h == c.source)
        })
        .map(|c| c.source.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    if !missing.is_empty() {
        return Err(MappingError::MissingColumns(missing));
    }
    let indices: Vec<_> = mapping
        .output_columns
        .iter()
        .map(|c| headers.iter().position(|h| h == c.source))
        .collect();
    let mut buffer = BoundedOutput {
        bytes: Vec::new(),
        limit: limits.max_output_bytes,
        exceeded: false,
    };
    let mut writer = csv::WriterBuilder::new()
        .terminator(csv::Terminator::Any(b'\n'))
        .from_writer(&mut buffer);
    let result = {
        let mut write = |cells: &[String]| -> Result<(), MappingError> {
            writer
                .write_record(cells)
                .map_err(|e| MappingError::Csv(e.to_string()))?;
            writer.flush().map_err(|e| MappingError::Csv(e.to_string()))
        };
        // Column labels are untrusted cells too. Normal contract labels are unchanged.
        (|| {
            write(
                &mapping
                    .output_columns
                    .iter()
                    .map(|c| escape_formula(&c.name, mapping.formula_escape))
                    .collect::<Vec<_>>(),
            )?;
            let mut rows_in = 0;
            let mut rows_out = 0;
            for record in reader.records() {
                let row_number = rows_in + 1;
                if row_number > limits.max_rows {
                    return Err(MappingError::Limit {
                        kind: "rows",
                        limit: limits.max_rows,
                    });
                }
                let record = record.map_err(|e| MappingError::Row {
                    row_number,
                    column: "<csv>".into(),
                    value_preview: String::new(),
                    reason: e.to_string(),
                })?;
                if record.len() != headers.len() {
                    return Err(MappingError::Row {
                        row_number,
                        column: "<csv>".into(),
                        value_preview: String::new(),
                        reason: "record width differs from header".into(),
                    });
                }
                rows_in += 1;
                let mut cells = Vec::with_capacity(mapping.output_columns.len());
                let mut row_bytes = 0u64;
                for (column, index) in mapping.output_columns.iter().zip(&indices) {
                    let value = index.map_or("", |i| &record[i]);
                    if let Transform::Constant { value } = &column.transform {
                        if value.len() as u64 > limits.max_output_bytes {
                            return Err(MappingError::Limit {
                                kind: "output bytes",
                                limit: limits.max_output_bytes,
                            });
                        }
                    }
                    let cell = transform(&column.transform, value).map_err(|reason| {
                        MappingError::Row {
                            row_number,
                            column: column.source.clone(),
                            value_preview: preview(value),
                            reason,
                        }
                    })?;
                    row_bytes = row_bytes.saturating_add(cell.len() as u64);
                    if row_bytes > limits.max_output_bytes {
                        return Err(MappingError::Limit {
                            kind: "output bytes",
                            limit: limits.max_output_bytes,
                        });
                    }
                    cells.push(escape_formula(&cell, mapping.formula_escape));
                }
                write(&cells)?;
                rows_out += 1;
            }
            if rows_in == 0 {
                return Err(MappingError::EmptyInput);
            }
            Ok((rows_in, rows_out))
        })()
    };
    drop(writer);
    if buffer.exceeded {
        return Err(MappingError::Limit {
            kind: "output bytes",
            limit: limits.max_output_bytes,
        });
    }
    let (rows_in, rows_out) = result?;
    assert_eq!(rows_in, rows_out);
    Ok(MappingOutput {
        sha256: format!("{:x}", Sha256::digest(&buffer.bytes)),
        bytes: buffer.bytes,
        rows_in,
        rows_out,
    })
}

struct BoundedOutput {
    bytes: Vec<u8>,
    limit: u64,
    exceeded: bool,
}
impl Write for BoundedOutput {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if (self.bytes.len() as u64).saturating_add(bytes.len() as u64) > self.limit {
            self.exceeded = true;
            return Err(io::Error::other("output byte limit"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn preview(value: &str) -> String {
    value
        .chars()
        .flat_map(|c| {
            if c.is_control() {
                c.escape_default().collect::<Vec<_>>()
            } else {
                vec![c]
            }
        })
        .take(40)
        .collect()
}

fn decimal_number(value: &str) -> bool {
    let value = value
        .strip_prefix('-')
        .or_else(|| value.strip_prefix('+'))
        .unwrap_or(value);
    let mut parts = value.split('.');
    let integer = parts.next().unwrap_or("");
    !integer.is_empty()
        && integer.bytes().all(|b| b.is_ascii_digit())
        && parts
            .next()
            .is_none_or(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
        && parts.next().is_none()
}

fn escape_formula(value: &str, enabled: bool) -> String {
    if enabled && value.starts_with(['=', '+', '-', '@', '\t', '\r']) && !decimal_number(value) {
        format!("'{value}")
    } else {
        value.to_owned()
    }
}

fn transform(operation: &Transform, value: &str) -> Result<String, String> {
    Ok(match operation {
        Transform::Copy => value.to_owned(),
        Transform::Trim => value.trim().to_owned(),
        Transform::Upper => value.to_uppercase(),
        Transform::Lower => value.to_lowercase(),
        Transform::Constant { value } => value.clone(),
        Transform::Money { currency, decimals } => money(value, currency, *decimals)?,
        Transform::Date { from, to } => date(value, *from, *to)?,
    })
}

fn money(value: &str, currency: &str, decimals: u8) -> Result<String, String> {
    let invalid = || "invalid or blank decimal money".to_owned();
    let symbol = match currency {
        "USD" => '$',
        "EUR" => '€',
        "GBP" => '£',
        _ => return Err("unsupported currency".into()),
    };
    let mut text = value.trim();
    let parens = text.starts_with('(') && text.ends_with(')');
    if parens {
        text = &text[1..text.len() - 1];
    }
    let mut negative = parens;
    let mut sign = false;
    if let Some(rest) = text.strip_prefix('-') {
        negative = true;
        sign = true;
        text = rest;
    }
    if let Some(rest) = text.strip_prefix(symbol) {
        text = rest;
    }
    if !sign && !parens {
        if let Some(rest) = text.strip_prefix('-') {
            negative = true;
            text = rest;
        }
    } else if parens && sign {
        return Err(invalid());
    }
    let mut parts = text.split('.');
    let integer = parts.next().ok_or_else(invalid)?;
    let fraction = parts.next().unwrap_or("");
    if parts.next().is_some()
        || (!fraction.is_empty() && !fraction.bytes().all(|b| b.is_ascii_digit()))
        || text.ends_with('.')
    {
        return Err(invalid());
    }
    let groups: Vec<_> = integer.split(',').collect();
    if groups
        .iter()
        .any(|g| g.is_empty() || !g.bytes().all(|b| b.is_ascii_digit()))
        || (groups.len() > 1 && (groups[0].len() > 3 || groups[1..].iter().any(|g| g.len() != 3)))
    {
        return Err(invalid());
    }
    let integer = groups.concat();
    let integer = integer.trim_start_matches('0');
    let integer = if integer.is_empty() { "0" } else { integer };
    let scale = usize::from(decimals);
    let mut digits = integer.as_bytes().to_vec();
    digits.extend(fraction.bytes().take(scale));
    digits.resize(integer.len() + scale, b'0');
    if fraction.as_bytes().get(scale).is_some_and(|b| *b >= b'5') {
        let mut carry = true;
        for digit in digits.iter_mut().rev() {
            if *digit == b'9' {
                *digit = b'0';
            } else {
                *digit += 1;
                carry = false;
                break;
            }
        }
        if carry {
            digits.insert(0, b'1');
        }
    }
    let nonzero = digits.iter().any(|d| *d != b'0');
    if scale > 0 {
        digits.insert(digits.len() - scale, b'.');
    }
    let mut output = String::from_utf8(digits).map_err(|_| invalid())?;
    if negative && nonzero {
        output.insert(0, '-');
    }
    Ok(output)
}

fn date(value: &str, from: DateFormat, to: DateFormat) -> Result<String, String> {
    use DateFormat::*;
    let invalid = || "invalid date for fixed source format".to_owned();
    let (separator, widths) = match from {
        MmDdYy | DdMmYy => ('/', [2, 2, 2]),
        MmDdYyyy | DdMmYyyy => ('/', [2, 2, 4]),
        YyyyMmDd | Iso8601 => ('-', [4, 2, 2]),
    };
    let parts: Vec<_> = value.split(separator).collect();
    if parts.len() != 3
        || parts
            .iter()
            .zip(widths)
            .any(|(p, w)| p.len() != w || !p.bytes().all(|b| b.is_ascii_digit()))
    {
        return Err(invalid());
    }
    let numbers: Vec<u32> = parts
        .iter()
        .map(|p| p.parse::<u32>().map_err(|_| invalid()))
        .collect::<Result<_, _>>()?;
    let (year, month, day) = match from {
        MmDdYy => (2000 + numbers[2], numbers[0], numbers[1]),
        DdMmYy => (2000 + numbers[2], numbers[1], numbers[0]),
        MmDdYyyy => (numbers[2], numbers[0], numbers[1]),
        DdMmYyyy => (numbers[2], numbers[1], numbers[0]),
        YyyyMmDd | Iso8601 => (numbers[0], numbers[1], numbers[2]),
    };
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if leap {
                29
            } else {
                28
            }
        }
        _ => return Err(invalid()),
    };
    if year == 0 || day == 0 || day > days {
        return Err(invalid());
    }
    Ok(match to {
        MmDdYy => {
            if !(2000..=2099).contains(&year) {
                return Err("two-digit output would lose century".into());
            }
            format!("{month:02}/{day:02}/{:02}", year - 2000)
        }
        DdMmYy => {
            if !(2000..=2099).contains(&year) {
                return Err("two-digit output would lose century".into());
            }
            format!("{day:02}/{month:02}/{:02}", year - 2000)
        }
        MmDdYyyy => format!("{month:02}/{day:02}/{year:04}"),
        DdMmYyyy => format!("{day:02}/{month:02}/{year:04}"),
        YyyyMmDd | Iso8601 => format!("{year:04}-{month:02}-{day:02}"),
    })
}

#[cfg(test)]
mod tests;
