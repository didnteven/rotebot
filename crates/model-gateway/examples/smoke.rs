//! Real-key smoke test for the direct Meta route (plan §4.5 "Dedicated smoke harness",
//! M0 day 5). One catalog check and one tiny capped structured call; prints usage and
//! the estimated cost. Spend is bounded by `max_output_tokens: 64` and a ~300-token prompt.
//!
//!   cargo run -p macroscope-model-gateway --example smoke            # key from the app's Keychain item
//!   cargo run -p macroscope-model-gateway --example smoke -- --prompt  # type the key (hidden)
//!
//! The key is never taken from argv or the environment, never printed, and never written.

use macroscope_model_gateway::{
    CostEstimator, MetaResponsesClient, StructuredRequest, DEFAULT_MODEL,
};
use macroscope_vault::{validate_key, Secret, SecretRef, SecretStore};
use serde_json::json;
use std::io::Write;

fn read_hidden() -> Option<Secret> {
    eprint!("Meta API key (input hidden): ");
    let _ = std::io::stderr().flush();
    // `stty -echo` keeps the key off the screen; it is read from the terminal, not argv.
    let _ = std::process::Command::new("stty").arg("-echo").status();
    let mut line = String::new();
    let read = std::io::stdin().read_line(&mut line);
    let _ = std::process::Command::new("stty").arg("echo").status();
    eprintln!();
    read.ok()?;
    let (secret, _) = validate_key(line.trim()).ok()?;
    line.clear();
    Some(secret)
}

fn keychain_key() -> Option<Secret> {
    #[cfg(target_os = "macos")]
    {
        let store = macroscope_vault::KeychainStore;
        store
            .get(&SecretRef::new("meta-api-key").ok()?)
            .ok()
            .flatten()
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

#[tokio::main]
async fn main() {
    let prompt = std::env::args().any(|a| a == "--prompt");
    let base = std::env::args()
        .skip_while(|a| a != "--base-url")
        .nth(1)
        .unwrap_or_else(|| "https://api.meta.ai/v1".into());
    let key = if prompt {
        read_hidden()
    } else {
        keychain_key().or_else(read_hidden)
    };
    let Some(key) = key else {
        eprintln!("No key. Save one in Rotebot (Settings → Enter key…) or pass --prompt.");
        std::process::exit(2);
    };
    let client = MetaResponsesClient::new(base.clone());
    println!("endpoint: {base}");
    match client.catalog_check(&key).await {
        Ok(catalog) => println!(
            "catalog: ok ({} models; spark listed: {})",
            catalog.models.len(),
            catalog.models.iter().any(|m| m == DEFAULT_MODEL)
        ),
        Err(e) => {
            println!("catalog: {e}");
            std::process::exit(1);
        }
    }
    let schema = json!({"type": "object", "additionalProperties": false,
        "required": ["ok", "echo"], "properties": {"ok": {"type": "boolean"}, "echo": {"type": "string", "maxLength": 32}}});
    let started = std::time::Instant::now();
    let result = client
        .generate_structured(StructuredRequest {
            key: &key,
            model: Some(DEFAULT_MODEL),
            system: "Answer with the JSON object only.",
            user: "Set ok to true and echo to the word macroscope.",
            max_output_tokens: 64,
            reasoning_effort: "low",
            schema_name: "smoke",
            schema: &schema,
        })
        .await;
    match result {
        Ok(out) => {
            let cost = CostEstimator::default()
                .estimate(DEFAULT_MODEL, &out.usage)
                .ok();
            println!(
                "structured call: ok in {} ms",
                started.elapsed().as_millis()
            );
            println!("output: {}", out.value);
            println!("usage: {:?} (complete: {})", out.usage, out.usage_known);
            println!(
                "estimated cost: {} microdollars",
                cost.map(|c| c.to_string()).unwrap_or("unknown".into())
            );
            println!("Next: confirm the charge in your Meta developer dashboard (plan §4.5 attribution check).");
        }
        Err(e) => {
            println!("structured call: {e}");
            std::process::exit(1);
        }
    }
}
