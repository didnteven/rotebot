use macroscope_app_core::runner::RunnerEngine;
use macroscope_domain::{TaskContract, ToolId};
use macroscope_effect_broker::{
    self as broker, Approval, Epochs, SelectedFile, SelectedRoot, StageRequest,
};
use std::path::Path;
#[test]
fn js_orders_child_stage_approve_commit_preserves_frozen_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let input = root.join("input.csv");
    std::fs::write(&input, include_bytes!("../../../fixtures/orders/input.csv")).unwrap();
    let contract: TaskContract =
        serde_json::from_str(include_str!("../../../fixtures/orders/task.json")).unwrap();
    let epochs = Epochs {
        grant_revision: 1,
        policy_epoch: 1,
    };
    let request = || StageRequest {
        tool_id: ToolId::new(),
        version_hash: "synthetic-js-orders".into(),
        mapping: contract.mapping.clone(),
        contract: contract.clone(),
        input: SelectedFile::from_picker(&input).unwrap(),
        destination: SelectedRoot::from_picker(&root).unwrap(),
        basename: "orders_ready.csv".into(),
        limits: macroscope_executor::Limits::default(),
        epochs,
        staging_parent: root.clone(),
        key: [7; 32],
    };
    let engine = RunnerEngine::new(
        Path::new(env!("CARGO_BIN_EXE_macroscope-runner")),
        Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../experiments/runtime-gate-v2/profile.sb"
        )),
    )
    .unwrap();
    let mut staged = engine
        .stage_synthetic(
            request(),
            include_str!("../../../experiments/runtime-gate-v2/orders.js"),
            macroscope_runner::Limits::default(),
        )
        .unwrap();
    let expected = include_bytes!("../../../fixtures/orders/expected.csv");
    assert_eq!(staged.preview().unwrap(), expected);
    assert_eq!(staged.plan().engine, "js-quickjs");
    assert!(!root.join("orders_ready.csv").exists());
    let plan_hash = staged.plan_hash().to_string();
    let approval = Approval {
        plan_hash: plan_hash.clone(),
        approved_at_ms: 1,
    };
    broker::approve(&mut staged, &plan_hash, approval.clone()).unwrap();
    broker::commit(staged, approval, epochs).unwrap();
    assert_eq!(
        std::fs::read(root.join("orders_ready.csv")).unwrap(),
        expected
    );
    assert_eq!(
        std::fs::read(&input).unwrap(),
        include_bytes!("../../../fixtures/orders/input.csv")
    );
    assert!(engine
        .stage_synthetic(
            request(),
            "function run(){macroscope.output('orders_ready.csv','x')}",
            macroscope_runner::Limits::default()
        )
        .is_err());
}
