//! End-to-end Tell flow through `AppCore`, the same calls the desktop shell makes.

use macroscope_app_core::dto::*;
use macroscope_app_core::{AppCore, CoreConfig, MuseProbe};
use macroscope_muse_client::PlanStatus;
use macroscope_vault::{MemoryStore, SecretStore};
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;

struct FakeMuse(PlanStatus);

impl MuseProbe for FakeMuse {
    fn plan_status(&self) -> Pin<Box<dyn Future<Output = PlanStatus> + Send + '_>> {
        let status = self.0.clone();
        Box::pin(async move { status })
    }
    fn version(&self) -> Option<String> {
        Some("1.4.13".into())
    }
}

fn core_at(dir: &Path, vault: Arc<dyn SecretStore>, muse: PlanStatus) -> AppCore {
    AppCore::new(CoreConfig {
        data_dir: dir.join("data"),
        vault,
        gateway_base_url: "http://127.0.0.1:9".into(),
        home: dir.join("home"),
        muse: Arc::new(FakeMuse(muse)),
        app_version: "0.1.0".into(),
        platform: "macos".into(),
        now_ms: CoreConfig::system_clock(),
    })
    .unwrap()
}

fn core(dir: &Path) -> AppCore {
    core_at(
        dir,
        Arc::new(MemoryStore::default()),
        PlanStatus::NotSignedIn,
    )
}

async fn wait_terminal(core: &AppCore, build_id: &str) -> BuildState {
    for _ in 0..200 {
        let state = core.build_state(build_id).unwrap();
        if matches!(
            state.phase,
            BuildPhase::Ready | BuildPhase::Failed | BuildPhase::Stopped
        ) {
            return state;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    panic!("build did not finish");
}

fn out_dir(core: &AppCore, root: &Path) -> FileHandle {
    let dir = root.join("Reports");
    std::fs::create_dir_all(&dir).unwrap();
    core.register_picked_dir(&dir).unwrap()
}

#[tokio::test]
async fn sample_tell_flow_creates_exactly_the_expected_csv() {
    let tmp = tempfile::tempdir().unwrap();
    let core = core(tmp.path());
    let access = core.model_access().await;
    assert_eq!(access.effective_route, Some(ModelRoute::OfflineDemo));
    assert!(
        access.blocked_reason.is_some(),
        "explains how to get a model route"
    );

    let preview = core.load_sample_csv().unwrap();
    assert_eq!(preview.row_count, 18);
    let draft = core
        .draft_task(
            &preview.handle.handle_id,
            "Clean the orders export and email it with Gmail",
            vec!["gmail".into()],
        )
        .unwrap();
    assert!(
        draft.questions.is_empty(),
        "01/16/26 makes the month-first order unambiguous"
    );
    assert!(draft.ready_to_build, "{:?}", draft.blockers);
    assert_eq!(draft.output_name, "orders_ready.csv");
    assert_eq!(draft.mentioned_connectors, ["gmail"]);

    let outbound = core.outbound_preview(&draft.draft_id).await.unwrap();
    assert_eq!(outbound.route, ModelRoute::OfflineDemo);
    assert!(!outbound.contains_real_rows);

    let started = core.start_build(&draft.draft_id, false).await.unwrap();
    let built = wait_terminal(&core, &started.build_id).await;
    assert_eq!(built.phase, BuildPhase::Ready, "{:?}", built.failure);
    assert!(built.checks.iter().all(|c| c.passed), "{:#?}", built.checks);
    let phases: Vec<_> = built.events.iter().map(|e| e.phase).collect();
    assert_eq!(
        phases,
        [
            BuildPhase::Queued,
            BuildPhase::Writing,
            BuildPhase::Candidate,
            BuildPhase::Checking,
            BuildPhase::Ready
        ]
    );
    let cmp = built.comparison.as_ref().unwrap();
    assert_eq!(cmp.output_columns, ["Order", "Date", "Amount"]);

    let tool = core
        .save_tool(&built.build_id, "Clean import CSV")
        .await
        .unwrap();
    assert_eq!(core.list_tools().unwrap().len(), 1);
    assert_eq!(tool.output_columns, ["Order", "Date", "Amount"]);

    let output = out_dir(&core, tmp.path());
    let input = core.load_sample_csv().unwrap().handle;
    let staged = match core
        .stage_run(
            &tool.summary_row.id,
            &input.handle_id,
            &output.handle_id,
            "orders_ready.csv",
        )
        .unwrap()
    {
        StageResult::Staged(s) => *s,
        StageResult::Failed(f) => panic!("{:?}", f.failure),
    };
    assert_eq!((staged.rows_in, staged.rows_out), (18, 18));
    assert!(
        !tmp.path().join("Reports/orders_ready.csv").exists(),
        "staging writes nothing live"
    );

    let receipt = match core.commit_run(&staged.run_id, &staged.plan_hash).unwrap() {
        CommitResult::Committed(r) => r,
        CommitResult::Failed(f) => panic!("{:?}", f.failure),
    };
    let written = std::fs::read(tmp.path().join("Reports/orders_ready.csv")).unwrap();
    assert_eq!(
        written,
        include_bytes!("../../../fixtures/orders/expected.csv")
    );
    assert_eq!(receipt.status, ReceiptStatus::Committed);
    assert_eq!(core.list_receipts().unwrap().len(), 1);
    assert_eq!(
        core.get_tool(&tool.summary_row.id)
            .unwrap()
            .summary_row
            .run_count,
        1
    );
    assert!(
        !String::from_utf8_lossy(&written).contains("Ada Example"),
        "customer names are not copied"
    );

    // Second run with the same name refuses instead of overwriting.
    let again = core
        .stage_run(
            &tool.summary_row.id,
            &input.handle_id,
            &output.handle_id,
            "orders_ready.csv",
        )
        .unwrap();
    assert!(matches!(
        again,
        StageResult::Failed(FailureEnvelope {
            failure: RunFailure::DestinationExists { .. }
        })
    ));
}

#[tokio::test]
async fn ambiguous_dates_block_until_answered_and_answer_changes_the_mapping() {
    let tmp = tempfile::tempdir().unwrap();
    let core = core(tmp.path());
    let file = tmp.path().join("payments.csv");
    std::fs::write(
        &file,
        "paid_on,total,payer\n03/04/26,$10.00,A\n05/06/26,\"$1,200.50\",B\n",
    )
    .unwrap();
    let handle = core.register_picked_file(&file).unwrap();
    let draft = core.draft_task(&handle.handle_id, "", vec![]).unwrap();
    assert!(!draft.ready_to_build);
    assert_eq!(
        draft.questions[0].prompt,
        "Does 03/04/26 mean March 4 or April 3?"
    );
    let answered = core
        .answer_question(&draft.draft_id, "date-format", "dd_mm_yy")
        .unwrap();
    assert!(answered.ready_to_build, "{:?}", answered.blockers);
    let built = {
        let b = core.start_build(&answered.draft_id, false).await.unwrap();
        wait_terminal(&core, &b.build_id).await
    };
    assert_eq!(built.phase, BuildPhase::Ready, "{:?}", built.failure);
    let cmp = built.comparison.unwrap();
    assert_eq!(cmp.output_rows[0][0], "2026-04-03", "DD/MM/YY applied");
    assert!(core
        .answer_question(&draft.draft_id, "date-format", "nonsense")
        .is_err());
}

#[tokio::test]
async fn bad_rows_in_a_later_input_stop_with_row_and_column_and_write_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let core = core(tmp.path());
    let tool = core.install_sample_tool().unwrap();
    assert!(tool.summary_row.sample);
    assert_eq!(
        core.install_sample_tool().unwrap().summary_row.id,
        tool.summary_row.id,
        "idempotent"
    );
    let bad = tmp.path().join("bad.csv");
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/orders/negative/bad-date.csv"),
        &bad,
    )
    .unwrap();
    let input = core.register_picked_file(&bad).unwrap();
    let output = out_dir(&core, tmp.path());
    match core
        .stage_run(
            &tool.summary_row.id,
            &input.handle_id,
            &output.handle_id,
            "x.csv",
        )
        .unwrap()
    {
        StageResult::Failed(FailureEnvelope {
            failure: RunFailure::InputInvalid { row, column, .. },
        }) => {
            assert_eq!(row, Some(1));
            assert_eq!(column.as_deref(), Some("created"));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        std::fs::read_dir(tmp.path().join("Reports"))
            .unwrap()
            .count(),
        0
    );
}

#[tokio::test]
async fn approval_must_match_and_staged_runs_can_be_discarded() {
    let tmp = tempfile::tempdir().unwrap();
    let core = core(tmp.path());
    let tool = core.install_sample_tool().unwrap();
    let input = core.load_sample_csv().unwrap().handle;
    let output = out_dir(&core, tmp.path());
    let StageResult::Staged(staged) = core
        .stage_run(
            &tool.summary_row.id,
            &input.handle_id,
            &output.handle_id,
            "a.csv",
        )
        .unwrap()
    else {
        panic!()
    };
    assert!(core
        .commit_run(&staged.run_id, "not-the-plan-hash")
        .is_err());
    let StageResult::Staged(staged) = core
        .stage_run(
            &tool.summary_row.id,
            &input.handle_id,
            &output.handle_id,
            "a.csv",
        )
        .unwrap()
    else {
        panic!()
    };
    core.discard_run(&staged.run_id).unwrap();
    assert!(core.commit_run(&staged.run_id, &staged.plan_hash).is_err());
    assert!(!tmp.path().join("Reports/a.csv").exists());
}

#[tokio::test]
async fn api_key_route_requires_review_and_key_is_never_stored_in_the_database() {
    let tmp = tempfile::tempdir().unwrap();
    let vault: Arc<dyn SecretStore> = Arc::new(MemoryStore::default());
    let core = core_at(tmp.path(), vault.clone(), PlanStatus::NotSignedIn);
    let key = "mk-test-canary-0123456789-abcd";
    let access = core.save_api_key(&format!("  {key}\n")).await;
    assert!(access.is_err(), "keys with line breaks are refused");
    let access = core.save_api_key(key).await.unwrap();
    assert!(access.api_key.present);
    assert_eq!(
        access.api_key.fingerprint.as_deref(),
        Some("Key ending …abcd")
    );
    assert_eq!(access.effective_route, Some(ModelRoute::ApiKey));

    let preview = core.load_sample_csv().unwrap();
    let draft = core
        .draft_task(&preview.handle.handle_id, "", vec![])
        .unwrap();
    let outbound = core.outbound_preview(&draft.draft_id).await.unwrap();
    assert_eq!(outbound.route, ModelRoute::ApiKey);
    assert!(
        !outbound.payload_text.contains("Ada Example"),
        "no real rows in the packet"
    );
    assert!(
        !outbound.payload_text.contains("ORD-001"),
        "no real rows in the packet"
    );
    assert!(
        core.start_build(&draft.draft_id, false).await.is_err(),
        "cloud builds need the review"
    );

    // The real build fails fast against the closed local port, with a typed failure.
    let b = core.start_build(&draft.draft_id, true).await.unwrap();
    let state = wait_terminal(&core, &b.build_id).await;
    assert_eq!(state.phase, BuildPhase::Failed);
    assert_eq!(
        core.list_outbound().unwrap().len(),
        1,
        "the send was recorded before dispatch"
    );

    drop(core);
    for entry in walk(&tmp.path().join("data")) {
        let bytes = std::fs::read(&entry).unwrap();
        assert!(
            !bytes.windows(key.len()).any(|w| w == key.as_bytes()),
            "{entry:?}"
        );
    }
}

#[tokio::test]
async fn muse_plan_route_needs_sign_in_and_explicit_ok() {
    let tmp = tempfile::tempdir().unwrap();
    let signed_in = PlanStatus::SignedIn {
        summary: serde_json::json!({"plan": "Muse Plus"}),
    };
    let core = core_at(tmp.path(), Arc::new(MemoryStore::default()), signed_in);
    let access = core.model_access().await;
    assert_eq!(access.effective_route, Some(ModelRoute::MusePlan));
    assert!(
        matches!(&access.muse, MuseState::SignedIn { plan_summary: Some(p), .. } if p == "Muse Plus")
    );
    let preview = core.load_sample_csv().unwrap();
    let draft = core
        .draft_task(&preview.handle.handle_id, "", vec![])
        .unwrap();
    let b = core.start_build(&draft.draft_id, true).await.unwrap();
    let state = wait_terminal(&core, &b.build_id).await;
    assert_eq!(state.phase, BuildPhase::Failed);
    let failure = state.failure.unwrap();
    assert_eq!(
        failure.kind,
        BuildFailureKind::NoModelAccess,
        "{}",
        failure.message
    );
}

#[tokio::test]
async fn local_only_falls_back_to_offline_and_explains_why() {
    let tmp = tempfile::tempdir().unwrap();
    let core = core(tmp.path());
    core.save_api_key("mk-test-canary-0123456789-wxyz")
        .await
        .unwrap();
    let access = core.set_local_only(true).await.unwrap();
    assert_eq!(access.egress, EgressState::LocalOnly);
    assert_eq!(access.effective_route, Some(ModelRoute::OfflineDemo));
    assert!(access.blocked_reason.unwrap().contains("Local only"));
    let access = core.set_local_only(false).await.unwrap();
    assert_eq!(access.effective_route, Some(ModelRoute::ApiKey));
}

#[tokio::test]
async fn tools_and_settings_survive_restart_and_delete_removes_versions() {
    let tmp = tempfile::tempdir().unwrap();
    let vault: Arc<dyn SecretStore> = Arc::new(MemoryStore::default());
    let tool_id = {
        let core = core_at(tmp.path(), vault.clone(), PlanStatus::NotSignedIn);
        core.select_route(ModelRoute::OfflineDemo).await.unwrap();
        core.install_sample_tool().unwrap().summary_row.id
    };
    let core = core_at(tmp.path(), vault.clone(), PlanStatus::NotSignedIn);
    assert_eq!(
        core.model_access().await.selected_route,
        Some(ModelRoute::OfflineDemo)
    );
    assert_eq!(core.list_tools().unwrap()[0].id, tool_id);
    core.delete_tool(&tool_id).unwrap();
    assert!(core.list_tools().unwrap().is_empty());
    assert!(!tmp.path().join("data/tools").join(&tool_id).exists());
    assert!(!core.list_activity().unwrap().is_empty());
}

fn walk(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out = vec![];
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(walk(&p));
            } else {
                out.push(p);
            }
        }
    }
    out
}

#[tokio::test]
async fn local_only_choice_persists_across_restart() {
    let tmp = tempfile::tempdir().unwrap();
    let vault: Arc<dyn SecretStore> = Arc::new(MemoryStore::default());
    {
        let core = core_at(tmp.path(), vault.clone(), PlanStatus::NotSignedIn);
        assert_eq!(core.model_access().await.egress, EgressState::CloudAllowed);
        core.set_local_only(true).await.unwrap();
    }
    let core = core_at(tmp.path(), vault, PlanStatus::NotSignedIn);
    assert_eq!(core.model_access().await.egress, EgressState::LocalOnly);
}
