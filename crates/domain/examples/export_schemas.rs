//! Regenerate committed contracts: cargo run -p macroscope-domain --example export_schemas
use macroscope_domain::{CsvMapping, TaskContract, ToolManifest};
use schemars::{schema_for, Schema};
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../contracts");
    for (name, schema) in [
        ("task-contract", schema_for!(TaskContract)),
        ("csv-mapping", schema_for!(CsvMapping)),
        ("tool-manifest", schema_for!(ToolManifest)),
    ] {
        write_schema(&root, name, schema)?;
    }
    Ok(())
}

fn write_schema(root: &Path, name: &str, schema: Schema) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::write(
        root.join(format!("{name}.schema.json")),
        format!("{}\n", serde_json::to_string_pretty(&schema)?),
    )?;
    Ok(())
}
