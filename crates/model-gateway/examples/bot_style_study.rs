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
    ("01-pebble", "Design one extremely simple original robot companion called Pebble. A small smooth charcoal pebble-shaped body and a slightly larger rounded-square head, two tiny warm ivory oval eyes directly inset in the head, short rounded feet and little pill-shaped arms. No separate face visor, no mouth, no eyebrows, no ears, no antenna. Just five or six clean sculpted volumes. One tiny muted mint circular light on the chest is the only accent. Calm curious head tilt, approachable and quietly cool. Soft matte silicone or unglazed ceramic, subtle realistic studio shading. Minimalist designer toy with charm and personality from the silhouette and proportions alone. No fur, armor, joints, mechanical seams, markings, decorations, costume, accessories, weapons, words or logos. Feels sophisticated and friendly, not aggressive or babyish. Simple eyes should read clearly at 48 pixels."),
    ("02-mochi", "Design one exceptionally simple original robot companion called Mochi. A single soft lavender rounded capsule, like a friendly smooth little stone, with two small dark glossy oval eyes directly on its surface, tiny bean-shaped feet and two minimal rounded arm nubs. No distinct head-body joint, no face screen, no mouth, no brows, no ears, no antenna, no mechanical details. Slight asymmetry and a gentle curious lean give it life. Clean memorable silhouette, about four simple volumes, matte soft-touch vinyl with subtle ceramic shading. Very restrained one-color lavender palette with dark eyes, no glowing effects. Cute but composed, a premium minimalist desktop companion. No plush fur, panels, seams, patterns, clothes, logos, symbols or accessories. Must look beautifully simple and recognizable as a tiny UI avatar."),
    ("03-orbit", "Design one minimalist original robot companion called Orbit. A smooth warm-white rounded cube head on a tiny dark slate oval body, two small mint glowing dots for eyes directly inset in the head, two tiny rounded feet, short simple arms. A tiny gentle tilt to the head. No screen or outlined face panel, no mouth, no eyebrows, no ears, no antenna, no chest symbols. A compact sculptural character built from a few broad soft shapes. Quietly futuristic and endearing, designer vinyl toy aesthetics, smooth matte ceramic and soft diffuse lighting. Restrained white / slate / mint palette only. Avoid detailed robotics, segmented armor, mechanical joints, fur, glitter, gradients painted on the character, decorations, floating parts, props, labels or text. Make the silhouette and little eyes do all the work."),
];

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../");
    let output = root.join("output/bot-style-study-simple");
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
    let framing = " One full-body character only, isolated and centered, mostly frontal with a slight three-quarter angle. Square composition with generous empty space around the entire character. Clean plain light neutral background, soft studio lighting, no floor shadow, no scene, no typography, no watermark, no grid. High-quality 3D product render, but visually extremely simple. Every form must remain legible at 48 pixels.";
    for (name, design) in STUDIES {
        let prompt = format!("{design}{framing}");
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
        let bytes = generate_bot_png(DEFAULT_BASE_URL, &key, &prompt, None).await?;
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
