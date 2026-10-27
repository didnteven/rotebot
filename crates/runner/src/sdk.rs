use crate::{Failure, Limits, Output, Shared};
use macroscope_domain::DateFormat;
use rquickjs::{function::Opt, Array, Ctx, Exception, Function, Object, Result};
use std::{
    collections::{BTreeMap, HashSet},
    io::{self, Write},
};

struct Data {
    inputs: BTreeMap<String, String>,
    limits: Limits,
    state: Shared,
}
// SAFETY: Data contains only owned Rust values, with no JavaScript lifetimes.
unsafe impl<'js> rquickjs::JsLifetime<'js> for Data {
    type Changed<'to> = Data;
}
fn fail(ctx: &Ctx<'_>, kind: &str, message: &str) -> rquickjs::Error {
    let data = ctx.userdata::<Data>().unwrap();
    data.state
        .lock()
        .unwrap()
        .failure
        .get_or_insert_with(|| Failure::new(kind, message));
    Exception::throw_type(ctx, message)
}
fn input(ctx: Ctx<'_>, handle: String) -> Result<String> {
    ctx.userdata::<Data>()
        .unwrap()
        .inputs
        .get(&handle)
        .cloned()
        .ok_or_else(|| fail(&ctx, "forbidden", "unknown input handle"))
}
fn output(ctx: Ctx<'_>, name: String, text: String) -> Result<()> {
    if name.len() > 255
        || name.is_empty()
        || name.contains(['/', '\\', '\0', ':'])
        || name.chars().any(char::is_control)
        || ![".csv", ".tsv", ".txt", ".md", ".json"]
            .iter()
            .any(|ext| name.to_ascii_lowercase().ends_with(ext))
    {
        return Err(fail(
            &ctx,
            "forbidden",
            "output requires a .csv, .tsv, .txt, .md or .json basename",
        ));
    }
    let data = ctx.userdata::<Data>().unwrap();
    if text.len() > data.limits.output_bytes {
        return Err(fail(&ctx, "output_limit", "output byte limit exceeded"));
    }
    let mut state = data.state.lock().unwrap();
    if !state.outputs.is_empty() {
        drop(state);
        return Err(fail(&ctx, "output_limit", "only one output is allowed"));
    }
    state.outputs.push(Output { name, text });
    Ok(())
}
fn log(ctx: Ctx<'_>, text: String) -> Result<()> {
    let data = ctx.userdata::<Data>().unwrap();
    let mut state = data.state.lock().unwrap();
    // Charge fixed per-message overhead too, preventing millions of empty logs.
    let charge = text.len().saturating_add(64);
    if charge > data.limits.log_bytes.saturating_sub(state.log_bytes) {
        drop(state);
        return Err(fail(&ctx, "output_limit", "log byte limit exceeded"));
    }
    state.log_bytes += charge;
    state.log.push(text);
    Ok(())
}
fn parse<'js>(ctx: Ctx<'js>, text: String, options: Opt<Object<'js>>) -> Result<Array<'js>> {
    let mut reader = csv::ReaderBuilder::new().from_reader(text.as_bytes());
    let headers = reader
        .headers()
        .map_err(|_| Exception::throw_type(&ctx, "invalid CSV header"))?
        .clone();
    let mut names = HashSet::new();
    if headers.is_empty()
        || headers.len() > 4096
        || headers.iter().any(|h| h.is_empty() || !names.insert(h))
    {
        return Err(Exception::throw_type(
            &ctx,
            "empty, duplicate or excessive CSV headers",
        ));
    }
    if let Some(options) = options.0 {
        let required: Array = options.get("required")?;
        if required.len() > 4096 {
            return Err(fail(&ctx, "forbidden", "too many required columns"));
        }
        for name in required.iter::<String>() {
            if !names.contains(name?.as_str()) {
                return Err(Exception::throw_type(&ctx, "missing required CSV column"));
            }
        }
    }
    let rows = Array::new(ctx.clone())?;
    for (i, record) in reader.records().enumerate() {
        if i >= 1_000_000 {
            return Err(fail(&ctx, "memory", "CSV row limit exceeded"));
        }
        let record = record.map_err(|_| Exception::throw_type(&ctx, "invalid CSV record"))?;
        let row = Object::new(ctx.clone())?;
        // Null prototype prevents special column names such as __proto__ from
        // changing the object or conferring access to a constructor.
        row.set_prototype(None)?;
        for (name, value) in headers.iter().zip(record.iter()) {
            row.set(name, value)?;
        }
        rows.set(i, row)?;
    }
    if rows.is_empty() {
        return Err(Exception::throw_type(&ctx, "CSV contains no rows"));
    }
    Ok(rows)
}
struct Bounded {
    bytes: Vec<u8>,
    max: usize,
}
impl Write for Bounded {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.max.saturating_sub(self.bytes.len()) {
            return Err(io::Error::other("CSV output limit"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn stringify(ctx: Ctx<'_>, rows: Array<'_>, columns: Array<'_>) -> Result<String> {
    if columns.is_empty() || columns.len() > 4096 || rows.len() > 1_000_000 {
        return Err(fail(&ctx, "forbidden", "CSV shape exceeds limits"));
    }
    let columns: Vec<String> = columns.iter::<String>().collect::<Result<_>>()?;
    let mut unique = HashSet::new();
    if columns.iter().any(|c| c.is_empty() || !unique.insert(c)) {
        return Err(Exception::throw_type(&ctx, "invalid output columns"));
    }
    let max = ctx.userdata::<Data>().unwrap().limits.output_bytes;
    let mut writer = csv::WriterBuilder::new()
        .terminator(csv::Terminator::Any(b'\n'))
        .from_writer(Bounded { bytes: vec![], max });
    writer
        .write_record(&columns)
        .map_err(|_| fail(&ctx, "output_limit", "CSV output byte limit exceeded"))?;
    for row in rows.iter::<Object>() {
        let row = row?;
        for column in &columns {
            let value: String = row.get(column.as_str())?;
            // Same formula policy as the finite executor: numeric negatives are
            // safe decimal text; formula-like cells are quoted with an apostrophe.
            let value = macroscope_executor::escape_formula(&value, true);
            writer
                .write_field(value)
                .map_err(|_| fail(&ctx, "output_limit", "CSV output byte limit exceeded"))?;
        }
        writer
            .write_record(None::<&[u8]>)
            .map_err(|_| fail(&ctx, "output_limit", "CSV output byte limit exceeded"))?;
    }
    let bytes = writer
        .into_inner()
        .map_err(|_| fail(&ctx, "output_limit", "CSV output byte limit exceeded"))?
        .bytes;
    String::from_utf8(bytes).map_err(|_| Exception::throw_type(&ctx, "invalid UTF-8"))
}
fn to_iso(ctx: Ctx<'_>, value: String, format: String) -> Result<String> {
    if value.len() > 4096 || format.len() > 32 {
        return Err(fail(&ctx, "forbidden", "date argument too large"));
    }
    let format = match format.as_str() {
        "MM/DD/YY" => DateFormat::MmDdYy,
        "DD/MM/YY" => DateFormat::DdMmYy,
        "MM/DD/YYYY" => DateFormat::MmDdYyyy,
        "DD/MM/YYYY" => DateFormat::DdMmYyyy,
        "YYYY-MM-DD" | "ISO8601" => DateFormat::Iso8601,
        _ => return Err(Exception::throw_type(&ctx, "unsupported date format")),
    };
    macroscope_executor::date(&value, format, DateFormat::Iso8601)
        .map_err(|_| Exception::throw_type(&ctx, "invalid date"))
}
fn decimal(ctx: Ctx<'_>, value: String, options: Object<'_>) -> Result<String> {
    if value.len() > 4096 {
        return Err(fail(&ctx, "forbidden", "money argument too large"));
    }
    let currency: String = options.get("currency")?;
    let decimals = if options.contains_key("decimals")? {
        let value: rquickjs::Value = options.get("decimals")?;
        let value = value
            .as_int()
            .filter(|n| (0..=255).contains(n))
            .ok_or_else(|| {
                Exception::throw_type(&ctx, "decimals must be an integer from 0 to 255")
            })?;
        value as u8
    } else {
        2
    };
    macroscope_executor::money(&value, &currency, decimals)
        .map_err(|_| Exception::throw_type(&ctx, "invalid decimal money"))
}
pub(crate) fn install(
    ctx: &Ctx<'_>,
    inputs: BTreeMap<String, String>,
    limits: &Limits,
    state: Shared,
) -> Result<()> {
    ctx.store_userdata(Data {
        inputs,
        limits: limits.clone(),
        state,
    })
    .map_err(|_| rquickjs::Error::Unknown)?;
    let sdk = Object::new(ctx.clone())?;
    sdk.set_prototype(None)?;
    sdk.set("input", Function::new(ctx.clone(), input)?)?;
    sdk.set("output", Function::new(ctx.clone(), output)?)?;
    sdk.set("log", Function::new(ctx.clone(), log)?)?;
    let csv = Object::new(ctx.clone())?;
    csv.set_prototype(None)?;
    csv.set("parse", Function::new(ctx.clone(), parse)?)?;
    csv.set("stringify", Function::new(ctx.clone(), stringify)?)?;
    sdk.set("csv", csv)?;
    let date = Object::new(ctx.clone())?;
    date.set_prototype(None)?;
    date.set("toISO", Function::new(ctx.clone(), to_iso)?)?;
    sdk.set("date", date)?;
    let money = Object::new(ctx.clone())?;
    money.set_prototype(None)?;
    money.set("decimal", Function::new(ctx.clone(), decimal)?)?;
    sdk.set("money", money)?;
    ctx.globals().set("macroscope", sdk)
}
