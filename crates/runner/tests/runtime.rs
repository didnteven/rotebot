use macroscope_runner::{Limits, Request, Response, MEMORY_CAP};
use std::{
    collections::BTreeMap,
    io::Write,
    process::{Command, Stdio},
};
fn run(code: &str, limits: Limits) -> Response {
    macroscope_runner::execute(Request {
        answers: BTreeMap::new(),
        code: code.into(),
        inputs: BTreeMap::new(),
        sources: BTreeMap::new(),
        limits,
        declared_model_prompt: None,
    })
}
fn success(code: &str) {
    let response = run(code, Limits::default());
    assert!(
        matches!(response, Response::Success { .. }),
        "{code}: {response:?}"
    );
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
        answers: BTreeMap::new(),
        code: include_str!("../../../experiments/runtime-gate-v2/orders.js").into(),
        inputs: BTreeMap::from([(
            "input_csv".into(),
            include_str!("../../../fixtures/orders/input.csv").into(),
        )]),
        sources: BTreeMap::new(),
        limits: Limits::default(),
        declared_model_prompt: None,
    };
    match macroscope_runner::execute(request) {
        Response::Success { outputs, log, .. } => {
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
        answers: BTreeMap::new(),
        code: "function run(){const a=[];for(;;)a.push(new Uint8Array(1024*1024).fill(1));}".into(),
        inputs: BTreeMap::new(),
        sources: BTreeMap::new(),
        limits: Limits::default(),
        declared_model_prompt: None,
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
fn object_allocation_exhaustion_reports_memory() {
    kind(
        include_str!("../../../experiments/runtime-gate-v2/canaries/memory-objects.js"),
        Limits::default(),
        "memory",
    );
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
        planned_calls: vec![],
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
fn math_helpers_are_available_but_random_is_removed() {
    success("function run(){ if(Math.round(19.7) !== 20) throw 1; if(Math.max(1, 2) !== 2) throw 2; if(typeof Math.random !== 'undefined') throw 3; }");
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
#[test]
fn sdk_source_returns_host_fetched_text_and_rejects_unknown_names() {
    let request = Request {
        answers: BTreeMap::new(),
        code:
            "function run(){macroscope.output(\"weather.txt\", macroscope.source(\"forecast\"));}"
                .into(),
        inputs: BTreeMap::new(),
        sources: BTreeMap::from([("forecast".into(), "sunny".into())]),
        limits: Limits::default(),
        declared_model_prompt: None,
    };
    match macroscope_runner::execute(request) {
        Response::Success { outputs, .. } => {
            assert_eq!(outputs.len(), 1);
            assert_eq!(outputs[0].text, "sunny");
        }
        other => panic!("{other:?}"),
    }
    let bad = Request {
        answers: BTreeMap::new(),
        code: "function run(){macroscope.output(\"weather.txt\", macroscope.source(\"nope\"));}"
            .into(),
        inputs: BTreeMap::new(),
        sources: BTreeMap::new(),
        limits: Limits::default(),
        declared_model_prompt: None,
    };
    match macroscope_runner::execute(bad) {
        Response::Error { error } => assert_eq!(error.kind, "forbidden"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn connection_requests_are_bounded_json_snapshots_and_return_no_network_data() {
    let result = run(
        r#"function run(){
        const args = {to:'a@example.invalid', nested:{text:'<script>hello</script>'}};
        if(macroscope.connection.request('gmail','create_draft',args) !== undefined) throw 'network result';
        args.nested.text = 'changed';
    }"#,
        Limits::default(),
    );
    let Response::Success { planned_calls, .. } = result else {
        panic!("{result:?}")
    };
    assert_eq!(planned_calls.len(), 1);
    assert_eq!(
        planned_calls[0].args["nested"]["text"],
        "<script>hello</script>"
    );
    for args in [
        "undefined",
        "{x:undefined}",
        "{x:()=>1}",
        "{x:NaN}",
        "{x:Infinity}",
        "{x:1n}",
        "{get x(){return 1}}",
        "{x:[,1]}",
        "{toJSON(){return {}}}",
    ] {
        let code = format!(
            "function run(){{macroscope.connection.request('gmail','create_draft',{args})}}"
        );
        assert!(
            matches!(run(&code, Limits::default()), Response::Error { .. }),
            "{args}"
        );
    }
    kind("function run(){macroscope.connection.request('gmail','create_draft',{body:'x'.repeat(65536)})}", Limits::default(), "forbidden");
    kind("function run(){for(let i=0;i<6;i++)try{macroscope.connection.request('gmail','create_draft',{})}catch(e){}}", Limits::default(), "forbidden");
    let result = run("function run(){for(let i=0;i<5;i++)macroscope.connection.request('gmail','create_draft',{})}", Limits::default());
    let bytes = macroscope_runner::encode_response(&result);
    assert!(
        matches!(Response::from_wire(&bytes).unwrap(), Response::Success {planned_calls,..} if planned_calls.len()==5)
    );
}

fn failure(code: &str) -> macroscope_runner::Failure {
    match run(code, Limits::default()) {
        Response::Error { error } => error,
        other => panic!("expected failure: {other:?}"),
    }
}

#[test]
fn runtime_details_keep_program_identifiers_and_user_positions() {
    let error = failure("function run() {\n  const rows = [];\n  groupBy(rows);\n}");
    assert_eq!(error.kind, "runtime");
    assert!(error.message.contains(", column "), "{error:?}");
    assert!(
        error.message.contains("ReferenceError at line 3"),
        "{error:?}"
    );
    assert!(
        error.message.contains("groupBy is not defined"),
        "{error:?}"
    );
    assert!(!error.message.contains("synchronous"), "{error:?}");
    assert!(!error.message.contains(".js"), "{error:?}");
    let error = failure("function run() {\n  const x = ;\n}");
    assert_eq!(error.kind, "syntax");
    assert!(error.message.contains("SyntaxError at line 2"), "{error:?}");
    assert!(error.message.contains("unexpected token"), "{error:?}");
    let error = failure("function run(){\n  return Date.now();\n}");
    assert!(
        error.message.contains("ReferenceError at line 2"),
        "{error:?}"
    );
    assert!(
        error.message.contains("Use synchronous SDK code only"),
        "{error:?}"
    );
}

#[test]
fn runtime_details_redact_quoted_numeric_and_arbitrary_data() {
    let error = failure("function run(){\n  let row;\n  row.trim();\n}");
    assert!(error.message.contains("TypeError at line 3"), "{error:?}");
    assert!(
        error
            .message
            .contains("cannot read property … of undefined"),
        "{error:?}"
    );
    assert!(!error.message.contains("trim"), "{error:?}");
    for constructor in ["Error", "TypeError", "RangeError", "InternalError"] {
        let code = format!(
            r#"function run(){{throw new {constructor}("contains 123 and 'x@example.invalid'");}}"#
        );
        let error = failure(&code);
        assert!(error.message.contains("contains … and …"), "{error:?}");
        for secret in ["123", "x@example.invalid"] {
            assert!(!error.message.contains(secret), "{error:?}");
        }
    }
    let error = failure(r#"function run(){throw "secret-value";}"#);
    assert_eq!(
        error.message,
        "JavaScript execution failed: a non-Error value was thrown"
    );
    for expression in [
        "new Error(data)",
        "new TypeError(data)",
        "new ReferenceError(data)",
        "new SyntaxError(data)",
        "JSON.parse(data)",
        "Object.assign(new Error(), {message:data, name:data, stack:data})",
        "Object.assign(new TypeError(), {message:data})",
    ] {
        let response = macroscope_runner::execute(Request {
            code: format!(
                "function run(){{const data=macroscope.input('input_csv');throw {expression};}}"
            ),
            inputs: BTreeMap::from([(
                "input_csv".into(),
                "private-address@example.invalid 98765".into(),
            )]),
            sources: BTreeMap::new(),
            answers: BTreeMap::new(),
            limits: Limits::default(),
            declared_model_prompt: None,
        });
        let Response::Error { error } = response else {
            panic!("{response:?}");
        };
        for secret in ["private-address", "example.invalid", "98765"] {
            assert!(!error.message.contains(secret), "{expression}: {error:?}");
        }
    }
    let response = macroscope_runner::execute(Request {
        code: "throw new SyntaxError(macroscope.source('secret')); function run(){}".into(),
        inputs: BTreeMap::new(),
        sources: BTreeMap::from([("secret".into(), "FETCHED_SECRET_TOP_LEVEL".into())]),
        answers: BTreeMap::new(),
        limits: Limits::default(),
        declared_model_prompt: None,
    });
    let Response::Error { error } = response else {
        panic!("{response:?}");
    };
    assert_eq!(error.kind, "syntax");
    assert!(
        !error.message.contains("FETCHED_SECRET_TOP_LEVEL"),
        "{error:?}"
    );
}

#[test]
fn runtime_details_are_bounded_and_ignore_hostile_getters_and_stack_paths() {
    let code = format!("function run(){{throw new Error('{}');}}", "x".repeat(500));
    let error = failure(&code);
    assert_eq!(
        error.message.split(": ").last().unwrap().chars().count(),
        200
    );
    for key in [
        "name",
        "message",
        "stack",
        "lineNumber",
        "columnNumber",
        "fileName",
    ] {
        let error = failure(&format!("function run(){{throw Object.defineProperty(new Error(),'{key}',{{get(){{for(;;){{}}}}}});}}"));
        assert_eq!(error.kind, "runtime", "{error:?}");
    }
    let error = failure("function run(){const error=Object.defineProperty(new Error(),'message',{get(){for(;;){}}}); Object.defineProperty(Object.prototype,'value',{get(){for(;;){}}}); throw error;}");
    assert_eq!(error.kind, "runtime", "{error:?}");
    let error = failure("function run(){throw Object.assign(new Error(), {stack:'    at secret (/private/host/secret.js:8:9)\\n    at run (macroscope-user.js:1:16)', name:'/private/host/name', fileName:'/private/host/secret.js', lineNumber:8});}");
    assert!(
        error.message.contains("Error at line 1, column 16"),
        "{error:?}"
    );
    assert!(!error.message.contains("/private"), "{error:?}");
}

#[test]
fn returned_promise_explains_synchronous_requirement() {
    let error = failure("async function run(){}");
    assert_eq!(error.kind, "runtime");
    assert!(
        error.message.contains("run must be synchronous"),
        "{error:?}"
    );
    assert!(
        error.message.contains("Use synchronous SDK code only"),
        "{error:?}"
    );
}

#[test]
fn packet_builtin_claims_match_the_sandbox() {
    // These exact lists also appear in the builder packet's SDK reference.
    let available = [
        "Array",
        "Object",
        "Map",
        "Set",
        "JSON",
        "String",
        "Number",
        "Boolean",
        "BigInt",
        "Math",
        "Error",
        "TypeError",
        "ReferenceError",
        "SyntaxError",
        "RangeError",
        "InternalError",
    ];
    let unavailable = [
        "Date",
        "eval",
        "Function",
        "Worker",
        "Promise",
        "setTimeout",
        "setInterval",
        "setImmediate",
        "queueMicrotask",
        "fetch",
        "require",
        "process",
        "RegExp",
    ];
    let sdk = include_str!("../SDK.md");
    for name in available {
        assert!(
            sdk.contains(&format!("`{name}`")),
            "SDK must document {name}"
        );
        success(&format!(
            "function run(){{if(typeof globalThis['{name}']==='undefined')throw '{name}';}}"
        ));
    }
    for name in unavailable {
        assert!(
            sdk.contains(&format!("`{name}`")),
            "SDK must document {name}"
        );
        success(&format!(
            "function run(){{if(typeof globalThis['{name}']!=='undefined')throw '{name}';}}"
        ));
    }
    // QuickJS-NG supplies groupBy even though it is newer than ES2023.
    for name in ["Object.groupBy", "Map.groupBy"] {
        assert!(
            sdk.contains(&format!("`{name}`")),
            "SDK must document {name}"
        );
        success(&format!(
            "function run(){{if(typeof {name}!=='function')throw '{name}';}}"
        ));
    }
    success("function run(){if(typeof Math.random!=='undefined')throw 'random'; if(Object.groupBy([1,2,3],x=>x%2)[1].length!==2)throw 'object grouping'; if(Map.groupBy([1,2,3],x=>x%2).get(1).length!==2)throw 'map grouping';}");
}
