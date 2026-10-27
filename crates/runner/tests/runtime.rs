use macroscope_runner::{Limits, Request, Response, MEMORY_CAP};
use std::{
    collections::BTreeMap,
    io::Write,
    process::{Command, Stdio},
};
fn run(code: &str, limits: Limits) -> Response {
    macroscope_runner::execute(Request {
        code: code.into(),
        inputs: BTreeMap::new(),
        limits,
    })
}
fn success(code: &str) {
    let response = run(code, Limits::default());
    assert!(matches!(response, Response::Success { .. }), "{response:?}");
}
fn kind(code: &str, limits: Limits, expected: &str) {
    match run(code, limits) {
        Response::Error { error } => assert_eq!(error.kind, expected, "{error:?}"),
        other => panic!("{other:?}"),
    }
}
#[test]
fn sdk_orders_matches_independent_fixture() {
    let request = Request {
        code: include_str!("../../../experiments/runtime-gate-v2/orders.js").into(),
        inputs: BTreeMap::from([(
            "input_csv".into(),
            include_str!("../../../fixtures/orders/input.csv").into(),
        )]),
        limits: Limits::default(),
    };
    match macroscope_runner::execute(request) {
        Response::Success { outputs, log } => {
            assert_eq!(outputs.len(), 1);
            assert_eq!(
                outputs[0].text.as_bytes(),
                include_bytes!("../../../fixtures/orders/expected.csv")
            );
            assert_eq!(log, ["rows: 18"]);
        }
        other => panic!("{other:?}"),
    }
}
#[test]
fn sdk_validation_decimal_and_csv() {
    success(
        r#"function run() {
        if (macroscope.money.decimal("$9007199254740993.125", {currency:"USD", decimals:2}) !== "9007199254740993.13") throw 1;
        if (macroscope.money.decimal("($1.235)", {currency:"USD"}) !== "-1.24") throw 2;
        if (macroscope.date.toISO("02/29/24", "MM/DD/YY") !== "2024-02-29") throw 3;
        const rows = macroscope.csv.parse('a,b\n"x,y","q""r"\n', {required:["a","b"]});
        if (rows[0].a !== "x,y" || rows[0].b !== 'q"r') throw 4;
        if (macroscope.csv.stringify([{x:"=1+1"}], ["x"]) !== "x\n'=1+1\n") throw 5;
    }"#,
    );
    for code in [
        r"function run(){macroscope.csv.parse('a\n1\n', {required:['missing']})}",
        "function run(){macroscope.money.decimal('1.2.3', {currency:'USD'})}",
        "function run(){macroscope.money.decimal(1.25, {currency:'USD'})}",
        "function run(){macroscope.money.decimal('1.25', {currency:'USD',decimals:256})}",
        "function run(){macroscope.money.decimal('1.25', {currency:'USD',decimals:1.5})}",
        "function run(){macroscope.date.toISO('02/29/23','MM/DD/YY')}",
        r"function run(){macroscope.csv.parse('a,a\n1,2\n')}",
    ] {
        kind(code, Limits::default(), "runtime");
    }
}
#[test]
fn forbidden_globals_and_all_constructor_chains() {
    success(
        r#"function run(){
        for (const n of ['eval','Function','Worker','Deno','require','process','fetch','setTimeout','setInterval','console','localStorage','caches','indexedDB','SharedArrayBuffer','Atomics']) {
            if (typeof globalThis[n] !== 'undefined') throw n;
        }
        for (const f of [function(){}, async function(){}, function*(){}, async function*(){}, macroscope.input]) {
            if (f.constructor !== undefined) throw 'constructor';
        }
        if (!Object.isFrozen(macroscope) || !Object.isFrozen(macroscope.csv) || !Object.isFrozen(macroscope.output)) throw 'not frozen';
    }"#,
    );
    for code in [
        "function run(){Function('return 42')()}",
        "function run(){new Worker('x')}",
        "function run(){eval('1')}",
    ] {
        kind(code, Limits::default(), "runtime");
    }
    success("function run(){try{macroscope.csv.parse=1}catch(e){}; try{macroscope.__proto__={x:1}}catch(e){}; if(typeof macroscope.csv.parse!=='function'||macroscope.x)throw 1;}");
    for code in [
        "function run(){import('data:text/javascript,globalThis.escaped=1')}",
        "import x from '/etc/hosts'; function run(){}",
    ] {
        kind(code, Limits::default(), "syntax");
    }
}
#[test]
fn cpu_wall_stack_and_output_limits_are_sticky() {
    kind(
        "function run(){for(;;){}}",
        Limits {
            cpu_ms: 30,
            ..Limits::default()
        },
        "cpu",
    );
    kind(
        "function run(){for(;;){}}",
        Limits {
            wall_ms: 20,
            ..Limits::default()
        },
        "wall",
    );
    kind("function run(){run()}", Limits::default(), "runtime");
    kind(
        "function run(){try{macroscope.output('../x.csv','x')}catch(e){}}",
        Limits::default(),
        "forbidden",
    );
    for code in [
        "function run(){macroscope.output('x.csv','x'.repeat(1001))}",
        "function run(){macroscope.output('x.csv','x');macroscope.output('y.csv','y')}",
        "function run(){for(;;)macroscope.log('')}",
    ] {
        kind(
            code,
            Limits {
                output_bytes: 1000,
                log_bytes: 1000,
                ..Limits::default()
            },
            "output_limit",
        );
    }
    kind(
        "function run(){macroscope.input('/etc/hosts')}",
        Limits::default(),
        "forbidden",
    );
}
#[test]
#[allow(
    clippy::zombie_processes,
    reason = "wait4 below reaps the child and collects its own kernel peak RSS"
)]
fn child_memory_limit_and_kernel_peak_rss() {
    let request = Request {
        code: "function run(){const a=[];for(;;)a.push(new Uint8Array(1024*1024).fill(1));}".into(),
        inputs: BTreeMap::new(),
        limits: Limits::default(),
    };
    let mut child = Command::new(env!("CARGO_BIN_EXE_macroscope-runner"))
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&request).unwrap())
        .unwrap();
    let mut status = 0;
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    assert_eq!(
        unsafe { libc::wait4(child.id() as i32, &mut status, 0, &mut usage) },
        child.id() as i32
    );
    assert_eq!(status, 0);
    let mut bytes = vec![];
    std::io::Read::read_to_end(&mut child.stdout.take().unwrap(), &mut bytes).unwrap();
    let response = Response::from_wire(&bytes).unwrap();
    assert!(matches!(response, Response::Error {error} if error.kind=="memory"));
    #[cfg(target_os = "macos")]
    let peak = usage.ru_maxrss as usize;
    #[cfg(not(target_os = "macos"))]
    let peak = usage.ru_maxrss as usize * 1024;
    let slack = 16 * 1024 * 1024;
    eprintln!(
        "kernel peak RSS={peak}; heap cap={MEMORY_CAP}; slack={slack}; overshoot={}",
        peak.saturating_sub(MEMORY_CAP)
    );
    assert!(peak <= MEMORY_CAP + slack, "peak RSS {peak}");
}

#[test]
fn protocol_limits_syntax_and_hostile_exception_values() {
    kind("function run( {", Limits::default(), "syntax");
    kind("const x=1", Limits::default(), "runtime");
    kind("async function run(){}", Limits::default(), "runtime");
    kind(
        "function run(){}",
        Limits {
            memory_bytes: MEMORY_CAP + 1,
            ..Limits::default()
        },
        "forbidden",
    );
    kind(
        "function run(){throw Object.defineProperty({},'message',{get(){for(;;){}}});}",
        Limits::default(),
        "runtime",
    );
    success("function run(){const rows=macroscope.csv.parse('__proto__,constructor\\nx,y\\n'); if(rows[0].__proto__!=='x'||rows[0].constructor!=='y'||Object.getPrototypeOf(rows[0])!==null)throw 1;}");
    let response = Response::Success {
        outputs: vec![macroscope_runner::Output {
            name: "x.csv".into(),
            text: "\0".repeat(4_000_000),
        }],
        log: vec![],
    };
    let bytes = macroscope_runner::encode_response(&response);
    assert!(bytes.len() < macroscope_runner::OUTPUT_CAP);
    assert!(
        matches!(Response::from_wire(&bytes).unwrap(), Response::Error {error} if error.kind=="output_limit")
    );
    let mut child = Command::new(env!("CARGO_BIN_EXE_macroscope-runner"))
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"{}\n{}\n").unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(result.status.success());
    assert!(
        matches!(Response::from_wire(&result.stdout).unwrap(),Response::Error {error} if error.kind=="syntax")
    );
}

#[test]
fn clock_and_randomness_are_unavailable_and_text_output_formats_work() {
    success("function run(){ if(typeof Date !== 'undefined' || typeof Math.random !== 'undefined') throw 1; }");
    for ext in ["csv", "tsv", "txt", "md", "json"] {
        success(&format!(
            "function run(){{macroscope.output('result.{ext}','synthetic')}}"
        ));
    }
}
