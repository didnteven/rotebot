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
    ("01-oval-visor", "Keep the slate-grey shell. Add one clean black rounded oblong face panel inset into the head, like our existing desktop bots. Inside it put two crisp luminous ivory vertical oval eyes, moderately sized and widely spaced. Fully open eyes with a calm attentive expression. No pupils, eyelids, brows, nose or mouth. A subtle mint chest line is the only accent."),
    ("02-pill-eyes", "Keep the graphite-grey shell. Replace the sleepy eyes with two bright ivory vertical capsule eye shapes, clean luminous shapes recessed directly in the matte face with dark thin socket edges. No separate visor. Fully open attentive eyes, no eyelids, pupils, brows or mouth. The eyes should be expressive digital marks rather than realistic anatomy. Plain chest, no extra ornament."),
    ("03-angled-display", "Keep a midnight slate shell. Add a simple nearly black rounded face panel inset into the head. Give it two small crisp ivory softly rounded diagonal eye marks, like a confident pair of short slanted capsules, readable at small icon size. The expression is alert, curious and quietly confident, never angry or sleepy. No brows, pupils or mouth. One tiny subdued mint chest dash."),
];

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../");
    let output = root.join("output/slate-bot-eye-study");
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
        let prompt = format!("Use case: stylized-concept. Create ONE refined desktop robot using the provided slate robot as the body and material reference. This is specifically an eye-design revision. Retain the reference's simple softly rounded cuboid head, compact rounded body, two small arms, two short feet, matte sculptural surface and restrained proportions. IMPORTANT: replace its half-closed sleepy eye design completely. The new face must be visually crisp, alert and legible at 40px avatar size. Specific eye direction: {design} Keep the design simple and cool, not babyish or elaborate. No ears, antennae, fur, horns, accessories, machinery, logos or text. The face panel if requested is understated black glass, not a bulky helmet; no chrome border or extra lights. Single complete character centred in square frame, slight three-quarter view, soft even lighting, plain pale background, no ground plane, no cast shadow, no props. Leave at least 12 percent clear space on every side. Never crop head or feet. No collage or multiple characters.");
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
