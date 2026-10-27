//! Data-only replay protocol and the structured-output dialect accepted by the host.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelRequest {
    pub key: String,
    pub prompt: String,
    pub data: Value,
    pub schema: Value,
}
pub fn parse_request(text: &str) -> Result<ModelRequest, String> {
    if text.len() > 90_112 {
        return Err("Model request too large".into());
    }
    let v: Value = serde_json::from_str(text).map_err(|_| "Model request must be JSON")?;
    let prompt = v["prompt"].as_str().ok_or("Model prompt must be text")?;
    let data = v.get("data").ok_or("Model data missing")?;
    let schema = v.get("schema").ok_or("Model schema missing")?;
    if prompt.len() > 8192 || data.to_string().len() > 65_536 || schema.to_string().len() > 8192 {
        return Err("Model request exceeds limits".into());
    }
    validate_schema(schema)?;
    let key = format!(
        "{:x}",
        Sha256::digest(canonical_json(
            &json!({"prompt":prompt,"data":data,"schema":schema})
        ))
    );
    Ok(ModelRequest {
        key,
        prompt: prompt.into(),
        data: data.clone(),
        schema: schema.clone(),
    })
}
pub fn validate_schema(s: &Value) -> Result<(), String> {
    fn visit(s: &Value, depth: usize) -> Result<(), String> {
        if depth > 32 {
            return Err("Schema too deep".into());
        }
        let m = s.as_object().ok_or("Schema must be an object")?;
        const ALLOWED: &[&str] = &[
            "type",
            "properties",
            "required",
            "additionalProperties",
            "items",
            "anyOf",
            "enum",
            "const",
            "description",
            "minimum",
            "maximum",
            "minItems",
            "maxItems",
            "minLength",
            "maxLength",
            "title",
        ];
        if m.keys().any(|k| !ALLOWED.contains(&k.as_str())) {
            return Err("Unsupported strict schema keyword".into());
        }
        if let Some(variants) = m.get("anyOf") {
            for v in variants.as_array().ok_or("anyOf must be an array")? {
                visit(v, depth + 1)?;
            }
        } else {
            let t = s["type"].as_str().ok_or("Schema needs a type")?;
            match t {
                "object" => {
                    let props = s["properties"]
                        .as_object()
                        .ok_or("Object needs properties")?;
                    let required = s["required"].as_array().ok_or("Object needs required")?;
                    if s["additionalProperties"] != false
                        || required.len() != props.len()
                        || !props.keys().all(|k| required.contains(&json!(k)))
                    {
                        return Err(
                            "Every property must be required; additionalProperties must be false"
                                .into(),
                        );
                    }
                    for v in props.values() {
                        visit(v, depth + 1)?;
                    }
                }
                "array" => visit(&s["items"], depth + 1)?,
                "string" | "number" | "integer" | "boolean" | "null" => {}
                _ => return Err("Unsupported schema type".into()),
            }
        }
        Ok(())
    }
    visit(s, 0)?;
    jsonschema::validator_for(s).map_err(|_| "Invalid model schema".to_string())?;
    Ok(())
}
/// Deterministic offline answer. Validation below refuses unsupported constraints.
pub fn demo_answer(s: &Value) -> Result<Value, String> {
    let v = if let Some(v) = s.get("const") {
        v.clone()
    } else if let Some(v) = s["enum"].as_array().and_then(|v| v.first()) {
        v.clone()
    } else if let Some(v) = s["anyOf"].as_array().and_then(|v| v.first()) {
        demo_answer(v)?
    } else {
        match s["type"].as_str() {
            Some("object") => {
                let mut m = serde_json::Map::new();
                for (k, v) in s["properties"].as_object().ok_or("Missing properties")? {
                    m.insert(k.clone(), demo_answer(v)?);
                }
                Value::Object(m)
            }
            Some("array") => {
                let n = s["minItems"].as_u64().unwrap_or(0);
                if n > 100 {
                    return Err("Demo array too large".into());
                }
                Value::Array(
                    (0..n)
                        .map(|_| demo_answer(&s["items"]))
                        .collect::<Result<_, _>>()?,
                )
            }
            Some("string") => {
                let n = s["minLength"].as_u64().unwrap_or(0);
                if n > 8192 {
                    return Err("Demo string too large".into());
                }
                json!("x".repeat(n as usize))
            }
            Some("integer" | "number") => s.get("minimum").cloned().unwrap_or(json!(0)),
            Some("boolean") => json!(false),
            Some("null") => Value::Null,
            _ => return Err("Unsupported demo schema".into()),
        }
    };
    if !jsonschema::validator_for(s).is_ok_and(|s| s.is_valid(&v)) {
        return Err("Demo cannot satisfy schema".into());
    }
    Ok(v)
}

/// Stable even when another workspace crate enables serde_json/preserve_order.
pub fn canonical_json(value: &Value) -> String {
    match value {
        Value::Object(map) => {
            let sorted: std::collections::BTreeMap<_, _> = map.iter().collect();
            format!(
                "{{{}}}",
                sorted
                    .into_iter()
                    .map(|(k, v)| format!(
                        "{}:{}",
                        serde_json::to_string(k).expect("JSON key"),
                        canonical_json(v)
                    ))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }
        Value::Array(values) => format!(
            "[{}]",
            values
                .iter()
                .map(canonical_json)
                .collect::<Vec<_>>()
                .join(",")
        ),
        _ => value.to_string(),
    }
}
