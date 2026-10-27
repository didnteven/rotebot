use macroscope_builder::model_schema;
use serde_json::Value;

fn walk(v: &Value, f: &mut dyn FnMut(&serde_json::Map<String, Value>)) {
    match v {
        Value::Object(m) => {
            f(m);
            m.values().for_each(|x| walk(x, f));
        }
        Value::Array(a) => a.iter().for_each(|x| walk(x, f)),
        _ => {}
    }
}

/// The strict dialect Meta's structured outputs accepted on 2026-10-27 (each rule was a
/// real 400 before the fix).
#[test]
fn model_schema_uses_the_strict_dialect_meta_accepts() {
    let schema = model_schema();
    walk(&schema, &mut |m| {
        assert!(!m.contains_key("oneOf"), "oneOf is not permitted");
        assert!(!m.contains_key("default"), "default dropped");
        if let Some(f) = m.get("format").and_then(Value::as_str) {
            assert!(
                !f.starts_with("uint") && !f.starts_with("int"),
                "numeric format {f}"
            );
        }
        if let Some(Value::Object(props)) = m.get("properties") {
            let required: Vec<&str> = m["required"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect();
            for k in props.keys() {
                assert!(required.contains(&k.as_str()), "{k} must be required");
            }
        }
    });
    let text = schema.to_string();
    assert!(text.contains("\"anyOf\"") && text.contains("\"maximum\":255"));
    assert!(text.contains("formula_escape") && text.contains("stop_and_explain"));
}

/// Model output that includes every (formerly defaulted) field is still accepted by the host.
#[test]
fn host_accepts_model_output_that_now_includes_every_field() {
    let from_model = r#"{"formula_escape":false,"on_error":"stop_and_explain","output_columns":[{"name":"Order","source":"order_id","transform":{"type":"copy"}}],"skip_header_rows":0}"#;
    let mapping: macroscope_domain::CsvMapping = serde_json::from_str(from_model).unwrap();
    assert_eq!(mapping.output_columns[0].name, "Order");
}
