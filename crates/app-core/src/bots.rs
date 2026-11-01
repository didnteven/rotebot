//! User-requested artwork, independent of task execution. Secrets stay in the vault.
use crate::*;
use base64::{engine::general_purpose::STANDARD, Engine};
use macroscope_model_gateway::{generate_bot_png, BOT_IMAGE_MICROS};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BotDesign {
    pub id: String,
    pub description: String,
    pub completed_poses: Vec<u8>,
    pub attempts: u32,
    /// Persisted before dispatch, so a crash cannot silently retry a charged request.
    pub uncertain_pose: Option<u8>,
    pub created_at_ms: i64,
    #[serde(default)]
    pub transparent_poses: Vec<u8>,
    #[serde(default)]
    pub background_removal_error: Option<String>,
    #[serde(default)]
    pub images: BTreeMap<u8, String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BotImageRequest {
    pub id: String,
    pub description: String,
    pub pose: u8,
    pub approved: bool,
    #[serde(default)]
    pub retry_uncertain: bool,
}
const POSES: [&str; 6] = [
    "neutral friendly standing",
    "focused working, hands together",
    "waiting patiently, curious head tilt",
    "celebrating success with happy eyes and raised arms",
    "concerned but reassuring expression",
    "resting, eyes closed",
];

impl AppCore {
    fn bot_id(id: &str) -> CoreResult<()> {
        let parsed = uuid::Uuid::parse_str(id)
            .map_err(|_| CoreError::Invalid("Invalid bot identity.".into()))?;
        if parsed.to_string() != id {
            return Err(CoreError::Invalid("Invalid bot identity.".into()));
        }
        Ok(())
    }
    fn bot_meta(&self, id: &str) -> CoreResult<BotDesign> {
        Self::bot_id(id)?;
        let value = self
            .db()
            .setting(&format!("bot:{id}"))?
            .ok_or(CoreError::NotFound("bot"))?;
        serde_json::from_value(value)
            .map_err(|_| CoreError::Internal("Could not read this bot.".into()))
    }
    fn save_bot_meta(&self, bot: &BotDesign) -> CoreResult<()> {
        let mut meta = bot.clone();
        meta.images.clear();
        self.db().set_setting(
            &format!("bot:{}", bot.id),
            &serde_json::to_value(meta)
                .map_err(|_| CoreError::Internal("Could not save this bot.".into()))?,
        )?;
        Ok(())
    }
    fn bot_path(&self, id: &str, pose: u8) -> CoreResult<PathBuf> {
        Self::bot_id(id)?;
        if pose > 5 {
            return Err(CoreError::Invalid("Unknown bot pose.".into()));
        }
        Ok(self
            .inner
            .cfg
            .data_dir
            .join("bots")
            .join(id)
            .join(format!("{pose}.png")))
    }
    pub fn list_bot_designs(&self) -> CoreResult<Vec<BotDesign>> {
        self.ensure_active()?;
        let ids: Vec<String> =
            serde_json::from_value(self.db().setting("bot-designs")?.unwrap_or(json!([])))
                .map_err(|_| CoreError::Internal("Could not read your bots.".into()))?;
        ids.iter().map(|id| self.bot_meta(id)).collect()
    }
    pub fn get_bot_design(&self, id: &str) -> CoreResult<BotDesign> {
        self.ensure_active()?;
        let mut bot = self.bot_meta(id)?;
        for pose in &bot.completed_poses {
            let path = if bot.transparent_poses.contains(pose) {
                self.bot_path(id, *pose)?
                    .with_file_name(format!("{pose}-transparent.png"))
            } else {
                self.bot_path(id, *pose)?
            };
            let bytes = std::fs::read(path).map_err(|_| {
                CoreError::Invalid(
                    "A saved bot image could not be read. Create another bot to replace it.".into(),
                )
            })?;
            bot.images.insert(
                *pose,
                format!("data:image/png;base64,{}", STANDARD.encode(bytes)),
            );
        }
        Ok(bot)
    }
    pub async fn generate_bot_image(&self, req: BotImageRequest) -> CoreResult<BotDesign> {
        let _io = self.inner.bot_io.try_lock().map_err(|_| {
            CoreError::Invalid(
                "A bot image is already being created. Wait for it to finish.".into(),
            )
        })?;
        self.ensure_active()?;
        Self::bot_id(&req.id)?;
        if !req.approved {
            return Err(CoreError::Invalid(
                "Approve the image cost before generating.".into(),
            ));
        }
        if req.pose > 5 || req.description.trim().is_empty() || req.description.len() > 1000 {
            return Err(CoreError::Invalid(
                "Describe your bot in 1–1,000 characters and choose a valid pose.".into(),
            ));
        }
        let key = self
            .inner
            .cfg
            .vault
            .get(&Self::api_key_ref())
            .map_err(|_| CoreError::Vault("Could not read your Meta API key.".into()))?
            .ok_or_else(|| {
                CoreError::Invalid("Add a Meta API key in Settings to create a custom bot.".into())
            })?;
        let epoch = self.inner.policy.epoch();
        let admission = self
            .inner
            .policy
            .admit(macroscope_policy::Job { epoch })
            .map_err(|_| {
                CoreError::Invalid(
                    "Cloud models are paused. Turn off Local only in Privacy to create a bot."
                        .into(),
                )
            })?;
        // Capture all persistent admission state before network I/O.
        let prepared = (|| -> CoreResult<(BotDesign, String, Option<Vec<u8>>, String)> {
            let _local = self.admit_local()?;
            let mut ids: Vec<String> =
                serde_json::from_value(self.db().setting("bot-designs")?.unwrap_or(json!([])))
                    .map_err(|_| CoreError::Internal("Could not read your bots.".into()))?;
            let mut bot = if ids.contains(&req.id) {
                self.bot_meta(&req.id)?
            } else {
                if req.pose != 0 {
                    return Err(CoreError::Invalid("Create a preview first.".into()));
                }
                if ids.len() >= 100 {
                    return Err(CoreError::Invalid(
                        "Your custom bot library is full (100 bots).".into(),
                    ));
                }
                BotDesign {
                    id: req.id.clone(),
                    description: req.description.trim().into(),
                    completed_poses: vec![],
                    attempts: 0,
                    uncertain_pose: None,
                    created_at_ms: self.now(),
                    transparent_poses: vec![],
                    background_removal_error: None,
                    images: BTreeMap::new(),
                }
            };
            if bot.description != req.description.trim() {
                return Err(CoreError::Invalid(
                    "Start a new preview to change the bot description.".into(),
                ));
            }
            if bot.completed_poses.contains(&req.pose) {
                return Ok((bot, String::new(), None, String::new()));
            }
            if bot.uncertain_pose.is_some() && !req.retry_uncertain {
                return Err(CoreError::Invalid("The last request may have been charged. Confirm a possible extra charge before retrying.".into()));
            }
            if bot.attempts >= 20 {
                return Err(CoreError::Invalid("This bot has reached its US$0.20 generation budget. Start a new design to continue.".into()));
            }
            let reference = if req.pose == 0 {
                None
            } else {
                if !bot.completed_poses.contains(&0) {
                    return Err(CoreError::Invalid("Create a preview first.".into()));
                }
                Some(
                    std::fs::read(self.bot_path(&req.id, 0)?)
                        .map_err(|_| CoreError::Invalid("Could not read the preview.".into()))?,
                )
            };
            let prompt = format!("Create exactly one original premium 3D AI companion. User design: {}. Pose: {}. Full body entirely visible, centered, square composition, generous 12% margin, consistent scale and camera, soft studio lighting, expressive face, tactile materials, clean plain pale neutral background clearly separated from the subject with no ground shadows or props, no text, no watermark, no grid or sprite sheet. If a reference is provided, preserve the exact character, materials, colors and proportions; change only the pose and expression.", bot.description, POSES[req.pose as usize]);
            let outbound_id = new_id("out_bot");
            self.db().insert_outbound(&db::OutboundRow {
                id: outbound_id.clone(),
                at_ms: self.now(),
                purpose: "Custom bot image".into(),
                destination: "Meta Muse Image".into(),
                route: "api_key".into(),
                synthetic_only: false,
                cost_micros: None,
                payload_text: Some(format!(
                    "{prompt}\nReference: {}",
                    if reference.is_some() {
                        "previously approved bot preview"
                    } else {
                        "none"
                    }
                )),
            })?;
            bot.attempts += 1;
            bot.uncertain_pose = Some(req.pose);
            self.save_bot_meta(&bot)?;
            if !ids.contains(&bot.id) {
                ids.insert(0, bot.id.clone());
                self.db().set_setting("bot-designs", &json!(ids))?;
            }
            Ok((bot, prompt, reference, outbound_id))
        })();
        let (mut bot, prompt, reference, outbound_id) = match prepared {
            Ok(p) => p,
            Err(e) => {
                admission.guard.acknowledge(true);
                return Err(e);
            }
        };
        if prompt.is_empty() {
            admission.guard.acknowledge(true);
            return self.get_bot_design(&bot.id);
        }
        let result = tokio::select! {
            biased;
            _ = self.inner.policy.revoked(epoch) => Err(GatewayError::Unavailable),
            result = generate_bot_png(&self.inner.cfg.gateway_base_url, &key, &prompt, reference.as_deref()) => result,
        };
        admission.guard.acknowledge(true);
        let bytes = match result {
            Ok(bytes) => {
                self.db()
                    .set_outbound_cost(&outbound_id, Some(BOT_IMAGE_MICROS))?;
                bytes
            }
            Err(error) => {
                // Only definite pre-generation rejections can be retried without warning.
                if matches!(
                    error,
                    GatewayError::AuthRejected
                        | GatewayError::BillingRequired
                        | GatewayError::Quota { .. }
                        | GatewayError::Rejected
                        | GatewayError::Refused
                ) {
                    bot.uncertain_pose = None;
                    bot.attempts = bot.attempts.saturating_sub(1);
                    if let Ok(_local) = self.admit_local() {
                        if self.inner.policy.epoch() == epoch {
                            let _ = self.save_bot_meta(&bot);
                        }
                    }
                }
                return Err(CoreError::Invalid(match error {
                    GatewayError::AuthRejected => "Meta refused image access. Check your key and Muse Image access in Settings.".into(),
                    GatewayError::BillingRequired => "Meta needs billing enabled before it can create bot images.".into(),
                    GatewayError::Quota { .. } => "Meta’s image limit has been reached. Wait before trying again.".into(),
                    _ => "Could not finish this image. The request may have been charged; saved poses are kept. Check the preview before retrying.".into(),
                }));
            }
        };
        // Decode before storage. Cap dimensions and allocations; never render remote URLs or SVG.
        let mut decoder = png::Decoder::new(std::io::Cursor::new(&bytes));
        decoder.set_limits(png::Limits {
            bytes: 64 * 1024 * 1024,
        });
        let mut reader = decoder.read_info().map_err(|_| {
            CoreError::Invalid(
                "Meta returned an unreadable image. This request may have been charged.".into(),
            )
        })?;
        let info = reader.info();
        if info.width == 0 || info.height == 0 || info.width > 4096 || info.height > 4096 {
            return Err(CoreError::Invalid(
                "Meta returned an oversized image. This request may have been charged.".into(),
            ));
        }
        let mut pixels = vec![
            0;
            reader
                .output_buffer_size()
                .ok_or_else(|| CoreError::Invalid("Image is too large.".into()))?
        ];
        reader.next_frame(&mut pixels).map_err(|_| {
            CoreError::Invalid(
                "Meta returned an incomplete image. This request may have been charged.".into(),
            )
        })?;
        let _local = self.admit_local()?;
        if self.inner.policy.epoch() != epoch {
            return Err(CoreError::Invalid(
                "Cloud work was stopped. The image request may have been charged.".into(),
            ));
        }
        let path = self.bot_path(&bot.id, req.pose)?;
        std::fs::create_dir_all(path.parent().unwrap())
            .map_err(|_| CoreError::Storage("Could not create the bot folder.".into()))?;
        let temporary = path.with_extension("pending");
        std::fs::write(&temporary, &bytes)
            .and_then(|_| std::fs::rename(&temporary, &path))
            .map_err(|_| {
                CoreError::Storage(
                    "Could not save the image. This request may have been charged.".into(),
                )
            })?;
        bot.completed_poses.push(req.pose);
        bot.uncertain_pose = None;
        self.save_bot_meta(&bot)?;
        drop(_local);
        self.clean_bot_pose(&bot.id, req.pose).await;
        self.get_bot_design(&bot.id)
    }
}

fn png_dimensions(bytes: &[u8]) -> CoreResult<(u32, u32)> {
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let reader = decoder
        .read_info()
        .map_err(|_| CoreError::Invalid("Could not read the bot image.".into()))?;
    Ok((reader.info().width, reader.info().height))
}

fn validate_cutout(bytes: &[u8], expected: (u32, u32)) -> CoreResult<()> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_limits(png::Limits {
        bytes: 64 * 1024 * 1024,
    });
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder
        .read_info()
        .map_err(|_| CoreError::Invalid("The background could not be removed cleanly.".into()))?;
    if (reader.info().width, reader.info().height) != expected
        || expected.0 > 4096
        || expected.1 > 4096
    {
        return Err(CoreError::Invalid(
            "Background removal changed the image dimensions.".into(),
        ));
    }
    let mut pixels = vec![
        0;
        reader
            .output_buffer_size()
            .ok_or_else(|| CoreError::Invalid("The image is too large.".into()))?
    ];
    let frame = reader
        .next_frame(&mut pixels)
        .map_err(|_| CoreError::Invalid("Could not read the transparent image.".into()))?;
    let channels = match frame.color_type {
        png::ColorType::Rgba => 4,
        png::ColorType::GrayscaleAlpha => 2,
        _ => {
            return Err(CoreError::Invalid(
                "The image still has an opaque background.".into(),
            ))
        }
    };
    let alpha: Vec<u8> = pixels[..frame.buffer_size()]
        .chunks_exact(channels)
        .map(|p| p[channels - 1])
        .collect();
    let clear = alpha.iter().filter(|a| **a < 8).count();
    if clear * 100 < alpha.len() || !alpha.iter().any(|a| *a > 200) {
        return Err(CoreError::Invalid(
            "The foreground could not be separated from its background.".into(),
        ));
    }
    Ok(())
}

impl AppCore {
    pub fn configure_bot_matte_helper(&self, helper: Option<PathBuf>) {
        *lock(&self.inner.bot_matte_helper) = helper;
    }
    async fn matte_bot_png(&self, bytes: Vec<u8>) -> CoreResult<Vec<u8>> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let dimensions = png_dimensions(&bytes)?;
        // Native transparent output needs no further processing.
        if validate_cutout(&bytes, dimensions).is_ok() {
            return Ok(bytes);
        }
        let helper = lock(&self.inner.bot_matte_helper).clone().ok_or_else(|| CoreError::Invalid("Local background removal is not available. Restart the updated macOS app and try again.".into()))?;
        let mut child = tokio::process::Command::new(helper)
            .env_clear()
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|_| CoreError::Invalid("Could not start local background removal.".into()))?;
        let mut input = child
            .stdin
            .take()
            .ok_or_else(|| CoreError::Internal("Image helper input unavailable.".into()))?;
        let output = child
            .stdout
            .take()
            .ok_or_else(|| CoreError::Internal("Image helper output unavailable.".into()))?;
        let operation = async {
            let write = async {
                input.write_all(&bytes).await?;
                input.shutdown().await?;
                drop(input);
                Ok::<_, std::io::Error>(())
            };
            let read = async {
                let mut png = Vec::new();
                output
                    .take(8 * 1024 * 1024 + 1)
                    .read_to_end(&mut png)
                    .await?;
                Ok::<_, std::io::Error>(png)
            };
            let (_, png) = tokio::try_join!(write, read).map_err(|_| {
                CoreError::Invalid("Local background removal was interrupted.".into())
            })?;
            let status = child
                .wait()
                .await
                .map_err(|_| CoreError::Invalid("Could not finish background removal.".into()))?;
            if !status.success() || png.len() > 8 * 1024 * 1024 {
                return Err(CoreError::Invalid("Could not identify the bot’s foreground. Your original image is saved; retry cleanup without a generation charge.".into()));
            }
            validate_cutout(&png, dimensions)?;
            Ok(png)
        };
        match tokio::time::timeout(std::time::Duration::from_secs(60), operation).await {
            Ok(result) => result,
            Err(_) => {
                let _ = child.kill().await;
                Err(CoreError::Invalid("Background removal took too long. Your image is saved; retry cleanup without a generation charge.".into()))
            }
        }
    }
    async fn clean_bot_pose(&self, id: &str, pose: u8) {
        let result = async {
            let bytes = {
                let _local = self.admit_local()?;
                let bot = self.bot_meta(id)?;
                if bot.transparent_poses.contains(&pose) {
                    return Ok(());
                }
                std::fs::read(self.bot_path(id, pose)?)
                    .map_err(|_| CoreError::Invalid("Could not read the saved original.".into()))?
            };
            let png = self.matte_bot_png(bytes).await?;
            let _local = self.admit_local()?;
            let mut bot = self.bot_meta(id)?;
            let path = self
                .bot_path(id, pose)?
                .with_file_name(format!("{pose}-transparent.png"));
            let temporary = path.with_extension("pending");
            std::fs::write(&temporary, png)
                .and_then(|_| std::fs::rename(&temporary, &path))
                .map_err(|_| CoreError::Storage("Could not save the transparent image.".into()))?;
            if !bot.transparent_poses.contains(&pose) {
                bot.transparent_poses.push(pose);
            }
            bot.background_removal_error = None;
            self.save_bot_meta(&bot)
        }
        .await;
        if let Err(error) = result {
            if let Ok(_local) = self.admit_local() {
                if let Ok(mut bot) = self.bot_meta(id) {
                    bot.background_removal_error = Some(error.to_string());
                    let _ = self.save_bot_meta(&bot);
                }
            }
        }
    }
    pub async fn remove_bot_backgrounds(&self, id: &str) -> CoreResult<BotDesign> {
        let _io =
            self.inner.bot_io.try_lock().map_err(|_| {
                CoreError::Invalid("Wait for the current bot image to finish.".into())
            })?;
        let bot = self.get_bot_design(id)?;
        for pose in bot.completed_poses {
            if !bot.transparent_poses.contains(&pose) {
                self.clean_bot_pose(id, pose).await;
                if self.bot_meta(id)?.background_removal_error.is_some() {
                    break;
                }
            }
        }
        self.get_bot_design(id)
    }
}

#[cfg(test)]
mod matte_tests {
    use super::*;
    #[test]
    fn cutout_requires_visible_subject_and_real_alpha_without_changing_extent() {
        let encode = |pixels: &[u8]| {
            let mut bytes = Vec::new();
            {
                let mut encoder = png::Encoder::new(&mut bytes, 2, 2);
                encoder.set_color(png::ColorType::Rgba);
                let mut writer = encoder.write_header().unwrap();
                writer.write_image_data(pixels).unwrap();
            }
            bytes
        };
        let cutout = encode(&[0, 0, 0, 0, 120, 90, 200, 255, 120, 90, 200, 160, 0, 0, 0, 0]);
        assert!(validate_cutout(&cutout, (2, 2)).is_ok());
        assert!(validate_cutout(&cutout, (3, 2)).is_err());
        assert!(validate_cutout(&encode(&[255; 16]), (2, 2)).is_err());
        assert!(validate_cutout(&encode(&[0; 16]), (2, 2)).is_err());
    }
}
