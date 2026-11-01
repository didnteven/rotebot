//! Only program text and fixed diagnostic templates may survive into host logs
//! and correction packets. Error messages are mutable, and even SyntaxError can
//! originate from JSON.parse on fetched data, so the Error name alone is no
//! guarantee of provenance.
pub(crate) const USER_SCRIPT: &str = "macroscope-user.js";
const HINT: &str = "Use synchronous SDK code only; eval, Function, Worker, Date, Math.random and OS APIs are unavailable";

/// Compile separately so only parser failures can retain SyntaxError text.
/// A top-level `throw new SyntaxError(input)` is an execution failure too.
/// rquickjs exposes compile-only evaluation for modules, but not global scripts.
pub(crate) fn eval_user(
    ctx: &rquickjs::Ctx<'_>,
    code: &str,
    compiling: &mut bool,
) -> rquickjs::Result<()> {
    use rquickjs::{qjs, Error, Value};
    let source = std::ffi::CString::new(code)?;
    // SAFETY: all values belong to this live context; source is NUL-terminated
    // and lives through JS_Eval. Value owns each returned JSValue. EvalFunction
    // consumes a duplicate, leaving the original owned by `compiled`.
    unsafe {
        let raw = ctx.as_raw().as_ptr();
        let compiled = Value::from_raw(
            ctx.clone(),
            qjs::JS_Eval(
                raw,
                source.as_ptr(),
                code.len() as _,
                c"macroscope-user.js".as_ptr(),
                (qjs::JS_EVAL_TYPE_GLOBAL
                    | qjs::JS_EVAL_FLAG_STRICT
                    | qjs::JS_EVAL_FLAG_COMPILE_ONLY) as _,
            ),
        );
        if compiled.is_exception() {
            return Err(Error::Exception);
        }
        *compiling = false;
        let result = Value::from_raw(
            ctx.clone(),
            qjs::JS_EvalFunction(raw, qjs::JS_DupValue(raw, compiled.as_raw())),
        );
        if result.is_exception() {
            Err(Error::Exception)
        } else {
            Ok(())
        }
    }
}

pub(crate) fn promise_detail() -> String {
    format!("JavaScript execution failed: TypeError: run must be synchronous. {HINT}")
}

pub(crate) fn missing_run_detail() -> String {
    "JavaScript execution failed: TypeError: the program must define function run() { ... } and put all of its code inside it; top-level code is not a program.".into()
}

fn redact(message: &str) -> String {
    let mut out = String::new();
    let mut chars = message.chars().peekable();
    while let Some(c) = chars.next() {
        if matches!(c, '\'' | '"' | '`') {
            out.push('…');
            while let Some(next) = chars.next() {
                if next == '\\' {
                    chars.next();
                } else if next == c {
                    break;
                }
            }
        } else if c.is_numeric() {
            out.push('…');
            while chars
                .peek()
                .is_some_and(|c| c.is_numeric() || matches!(c, '.' | '+' | '-'))
            {
                chars.next();
            }
        } else if c.is_control() {
            out.push(' ');
        } else {
            out.push(c);
        }
    }
    out
}

fn source_identifier<'a>(message: &'a str, code: &str) -> Option<&'a str> {
    let id = message
        .strip_suffix(" is not defined")
        .or_else(|| message.strip_suffix(" is not initialized"))?;
    let mut chars = id.chars();
    if !chars
        .next()
        .is_some_and(|c| c.is_alphabetic() || matches!(c, '_' | '$'))
        || !chars.all(|c| c.is_alphanumeric() || matches!(c, '_' | '$'))
    {
        return None;
    }
    code.split(|c: char| !c.is_alphanumeric() && !matches!(c, '_' | '$'))
        .any(|word| word == id)
        .then_some(id)
}

pub(crate) fn error_detail(
    (name, message, stack, line, column, file): (
        String,
        String,
        String,
        Option<u32>,
        Option<u32>,
        String,
    ),
    code: &str,
    compiling: bool,
) -> String {
    let reference = (name == "ReferenceError")
        .then(|| source_identifier(&message, code))
        .flatten();
    let redacted = redact(&message);
    // Arbitrary Error(row.email), manually changed messages, and data-dependent
    // JSON parser errors cannot be safely identified by quotation alone. Retain
    // literal program text or exact, data-free engine templates; otherwise redact
    // the entire payload. Never echo unknown unquoted text.
    let safe_template = matches!(
        redacted.as_str(),
        "cannot read property … of undefined"
            | "cannot read property … of null"
            | "cannot set property … of undefined"
            | "cannot set property … of null"
            | "not a function"
            | "not a constructor"
            | "not an object"
            | "value is not iterable"
            | "invalid array length"
            | "invalid string length"
            | "Maximum call stack size exceeded"
            | "stack overflow"
            | "out of memory"
            | "interrupted"
            | "invalid property access"
            | "object is not extensible"
            | "cannot convert symbol to string"
            | "cannot convert symbol to number"
    );
    let text = if reference.is_some() || (name == "SyntaxError" && compiling) {
        message.clone()
    } else if !message.is_empty() && code.contains(&message) {
        if matches!(name.as_str(), "ReferenceError" | "SyntaxError") {
            message.clone()
        } else {
            redacted
        }
    } else if safe_template {
        redacted
    } else {
        "…".into()
    };
    let text: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(200)
        .collect();
    // Parse only the first user-script frame, emitting numeric positions only.
    // Function names and all other filenames/frames are deliberately discarded.
    let frame = stack.lines().find_map(|frame| {
        let position = frame.trim().strip_prefix("at ")?;
        let position = position.rsplit_once('(').map_or(position, |(_, p)| p);
        let position = position.trim_end_matches(')');
        let mut numbers = position
            .strip_prefix(&format!("{USER_SCRIPT}:"))?
            .split(':');
        let line = numbers.next()?.parse::<u32>().ok()?;
        let column = numbers.next().and_then(|s| s.parse::<u32>().ok());
        if numbers.next().is_some() {
            return None;
        }
        Some((line, column))
    });
    let location = if file == USER_SCRIPT {
        line.map(|line| (line, column)).or(frame)
    } else {
        frame
    };
    let location =
        location.filter(|(line, _)| *line > 0 && (*line as usize) <= code.lines().count());
    let mut detail = format!("JavaScript execution failed: {name}");
    if let Some((line, column)) = location {
        detail.push_str(&format!(" at line {line}"));
        if let Some(column) = column.filter(|c| *c > 0 && *c < 1000000000) {
            detail.push_str(&format!(", column {column}"));
        }
    }
    detail.push_str(&format!(": {text}"));
    // The engine's argument error is redacted, so name the one SDK call that has a
    // fixed two-string shape. The hint comes from program text alone, never from data.
    if name == "TypeError"
        && location.is_some_and(|(line, _)| {
            code.lines()
                .nth(line as usize - 1)
                .is_some_and(|l| l.contains("macroscope.output("))
        })
    {
        detail.push_str(". macroscope.output takes two strings: the output file name, then the text");
    }
    if reference.is_some_and(|id| {
        matches!(
            id,
            "Date"
                | "eval"
                | "Function"
                | "Worker"
                | "fetch"
                | "require"
                | "process"
                | "setTimeout"
                | "setInterval"
                | "setImmediate"
                | "queueMicrotask"
                | "Promise"
                | "Deno"
                | "Bun"
                | "console"
                | "window"
                | "document"
                | "SharedArrayBuffer"
                | "Atomics"
        )
    }) {
        detail.push_str(&format!(". {HINT}"));
    }
    detail
}
