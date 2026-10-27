use macroscope_app_core::dto::*;
use macroscope_app_core::{AppCore, CoreConfig, NativeMuse};
use macroscope_vault::{MemoryStore, Secret, SecretRef, SecretStore};
use serde_json::json;
use std::path::Path;
use std::sync::Arc;

fn core_at(root: &Path, vault: Arc<MemoryStore>) -> AppCore {
    AppCore::new(CoreConfig {
        data_dir: root.join("data"),
        vault,
        gateway_base_url: "http://127.0.0.1:9".into(),
        home: root.join("home"),
        muse: Arc::new(NativeMuse {
            home: root.join("home"),
        }),
        app_version: "test".into(),
        platform: "macos".into(),
        now_ms: CoreConfig::system_clock(),
    })
    .unwrap()
}

async fn staged(core: &AppCore, tool: &str, root: &Path, name: &str) -> StagedRun {
    let input = core.load_sample_csv().unwrap().handle;
    let dir = core.register_picked_dir(root).unwrap();
    match core
        .stage_run(tool, &input.handle_id, &dir.handle_id, name)
        .await
        .unwrap()
    {
        StageResult::Staged(s) => *s,
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn pause_and_restore_never_reauthorize_a_previously_staged_run() {
    let tmp = tempfile::tempdir().unwrap();
    let core = core_at(tmp.path(), Arc::new(MemoryStore::default()));
    let tool = core.install_sample_tool().unwrap();
    let id = &tool.summary_row.id;
    let first = staged(&core, id, tmp.path(), "old.csv").await;
    assert_eq!(core.revoke_tool(id).unwrap().summary_row.state, "paused");
    assert!(matches!(
        core.stage_run(id, "forged", "forged", "other.csv")
            .await
            .unwrap(),
        StageResult::Failed(FailureEnvelope {
            failure: RunFailure::Revoked { .. }
        })
    ));
    assert_eq!(core.restore_tool(id).unwrap().summary_row.state, "enabled");
    assert!(matches!(
        core.commit_run(&first.run_id, &first.plan_hash).unwrap(),
        CommitResult::Failed(FailureEnvelope {
            failure: RunFailure::Revoked { .. }
        })
    ));
    assert!(!tmp.path().join("old.csv").exists());
    let second = staged(&core, id, tmp.path(), "fresh.csv").await;
    core.revoke_tool(id).unwrap();
    assert!(matches!(
        core.commit_run(&second.run_id, &second.plan_hash).unwrap(),
        CommitResult::Failed(FailureEnvelope {
            failure: RunFailure::Revoked { .. }
        })
    ));
    core.restore_tool(id).unwrap();
    let third = staged(&core, id, tmp.path(), "fresh.csv").await;
    assert!(matches!(
        core.commit_run(&third.run_id, &third.plan_hash).unwrap(),
        CommitResult::Committed(_)
    ));
}

#[tokio::test]
async fn export_is_structural_create_only_and_refuses_symlink_swaps() {
    let tmp = tempfile::tempdir().unwrap();
    let core = core_at(tmp.path(), Arc::new(MemoryStore::default()));
    let tool = core.install_sample_tool().unwrap();
    let root = tmp.path().join("Exports");
    std::fs::create_dir(&root).unwrap();
    let handle = core.register_picked_dir(&root).unwrap();
    let path = core
        .export_tool(&tool.summary_row.id, &handle.handle_id)
        .unwrap();
    let path = Path::new(&path);
    assert_eq!(
        path.file_name().unwrap().to_str().unwrap(),
        "Clean import CSV (sample)-v1"
    );
    assert_eq!(std::fs::read_dir(path).unwrap().count(), 4);
    let contract: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path.join("contract.json")).unwrap()).unwrap();
    assert_eq!(
        contract,
        json!({"inputColumns": tool.input_columns, "outputColumns": tool.output_columns})
    );
    for file in [
        "manifest.json",
        "mapping.json",
        "contract.json",
        "README.txt",
    ] {
        let content = std::fs::read_to_string(path.join(file)).unwrap();
        for canary in [
            "Ada Example",
            "ORD-001",
            "$1,240.00",
            "handleId",
            "previewRows",
        ] {
            assert!(!content.contains(canary), "{file}: {canary}");
        }
    }
    let readme = std::fs::read_to_string(path.join("README.txt")).unwrap();
    assert!(readme.contains("column names only") && readme.contains("Runs in Macroscope"));
    assert!(core
        .export_tool(&tool.summary_row.id, &handle.handle_id)
        .is_err());
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        std::fs::rename(&root, tmp.path().join("Moved")).unwrap();
        let outside = tmp.path().join("Outside");
        std::fs::create_dir(&outside).unwrap();
        symlink(&outside, &root).unwrap();
        assert!(core
            .export_tool(&tool.summary_row.id, &handle.handle_id)
            .is_err());
        assert_eq!(std::fs::read_dir(outside).unwrap().count(), 0);
    }
    assert!(core
        .list_activity()
        .unwrap()
        .iter()
        .any(|a| a.title.starts_with("Exported tool")));
}

async fn ready(core: &AppCore) -> BuildState {
    let input = core.load_sample_csv().unwrap();
    let draft = core
        .draft_task(&input.handle.handle_id, "Clean orders")
        .unwrap();
    let started = core.start_build(&draft.draft_id, false).await.unwrap();
    for _ in 0..200 {
        let state = core.build_state(&started.build_id).unwrap();
        if state.phase == BuildPhase::Ready {
            return state;
        }
        assert_ne!(state.phase, BuildPhase::Failed, "{state:?}");
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("build did not finish");
}

#[tokio::test]
async fn another_sample_checks_the_same_candidate_and_preserves_the_reviewed_revision() {
    let tmp = tempfile::tempdir().unwrap();
    let core = core_at(tmp.path(), Arc::new(MemoryStore::default()));
    let state = ready(&core).await;
    let original = core
        .database()
        .build(&state.build_id)
        .unwrap()
        .unwrap()
        .state["draftRevision"]
        .clone();
    let good = core.load_sample_csv().unwrap();
    let checked = core
        .build_try_input(&state.build_id, &good.handle.handle_id)
        .unwrap();
    assert_eq!(checked.phase, BuildPhase::Ready);
    assert_eq!(checked.comparison.unwrap().rows_out, 18);
    assert_eq!(
        core.database()
            .build(&state.build_id)
            .unwrap()
            .unwrap()
            .state["draftRevision"],
        original
    );
    let bad =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/orders/negative/bad-date.csv");
    let bad = core.register_picked_file(&bad).unwrap();
    let failed = core
        .build_try_input(&state.build_id, &bad.handle_id)
        .unwrap();
    assert_eq!(failed.phase, BuildPhase::Failed);
    let failure = failed.failure.unwrap();
    assert_eq!(failure.kind, BuildFailureKind::Checks);
    assert!(
        failure.message.contains("row 1") || failure.message.contains("Row 1"),
        "{}",
        failure.message
    );
    assert!(failure.message.contains("created"), "{}", failure.message);
    assert!(core
        .save_tool(&state.build_id, "must not save")
        .await
        .is_err());
    let state = ready(&core).await;
    core.build_try_input(&state.build_id, &good.handle.handle_id)
        .unwrap();
    let draft = core.database().draft(&state.draft_id).unwrap().unwrap().1;
    let mapping = serde_json::from_value(draft["mapping"].clone()).unwrap();
    core.update_mapping(&state.draft_id, mapping, "edited.csv")
        .unwrap();
    assert!(core.save_tool(&state.build_id, "changed").await.is_err());
    assert!(core
        .build_try_input(&state.build_id, &good.handle.handle_id)
        .is_err());
}

#[tokio::test]
async fn history_wipe_keeps_tools_credentials_and_user_outputs_then_optional_tools_wipe() {
    let tmp = tempfile::tempdir().unwrap();
    let vault = Arc::new(MemoryStore::default());
    let key = SecretRef::new("meta-api-key").unwrap();
    let token = SecretRef::new("connection-gmail").unwrap();
    vault.set(&key, Secret::new(b"test-key".to_vec())).unwrap();
    vault
        .set(&token, Secret::new(b"test-token".to_vec()))
        .unwrap();
    let core = core_at(tmp.path(), vault.clone());
    core.select_route(ModelRoute::OfflineDemo).await.unwrap();
    ready(&core).await;
    let tool = core.install_sample_tool().unwrap();
    let run = staged(&core, &tool.summary_row.id, tmp.path(), "user-output.csv").await;
    core.commit_run(&run.run_id, &run.plan_hash).unwrap();
    core.database()
        .record_detection("gmail", "task_text", 1, core.now_ms())
        .unwrap();
    core.database().dismiss_suggestion("gmail", None).unwrap();
    core.database()
        .record_usage(&macroscope_storage::db::UsageRow {
            leg_id: "usage-leg".into(),
            parent_id: Some("build-parent".into()),
            model: "test".into(),
            usage: json!({"input_tokens": 10}),
            price_version: "test".into(),
            estimated_micros: None,
            reconciliation: "unknown".into(),
        })
        .unwrap();
    std::fs::write(
        tmp.path().join("data/builds/synthetic-workspace"),
        "synthetic",
    )
    .unwrap();
    let preview = core.delete_preview().unwrap();
    assert!(!preview.tools_included);
    assert!(preview
        .items
        .iter()
        .any(|i| i.label == "Build usage legs" && i.count == 1));
    assert!(preview
        .items
        .iter()
        .any(|i| i.label == "Drafts" && i.count > 0));
    core.delete_data(DeleteOptions {
        include_tools: false,
    })
    .await
    .unwrap();
    assert!(core
        .delete_preview()
        .unwrap()
        .items
        .iter()
        .all(|i| i.count == 0));
    assert_eq!(core.list_tools().unwrap().len(), 1);
    assert_eq!(core.list_receipts().unwrap().len(), 1);
    assert!(core.list_activity().unwrap().is_empty());
    assert!(core.database().pending_deletion().unwrap().is_none());
    assert!(vault.get(&key).unwrap().is_some() && vault.get(&token).unwrap().is_some());
    assert!(tmp.path().join("user-output.csv").exists());
    let run = staged(&core, &tool.summary_row.id, tmp.path(), "after-wipe.csv").await;
    core.discard_run(&run.run_id).unwrap();
    core.delete_data(DeleteOptions {
        include_tools: true,
    })
    .await
    .unwrap();
    assert!(core.list_tools().unwrap().is_empty());
    assert!(core.list_receipts().unwrap().is_empty());
    assert_eq!(
        std::fs::read_dir(tmp.path().join("data/tools"))
            .unwrap()
            .count(),
        0
    );
    assert!(vault.get(&key).unwrap().is_some() && vault.get(&token).unwrap().is_some());
}

#[tokio::test]
async fn startup_resumes_both_tombstone_scopes_before_recovering_jobs() {
    for include_tools in [false, true] {
        let tmp = tempfile::tempdir().unwrap();
        let vault = Arc::new(MemoryStore::default());
        let core = core_at(tmp.path(), vault.clone());
        let tool = core.install_sample_tool().unwrap();
        staged(&core, &tool.summary_row.id, tmp.path(), "unpublished.csv").await;
        ready(&core).await;
        core.database().begin_deletion(include_tools).unwrap();
        // Simulate a crash after deleting rows but before deleting files.
        core.database().delete_local_rows(include_tools).unwrap();
        drop(core);
        let restarted = core_at(tmp.path(), vault);
        assert!(restarted.database().pending_deletion().unwrap().is_none());
        assert!(restarted
            .delete_preview()
            .unwrap()
            .items
            .iter()
            .all(|i| i.count == 0));
        assert_eq!(restarted.list_tools().unwrap().is_empty(), include_tools);
        assert!(!tmp.path().join("unpublished.csv").exists());
        assert_eq!(
            restarted.model_access().await.egress,
            EgressState::LocalOnly
        );
    }
}

#[tokio::test]
async fn local_only_preserves_staged_local_runs_but_grant_revocation_blocks_commit() {
    let tmp = tempfile::tempdir().unwrap();
    let core = core_at(tmp.path(), Arc::new(MemoryStore::default()));
    let tool = core.install_sample_tool().unwrap();
    let id = &tool.summary_row.id;
    let first = staged(&core, id, tmp.path(), "local.csv").await;
    core.set_local_only(true).await.unwrap();
    assert!(matches!(
        core.commit_run(&first.run_id, &first.plan_hash).unwrap(),
        CommitResult::Committed(_)
    ));
    let second = staged(&core, id, tmp.path(), "revoked.csv").await;
    core.revoke_tool(id).unwrap();
    assert!(matches!(
        core.commit_run(&second.run_id, &second.plan_hash).unwrap(),
        CommitResult::Failed(FailureEnvelope {
            failure: RunFailure::Revoked { .. }
        })
    ));
    assert!(!tmp.path().join("revoked.csv").exists());
}

#[test]
fn sample_setup_refuses_file_and_directory_symlinks_without_modifying_targets() {
    use std::os::unix::fs::symlink;
    let tmp = tempfile::tempdir().unwrap();
    let core = core_at(tmp.path(), Arc::new(MemoryStore::default()));
    let target = tmp.path().join("private.csv");
    std::fs::write(&target, b"keep me").unwrap();
    let sample = tmp.path().join("data/samples/orders.csv");
    symlink(&target, &sample).unwrap();
    let result = core.load_sample_csv();
    assert_eq!(
        std::fs::read(&target).unwrap(),
        b"keep me",
        "sample setup overwrote a symlink target"
    );
    assert!(result.is_err());
    std::fs::remove_file(sample).unwrap();
    std::fs::remove_dir(tmp.path().join("data/samples")).unwrap();
    symlink(tmp.path(), tmp.path().join("data/samples")).unwrap();
    assert!(core.load_sample_csv().is_err());
    assert!(!tmp.path().join("orders.csv").exists());
}

#[tokio::test]
async fn invalid_control_names_are_rejected_before_destination_checks() {
    let tmp = tempfile::tempdir().unwrap();
    let core = core_at(tmp.path(), Arc::new(MemoryStore::default()));
    let tool = core.install_sample_tool().unwrap();
    let input = core.load_sample_csv().unwrap().handle;
    let dir = core.register_picked_dir(tmp.path()).unwrap();
    for name in ["a\u{1}b.csv", "a\u{7f}b.csv"] {
        std::fs::write(tmp.path().join(name), b"existing").unwrap();
        let result = core
            .stage_run(&tool.summary_row.id, &input.handle_id, &dir.handle_id, name)
            .await
            .unwrap();
        assert!(
            matches!(
                result,
                StageResult::Failed(FailureEnvelope {
                    failure: RunFailure::InputInvalid { .. }
                })
            ),
            "{result:?}"
        );
    }
}
