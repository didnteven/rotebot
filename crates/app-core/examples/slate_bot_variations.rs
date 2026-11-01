//! Three explicitly requested character studies, at most one API call each.
//! The saved key is read through the vault and never printed, passed in argv, or exported.
use macroscope_model_gateway::{generate_bot_png, DEFAULT_BASE_URL};
use macroscope_vault::{FileStore, SecretRef, SecretStore};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
};

const STUDIES: [(&str, &str); 3] = [
    ("01-graphite-mint", "Make a close sibling in deeper graphite grey with soft mint eyes. Slightly narrower head, compact rounded body, small integrated arms, two stubby feet. A single thin mint inset on the chest. Keep the reference's simple understated personality."),
    ("02-stone-ice", "Make a close sibling in cool stone grey with very pale ice-blue eyes. A slightly taller softly rounded rectangular head and a smaller plain rounded body, integrated arms and two short feet. No chest decoration. Calm alert expression, with subtle narrow eyes."),
    ("03-midnight-amber", "Make a close sibling in matte midnight blue with a restrained warm amber eye colour. A slightly wider low rounded rectangular head and squat simple body, integrated arms and two short feet. One very small amber chest dash. Relaxed quietly confident expression."),
];

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../");
    let output = root.join("output/slate-bot-variations");
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
    let reference = fs::read(root.join("output/character-study-minimal-cool/01-slate-robot.png"))?;
    for (name, design) in STUDIES {
        let prompt = format!("Use case: stylized-concept. Create ONE new desktop robot character using the provided slate robot as the primary visual style reference. Preserve its restrained, simple sculptural design language: softly rounded geometric volumes, matte ceramic-like surface, small inset eyes, no mouth, no exposed mechanics, no antennae, no accessories, no shiny visor. Cool and understated, with balanced proportions; avoid babyish cuteness, giant eyes and elaborate details. Change only the specific design details requested here: {design} Full body, centred square composition, mostly frontal slight three-quarter angle, at least 12 percent empty margin on ALL sides including feet and head. Soft even studio lighting. Plain pale background cleanly separated from the figure, no cast shadow, no ground plane, no props, no text, no logo, no collage. Output one individual robot, never multiple characters.");
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
    println!("Complete: maximum three image requests, estimated maximum generation cost US$0.03. App artwork unchanged.");
    Ok(())
}
