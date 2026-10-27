//! Real SQLCipher failures injected with SQLite triggers, without production test hooks.
use macroscope_app_core::{dto::*, AppCore, CoreConfig, NativeMuse};
use macroscope_vault::{MemoryStore, Secret, SecretRef, SecretStore};
use serde_json::{json, Value};
use std::{path::Path, sync::Arc};
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

fn setup(root: &Path, base: String) -> (AppCore, rusqlite::Connection) {
    let vault = Arc::new(MemoryStore::default());
    vault
        .set(&SecretRef::new("db-key").unwrap(), Secret::new(vec![7; 32]))
        .unwrap();
    let core = AppCore::new(CoreConfig {
        data_dir: root.join("data"),
        vault,
        gateway_base_url: base,
        home: root.join("home"),
        muse: Arc::new(NativeMuse {
            home: root.join("home"),
        }),
        app_version: "test".into(),
        platform: "macos".into(),
        now_ms: CoreConfig::system_clock(),
    })
    .unwrap();
    let sql = rusqlite::Connection::open(root.join("data/db/state.db")).unwrap();
    sql.execute_batch(&format!("PRAGMA key = \"x'{}'\";", "07".repeat(32)))
        .unwrap();
    (core, sql)
}

fn reject(sql: &rusqlite::Connection, statement: &str) {
    sql.execute_batch(&format!("CREATE TRIGGER fail_write BEFORE {statement} BEGIN SELECT RAISE(FAIL, 'injected persistence failure'); END;")).unwrap();
}

fn draft(core: &AppCore) -> TaskDraft {
    let input = core.load_sample_csv().unwrap();
    core.draft_task(&input.handle.handle_id, "Prepare orders")
        .unwrap()
}

async fn terminal(core: &AppCore, id: &str) -> BuildState {
    for _ in 0..200 {
        let state = core.build_state(id).unwrap();
        if matches!(
            state.phase,
            BuildPhase::Ready | BuildPhase::Failed | BuildPhase::Stopped
        ) {
            return state;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("build did not terminate");
}

#[tokio::test]
async fn build_admission_propagates_persistence_failure_before_dispatch() {
    let tmp = tempfile::tempdir().unwrap();
    let server = MockServer::start().await;
    let (core, sql) = setup(tmp.path(), server.uri());
    core.save_api_key("mk-test-persistence-0123456789")
        .await
        .unwrap();
    let draft = draft(&core);
    reject(&sql, "INSERT ON builds");
    assert!(
        core.start_build(&draft.draft_id, true).await.is_err(),
        "admission ignored failed intent write"
    );
    assert!(server.received_requests().await.unwrap().is_empty());
    sql.execute_batch("DROP TRIGGER fail_write").unwrap();
    // Failed admission must not wedge the one-build limit.
    let started = core.start_build(&draft.draft_id, true).await.unwrap();
    core.stop_build(&started.build_id).unwrap();
}

#[tokio::test]
async fn failed_ready_persistence_never_publishes_ready_and_recovers_as_failed() {
    let tmp = tempfile::tempdir().unwrap();
    let (core, sql) = setup(tmp.path(), "http://127.0.0.1:9".into());
    let draft = draft(&core);
    reject(&sql, "INSERT ON builds WHEN NEW.phase = 'ready'");
    let started = core.start_build(&draft.draft_id, false).await.unwrap();
    let state = terminal(&core, &started.build_id).await;
    assert_eq!(
        state.phase,
        BuildPhase::Failed,
        "unpersisted Ready was published"
    );
    assert_eq!(state.failure.unwrap().kind, BuildFailureKind::Internal);
    assert_eq!(
        core.database()
            .build(&started.build_id)
            .unwrap()
            .unwrap()
            .phase,
        "failed"
    );
}

#[tokio::test]
async fn outbound_cost_persistence_failure_fails_the_build_and_keeps_reservation() {
    let server = MockServer::start().await;
    let task: Value =
        serde_json::from_str(include_str!("../../../fixtures/orders/task.json")).unwrap();
    Mock::given(method("POST")).and(path("/responses")).respond_with(ResponseTemplate::new(200).set_body_json(json!({
        "status":"completed", "output":[{"type":"message","content":[{"type":"output_text","text":task["mapping"].to_string()}]}],
        "usage":{"input_tokens":3000,"output_tokens":400,"output_tokens_details":{"reasoning_tokens":100}}
    }))).mount(&server).await;
    let tmp = tempfile::tempdir().unwrap();
    let (core, sql) = setup(tmp.path(), server.uri());
    core.save_api_key("mk-test-persistence-0123456789")
        .await
        .unwrap();
    let draft = draft(&core);
    reject(&sql, "UPDATE ON outbound");
    let started = core.start_build(&draft.draft_id, true).await.unwrap();
    let state = terminal(&core, &started.build_id).await;
    assert_eq!(
        state.phase,
        BuildPhase::Failed,
        "cost failure was silently ignored"
    );
    assert_eq!(state.failure.unwrap().kind, BuildFailureKind::Internal);
    assert_eq!(core.database().reserved_micros_since(0).unwrap(), 2_000_000);
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn total_transition_write_failure_stops_before_egress_and_restart_recovers_intent() {
    let tmp = tempfile::tempdir().unwrap();
    let server = MockServer::start().await;
    let (core, sql) = setup(tmp.path(), server.uri());
    core.save_api_key("mk-test-persistence-0123456789")
        .await
        .unwrap();
    let draft = draft(&core);
    reject(&sql, "INSERT ON builds WHEN NEW.phase != 'queued'");
    let started = core.start_build(&draft.draft_id, true).await.unwrap();
    let state = terminal(&core, &started.build_id).await;
    assert_eq!(state.phase, BuildPhase::Failed);
    assert_eq!(state.failure.unwrap().kind, BuildFailureKind::Internal);
    assert!(server.received_requests().await.unwrap().is_empty());
    assert_eq!(
        core.database()
            .build(&started.build_id)
            .unwrap()
            .unwrap()
            .phase,
        "queued"
    );
    assert_eq!(core.database().reserved_micros_since(0).unwrap(), 2_000_000);
    sql.execute_batch("DROP TRIGGER fail_write").unwrap();
    drop(core);
    let (restarted, _) = setup(tmp.path(), server.uri());
    assert_eq!(
        restarted.build_state(&started.build_id).unwrap().phase,
        BuildPhase::Failed
    );
    assert_eq!(
        restarted.database().reserved_micros_since(0).unwrap(),
        2_000_000
    );
}
