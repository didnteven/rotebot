use macroscope_domain::*;
use schemars::schema_for;
use serde_json::{json, Value};

fn manifest() -> Value {
    json!({
        "schema":"macroscope.tool/v1", "id":"tool_orders", "name":"Clean import CSV", "version":1,
        "runtime":{"engine":"csv-mapping", "pin":"foundation-placeholder", "sdk":"1.0", "entry":"mapping.json"},
        "platforms":["macos"],
        "inputs":{"type":"object", "additionalProperties":false, "required":["input_csv","output_root"], "properties":{
            "input_csv":{"type":"string", "format":"selected-file-handle"},
            "output_root":{"type":"string", "format":"selected-root-handle"}
        }},
        "triggers":[{"type":"manual"}],
        "capabilities":{
            "fileRead":[{"handle":"input_csv", "maxBytes":10000000}],
            "fileCreate":[{"root":"output_root", "namePattern":"*_ready.csv", "overwrite":false}],
            "fileMove":[], "fileTrash":[], "network":[], "clipboard":"none", "nativeApps":[], "secrets":[], "modelCalls":false
        },
        "limits":{"wallSeconds":120, "memoryMiB":256, "effects":1, "outputBytes":20000000},
        "execution":{"initialTrust":"supervised", "concurrency":1},
        "tests":{"suiteRef":"locked-suite-7", "fixtureDigest":"sha256:fixture-set"},
        "provenance":{"contractHash":"sha256:contract-v1", "buildRef":"build-7", "model":"muse-spark-1.3"},
        "integrity":{"treeHash":"sha256:published-tree", "approvalRef":"approval-7"}
    })
}

#[test]
fn committed_schemas_match_generated_contracts() {
    for (committed, generated) in [
        (
            include_str!("../../../contracts/task-contract.schema.json"),
            schema_for!(TaskContract),
        ),
        (
            include_str!("../../../contracts/csv-mapping.schema.json"),
            schema_for!(CsvMapping),
        ),
        (
            include_str!("../../../contracts/tool-manifest.schema.json"),
            schema_for!(ToolManifest),
        ),
    ] {
        assert_eq!(
            serde_json::from_str::<Value>(committed).unwrap(),
            serde_json::to_value(generated).unwrap(),
            "run cargo run -p macroscope-domain --example export_schemas"
        );
    }
}

#[test]
fn schemas_validate_real_serialized_contracts() {
    let task: Value =
        serde_json::from_str(include_str!("../../../fixtures/orders/task.json")).unwrap();
    for (schema, value) in [
        (schema_for!(TaskContract), task.clone()),
        (schema_for!(CsvMapping), task["mapping"].clone()),
        (schema_for!(ToolManifest), manifest()),
    ] {
        jsonschema::validator_for(&serde_json::to_value(schema).unwrap())
            .unwrap()
            .validate(&value)
            .unwrap();
    }
}

#[test]
fn tool_manifest_supports_both_engines_and_explicit_mcp_capabilities() {
    let mut value = manifest();
    let tool: ToolManifest = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(tool.runtime.engine, RuntimeEngine::CsvMapping);
    assert!(tool.capabilities.mcp.is_empty());
    value["runtime"]["engine"] = "deno".into();
    value["capabilities"]["mcp"] = json!([{"connectorId":"gmail", "toolNames":["messages.list"]}]);
    let tool: ToolManifest = serde_json::from_value(value).unwrap();
    assert_eq!(tool.runtime.engine, RuntimeEngine::Deno);
    assert_eq!(tool.capabilities.mcp[0].connector_id.0, "gmail");
    let roundtrip = serde_json::to_value(&tool).unwrap();
    assert_eq!(
        serde_json::from_value::<ToolManifest>(roundtrip.clone()).unwrap(),
        tool
    );
    jsonschema::validator_for(&serde_json::to_value(schema_for!(ToolManifest)).unwrap())
        .unwrap()
        .validate(&roundtrip)
        .unwrap();
}

#[test]
fn schema_and_serde_reject_embedded_mapping_code_and_unknown_manifest_versions() {
    let schema = serde_json::to_value(schema_for!(CsvMapping)).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    let mut mapping: Value =
        serde_json::from_str::<Value>(include_str!("../../../fixtures/orders/task.json")).unwrap()
            ["mapping"]
            .clone();
    mapping["output_columns"][0]["transform"]["expression"] = "process.env".into();
    assert!(!validator.is_valid(&mapping));
    assert!(serde_json::from_value::<CsvMapping>(mapping).is_err());
    let mut tool = manifest();
    tool["schema"] = "macroscope.tool/v99".into();
    assert!(serde_json::from_value::<ToolManifest>(tool.clone()).is_err());
    assert!(
        !jsonschema::validator_for(&serde_json::to_value(schema_for!(ToolManifest)).unwrap())
            .unwrap()
            .is_valid(&tool)
    );
}
