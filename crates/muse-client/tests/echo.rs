use macroscope_muse_client::*;
use macroscope_vault::Secret;
use std::path::Path;

async fn native() -> BinaryInfo {
    BinaryInfo::detect(Path::new(&std::env::var_os("HOME").expect("HOME")))
        .await
        .expect("native binary health")
}

#[tokio::test]
#[ignore = "real Muse echo conformance; enable MACROSCOPE_MUSE_IT=1"]
async fn real_msp_echo_handshake_turn_and_clean_shutdown() {
    if std::env::var("MACROSCOPE_MUSE_IT").as_deref() != Ok("1") {
        return;
    }
    let binary = native().await;
    let root = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let report = run_echo_conformance(&binary, CURRENT_FINGERPRINT, root.path()).await;
    assert_eq!(report.result, ConformanceResult::Passed, "{report:?}");
    let store = JsonAcceptanceStore::new(root.path().join("app-state/muse"));
    persist_conformance(&binary, &store, &report).unwrap();
    let record = store.load(&binary.binary_sha256).unwrap().unwrap();
    assert_eq!(record.result, ConformanceResult::Passed);
    assert_eq!(record.fingerprint, CURRENT_FINGERPRINT);
    assert_eq!(record.version, binary.version);
    assert!(record.checked_at_ms > 0);
    assert_eq!(
        report.setup_state(),
        SetupState::Ready {
            fingerprint: CURRENT_FINGERPRINT.into(),
            verified: Verification::Conformance
        }
    );
    // Explicit opt-in test helper to refresh reviewed real-wire replay evidence.
    if std::env::var("MACROSCOPE_RECORD_ECHO").as_deref() == Ok("1") {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/msp/echo-1.4.13.jsonl");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let frames = report
            .frames
            .iter()
            .map(|v| serde_json::to_string(v).unwrap())
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        std::fs::write(path, frames).unwrap();
    }
}

#[tokio::test]
#[ignore = "real Muse echo exec JSONL; enable MACROSCOPE_MUSE_IT=1"]
async fn real_exec_echo_jsonl_reaches_terminal() {
    if std::env::var("MACROSCOPE_MUSE_IT").as_deref() != Ok("1") {
        return;
    }
    let binary = native().await;
    let root = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let ws = root.path().join("workspace");
    std::fs::create_dir(&ws).unwrap();
    let schema = ws.join("output.schema.json");
    std::fs::write(&schema, r#"{"type":"object","properties":{"ok":{"type":"boolean"}},"required":["ok"],"additionalProperties":false}"#).unwrap();
    let env = ChildEnv::for_route(
        CredentialRoute::ApiKey,
        &root.path().join("home"),
        root.path(),
        Some(Secret::new(b"echo-exec-canary".to_vec())),
    )
    .unwrap();
    let output = ExecClient::pinned(&binary)
        .exec_echo_structured("hello from macroscope", &schema, &ws, &env)
        .await
        .expect("real echo JSONL parsing and runtime help flags");
    assert!(output.terminal.is_some(), "{output:?}");
    assert!(!output.events.is_empty());
}
