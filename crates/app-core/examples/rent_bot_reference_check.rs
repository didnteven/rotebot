//! One Rent preview checking the production prompt and bundled style reference.
//! The saved key is read through the vault and never printed, passed in argv, or exported.
use macroscope_app_core::bots::preview_character_prompt;
use macroscope_model_gateway::{generate_bot_png, DEFAULT_BASE_URL};
use macroscope_vault::{FileStore, SecretRef, SecretStore};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
};

const STUDIES: [(&str, &str); 1] = [("rent", "Rent")];

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../");
    let output = root.join("output/rent-bot-reference-check");
    fs::create_dir_all(&output)?;
    let home = PathBuf::from(std::env::var_os("HOME").ok_or("Home directory unavailable")?);
    let vault_path = home.join("Library/Application Support/dev.macroscope.desktop/dev-secrets");
    if !vault_path.is_dir() {
        return Err("The app's development vault was not found; no requests made.".into());
    }
    let store = FileStore::new(vault_path)?;
    let key = store
        .get(&SecretRef::new("meta-api-key")?)?
        .ok_or("No saved Meta API key; no requests made.")?;
    let reference = fs::read(root.join("crates/app-core/assets/bot-style-reference.png"))?;
    for (name, design) in STUDIES {
        let prompt = preview_character_prompt(design);
        let file = output.join(format!("{name}.png"));
        // A durable attempt marker prevents accidental recharges on rerunning the study.
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output.join(format!("{name}.attempted")))
        {
            Ok(mut marker) => marker.write_all(
                b"One Meta image request authorized; do not auto-retry. Estimated US$0.01.\n",
            )?,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                println!("{name}: already attempted; skipped");
                continue;
            }
            Err(error) => return Err(error.into()),
        }
        fs::write(output.join(format!("{name}.prompt.txt")), &prompt)?;
        println!("{name}: requesting one image, estimated US$0.01");
        let bytes = generate_bot_png(DEFAULT_BASE_URL, &key, &prompt, Some(&reference)).await?;
        fs::write(&file, &bytes)?;
        println!("{name}: original saved ({} bytes)", bytes.len());
        let helper =
            root.join("apps/desktop/src-tauri/binaries/macroscope-bot-matte-aarch64-apple-darwin");
        let transparent = output.join(format!("{name}-transparent.png"));
        if helper.is_file() {
            let mut child = Command::new(helper)
                .env_clear()
                .stdin(Stdio::piped())
                .stdout(fs::File::create(&transparent)?)
                .stderr(Stdio::null())
                .spawn()?;
            child
                .stdin
                .take()
                .ok_or("No helper input")?
                .write_all(&bytes)?;
            if child.wait()?.success() {
                println!("{name}: transparent preview saved");
            } else {
                let _ = fs::remove_file(&transparent);
                println!("{name}: local cleanup unavailable; original kept");
            }
        }
    }
    println!("Complete: maximum one image request, estimated maximum generation cost US$0.01. App artwork unchanged.");
    Ok(())
}
