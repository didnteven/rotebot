//! Focused local stand-in: no real credentials, generations, or charges.
use base64::{engine::general_purpose::STANDARD, Engine};
use macroscope_app_core::{bots::BotImageRequest, AppCore, CoreConfig, NativeMuse};
use macroscope_vault::MemoryStore;
use serde_json::{json, Value};
use std::sync::Arc;
use wiremock::{matchers::method, Mock, MockServer, Request, Respond, ResponseTemplate};

struct ImageProvider(String);
impl Respond for ImageProvider {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        assert_eq!(
            request.headers.get("authorization").unwrap(),
            "Bearer bot-test-key"
        );
        let body: Value = serde_json::from_slice(&request.body).unwrap();
        assert_eq!(body["model"], "muse-image-1.0");
        assert_eq!(body["n"], 1);
        assert_eq!(body["output_format"], "png");
        assert!(!body.to_string().contains("bot-test-key"));
        assert_eq!(request.url.path(), "/v1/images/edits");
        let prompt = body["prompt"].as_str().unwrap();
        if prompt.contains("STYLE REFERENCE:") {
            assert!(prompt.contains("task title or activity"));
            assert!(prompt.contains("Never substitute tiny colored dots"));
            assert!(!prompt.contains("REFERENCE LOCK:"));
        } else {
            assert!(prompt.contains("REFERENCE LOCK:"));
        }
        if request.url.path() == "/v1/images/edits" {
            assert!(body["images"][0]["image_url"]
                .as_str()
                .unwrap()
                .starts_with("data:image/png;base64,"));
        } else {
            assert_eq!(request.url.path(), "/v1/images/generations");
        }
        ResponseTemplate::new(200).set_body_json(json!({"data":[{"b64_json":self.0}]}))
    }
}
#[tokio::test]
async fn preview_pose_cache_restart_and_cloud_consent_boundary() {
    let server = MockServer::start().await;
    let mut png_bytes = Vec::new();
    {
        let encoder = png::Encoder::new(&mut png_bytes, 1, 1);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&[128]).unwrap();
    }
    Mock::given(method("POST"))
        .respond_with(ImageProvider(STANDARD.encode(png_bytes)))
        .expect(2)
        .mount(&server)
        .await;
    let tmp = tempfile::tempdir().unwrap();
    let vault = Arc::new(MemoryStore::default());
    let make_core = || {
        AppCore::new(CoreConfig {
            data_dir: tmp.path().join("data"),
            home: tmp.path().join("home"),
            vault: vault.clone(),
            gateway_base_url: format!("{}/v1", server.uri()),
            muse: Arc::new(NativeMuse {
                home: tmp.path().join("home"),
            }),
            app_version: "test".into(),
            platform: "test".into(),
            now_ms: CoreConfig::system_clock(),
        })
        .unwrap()
    };
    let core = make_core();
    core.save_api_key("bot-test-key").await.unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let request = |pose, approved| BotImageRequest {
        id: id.clone(),
        description: "A little lavender companion".into(),
        pose,
        approved,
        retry_uncertain: false,
    };
    assert!(core.generate_bot_image(request(0, false)).await.is_err());
    let preview = core.generate_bot_image(request(0, true)).await.unwrap();
    assert_eq!(preview.completed_poses, vec![0]);
    assert!(preview.images[&0].starts_with("data:image/png;base64,"));
    let pose = core.generate_bot_image(request(1, true)).await.unwrap();
    assert_eq!(pose.attempts, 2);
    assert_eq!(
        core.generate_bot_image(request(1, true))
            .await
            .unwrap()
            .attempts,
        2
    );
    drop(core);
    let core = make_core();
    assert_eq!(
        core.get_bot_design(&id).unwrap().completed_poses,
        vec![0, 1]
    );
    assert!(core.list_bot_designs().unwrap()[0].images.is_empty());
    core.set_local_only(true).await.unwrap();
    assert!(core.generate_bot_image(request(2, true)).await.is_err());
    assert_eq!(core.get_bot_design(&id).unwrap().attempts, 2);
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}
