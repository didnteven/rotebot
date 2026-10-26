use macroscope_muse_client::*;
use macroscope_vault::Secret;
use std::path::Path;

#[tokio::test]
#[ignore = "real Muse echo conformance; enable MACROSCOPE_MUSE_IT=1"]
async fn real_msp_echo_handshake_turn_and_clean_shutdown() {
    if std::env::var("MACROSCOPE_MUSE_IT").as_deref() != Ok("1") {
        return;
    }
    let real_home = std::env::var_os("HOME").expect("HOME");
    let binary = BinaryInfo::detect(Path::new(&real_home))
        .await
        .expect("native binary health");
    let root = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let ws = root.path().join("workspace");
    std::fs::create_dir(&ws).unwrap();
    let status = std::process::Command::new("/usr/bin/git")
        .env_clear()
        .env("PATH", SAFE_PATH)
        .args(["init", "--quiet"])
        .arg(&ws)
        .status()
        .unwrap();
    assert!(status.success());
    // Echo does not access the provider. This deliberately bogus key exercises
    // precisely the private ApiKey environment used by the shipped adapter.
    let env = ChildEnv::for_route(
        CredentialRoute::ApiKey,
        &root.path().join("home"),
        Path::new(&real_home),
        Some(Secret::new(b"bogus-echo-canary".to_vec())),
    )
    .unwrap();
    let mut client = MspClient::spawn(&binary, Provider::Echo, env, &ws)
        .await
        .expect("echo initialize and pinned fingerprint");
    let models = client.model_list().await.unwrap();
    assert!(models["models"].is_array());
    let session = client.session_start(&ws).await.unwrap();
    client
        .turn_start(&session, "hello from macroscope")
        .await
        .unwrap();
    let text = client.wait_turn().await.unwrap();
    assert_eq!(text, "echo: hello from macroscope");
    client.shutdown().await.expect("clean shutdown and reap");
}
