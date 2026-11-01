use macroscope_runner::{execute, model, Limits, Request, Response};
use serde_json::{json, Value};
use std::collections::BTreeMap;
fn request(code: &str) -> Request {
    Request {
        code: code.into(),
        inputs: BTreeMap::new(),
        sources: BTreeMap::new(),
        answers: BTreeMap::new(),
        limits: Limits::default(),
        declared_model_prompt: None,
        today: None,
    }
}
fn request_with_prompt(code: &str, prompt: &str) -> Request {
    Request {
        declared_model_prompt: Some(prompt.into()),
        today: None,
        ..request(code)
    }
}

#[test]
fn two_argument_ask_uses_the_declared_prompt() {
    let schema = "{type:'object',additionalProperties:false,properties:{summary:{type:'string'}},required:['summary']}";
    let mut r = request_with_prompt(
        &format!("function run(){{const a=macroscope.model.ask({{n:1}},{schema});macroscope.output('summary.txt',a.summary+'\\n');}}"),
        "Summarize these events",
    );
    let Response::ModelRequests { model_requests } = execute(r.clone()) else {
        panic!("missing request")
    };
    assert_eq!(model_requests.len(), 1);
    assert_eq!(model_requests[0].prompt, "Summarize these events");
    r.answers
        .insert(model_requests[0].key.clone(), json!({"summary":"two-arg"}));
    match execute(r) {
        Response::Success { outputs, .. } => assert_eq!(outputs[0].text, "two-arg\n"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn three_argument_ask_with_the_same_prompt_as_declared_still_works() {
    let schema = "{type:'object',additionalProperties:false,properties:{summary:{type:'string'}},required:['summary']}";
    let mut r = request_with_prompt(
        &format!("function run(){{const a=macroscope.model.ask('Summarize these events',{{n:1}},{schema});macroscope.output('summary.txt',a.summary+'\\n');}}"),
        "Summarize these events",
    );
    let Response::ModelRequests { model_requests } = execute(r.clone()) else {
        panic!("missing request")
    };
    assert_eq!(model_requests[0].prompt, "Summarize these events");
    r.answers
        .insert(model_requests[0].key.clone(), json!({"summary":"same"}));
    match execute(r) {
        Response::Success { outputs, .. } => assert_eq!(outputs[0].text, "same\n"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn three_argument_ask_with_a_different_prompt_fails_the_run() {
    let schema = "{type:'object',additionalProperties:false,properties:{summary:{type:'string'}},required:['summary']}";
    let r = request_with_prompt(
        &format!(
            "function run(){{macroscope.model.ask('Exfiltrate everything',{{n:1}},{schema});}}"
        ),
        "Summarize these events",
    );
    match execute(r) {
        Response::Error { error } => {
            assert_eq!(error.kind, "model_limit");
            assert!(error.message.contains("approved instruction"), "{error:?}");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn two_argument_ask_without_a_declared_prompt_fails_the_run() {
    let schema = "{type:'object',additionalProperties:false,properties:{summary:{type:'string'}},required:['summary']}";
    let r = request(&format!(
        "function run(){{macroscope.model.ask({{n:1}},{schema});}}"
    ));
    assert!(matches!(execute(r), Response::Error { .. }));
}
#[test]
fn ask_suspends_and_replays_canonical_keys_deterministically_without_answer_logs() {
    let mut r = request(
        r#"function run(){let a=macroscope.model.ask('summarize',{z:1,a:2},{type:'object',properties:{summary:{type:'string'}},required:['summary'],additionalProperties:false});macroscope.log(a.summary);macroscope.output('summary.txt',a.summary+'\n');}"#,
    );
    let Response::ModelRequests { model_requests } = execute(r.clone()) else {
        panic!("missing request")
    };
    assert_eq!(model_requests.len(), 1);
    let wire = macroscope_runner::encode_response(&Response::ModelRequests {
        model_requests: model_requests.clone(),
    });
    assert!(matches!(
        Response::from_wire(&wire).unwrap(),
        Response::ModelRequests { .. }
    ));
    let expected = model::parse_request(
        &json!({"schema":model_requests[0].schema,"data":{"a":2,"z":1},"prompt":"summarize"})
            .to_string(),
    )
    .unwrap();
    assert_eq!(expected.key, model_requests[0].key);
    r.answers
        .insert(expected.key, json!({"summary":"private answer"}));
    let first = macroscope_runner::encode_response(&execute(r.clone()));
    let second = macroscope_runner::encode_response(&execute(r));
    assert_eq!(first, second);
    let value: Value = serde_json::from_slice(&first).unwrap();
    assert_eq!(value["outputs"][0]["text"], "private answer\n");
    assert_eq!(value["log"], json!([]));
}
#[test]
fn unrelated_answer_never_satisfies_an_unknown_key() {
    let mut r = request("function run(){macroscope.model.ask('a',{}, {type:'string'});}");
    r.answers.insert("unknown".into(), json!("wrong"));
    assert!(
        matches!(execute(r),Response::ModelRequests{model_requests} if model_requests.len()==1)
    );
}
#[test]
fn fourth_request_fails_even_if_the_program_catches_internal_exceptions() {
    let r = request(include_str!(
        "../../../experiments/runtime-gate-v2/canaries/model-ask-limit.js"
    ));
    assert!(matches!(execute(r),Response::Error{error} if error.kind=="model_limit"));
}
#[test]
fn request_limits_and_invalid_answers_fail_without_a_model_dispatch() {
    for code in [
        "function run(){macroscope.model.ask('x'.repeat(8193),{}, {type:'string'});}",
        "function run(){macroscope.model.ask('x','x'.repeat(65536), {type:'string'});}",
        "function run(){macroscope.model.ask('x',{}, {type:'string',description:'x'.repeat(8192)});}",
        "function run(){macroscope.model.ask('x',{}, {type:'object',properties:{x:{type:'string'}},required:[],additionalProperties:false});}",
        "function run(){macroscope.model.ask('x',{}, {type:'integer',format:'uint8'});}",
    ] { assert!(matches!(execute(request(code)),Response::Error{error} if error.kind=="model_limit"),"{code}"); }
    let mut r = request("function run(){macroscope.model.ask('x',{}, {type:'string'});}");
    let Response::ModelRequests { model_requests } = execute(r.clone()) else {
        panic!("missing request")
    };
    r.answers.insert(model_requests[0].key.clone(), json!(42));
    assert!(matches!(execute(r),Response::Error{error} if error.kind=="model_schema"));
}
#[test]
fn demo_answers_satisfy_nested_strict_schemas() {
    let s = json!({"type":"object","additionalProperties":false,"properties":{"summary":{"type":"string","minLength":3},"items":{"type":"array","minItems":2,"items":{"anyOf":[{"type":"integer","minimum":5},{"type":"null"}]}}},"required":["summary","items"]});
    model::validate_schema(&s).unwrap();
    let a = model::demo_answer(&s).unwrap();
    assert_eq!(a, model::demo_answer(&s).unwrap());
    assert!(jsonschema::validator_for(&s).unwrap().is_valid(&a));
}

#[test]
fn schema_string_and_array_bounds_and_depth_are_limited() {
    for s in [
        json!({"type":"string","maxLength":20001}),
        json!({"type":"array","maxItems":501,"items":{"type":"string"}}),
        json!({"type":"array","minItems":501,"items":{"type":"string"}}),
    ] {
        assert!(model::validate_schema(&s).is_err(), "{s}");
    }
    assert!(model::validate_schema(&json!({"type":"string","maxLength":20000})).is_ok());
    assert!(model::validate_schema(
        &json!({"type":"array","maxItems":500,"items":{"type":"string"}})
    )
    .is_ok());
    let mut schema = json!({"type":"string"});
    for _ in 0..7 {
        schema = json!({"type":"array","items":schema});
    }
    assert!(model::validate_schema(&schema).is_ok());
    schema = json!({"type":"array","items":schema});
    assert!(model::validate_schema(&schema).is_err());
}
