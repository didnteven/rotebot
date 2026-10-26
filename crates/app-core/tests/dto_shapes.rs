//! JSON shapes must match apps/desktop/src/backend/types.ts exactly.

use macroscope_app_core::dto::*;
use serde_json::json;

#[test]
fn model_access_and_tagged_unions_match_the_contract() {
    let status = ModelAccessStatus {
        api_key: ApiKeyStatus {
            present: true,
            fingerprint: Some("Key ending …abcd".into()),
            last_test: Some(KeyTestResult::Quota {
                checked_at_ms: 1,
                retry_at_ms: Some(2),
            }),
        },
        muse: MuseState::SignedIn {
            version: "1.4.13".into(),
            plan_summary: None,
        },
        selected_route: None,
        effective_route: Some(ModelRoute::OfflineDemo),
        blocked_reason: Some("x".into()),
        egress: EgressState::CloudAllowed,
        model: "muse-spark-1.3".into(),
    };
    assert_eq!(
        serde_json::to_value(&status).unwrap(),
        json!({
            "apiKey": {"present": true, "fingerprint": "Key ending …abcd",
                       "lastTest": {"kind": "quota", "checkedAtMs": 1, "retryAtMs": 2}},
            "muse": {"kind": "signed_in", "version": "1.4.13", "planSummary": null},
            "selectedRoute": null,
            "effectiveRoute": "offline_demo",
            "blockedReason": "x",
            "egress": "cloud_allowed",
            "model": "muse-spark-1.3"
        })
    );
    assert_eq!(
        serde_json::to_value(MuseState::Missing).unwrap(),
        json!({"kind": "missing"})
    );
    assert_eq!(
        serde_json::to_value(MuseState::NotSignedIn {
            version: "1".into()
        })
        .unwrap(),
        json!({"kind": "not_signed_in", "version": "1"})
    );
}

#[test]
fn run_results_are_untagged_with_a_failure_envelope() {
    let failure = StageResult::Failed(FailureEnvelope {
        failure: RunFailure::InputInvalid {
            message: "m".into(),
            row: Some(3),
            column: Some("created".into()),
        },
    });
    assert_eq!(
        serde_json::to_value(&failure).unwrap(),
        json!({"failure": {"kind": "input_invalid", "message": "m", "row": 3, "column": "created"}})
    );
    let receipt = Receipt {
        run_id: "r".into(),
        tool_id: "t".into(),
        tool_name: "n".into(),
        version_hash: "v".into(),
        plan_hash: "p".into(),
        input_digest: "i".into(),
        committed_digest: "c".into(),
        output_path: "/o.csv".into(),
        status: ReceiptStatus::OutcomeUnknown,
        at_ms: 5,
    };
    let v = serde_json::to_value(CommitResult::Committed(receipt)).unwrap();
    assert_eq!(v["status"], "outcome_unknown");
    assert_eq!(v["committedDigest"], "c");
    assert!(v.get("failure").is_none());
}

#[test]
fn tool_detail_flattens_the_summary_fields() {
    let detail = ToolDetail {
        summary_row: ToolSummary {
            id: "t".into(),
            name: "n".into(),
            version: 1,
            engine: "csv-mapping".into(),
            created_at_ms: 1,
            last_run_at_ms: None,
            run_count: 0,
            sample: true,
        },
        summary: vec![],
        mapping: serde_json::from_value(
            json!({"output_columns": [], "on_error": "stop_and_explain"}),
        )
        .unwrap(),
        input_columns: vec![],
        output_columns: vec![],
        access: vec![],
        version_hash: "h".into(),
    };
    let v = serde_json::to_value(&detail).unwrap();
    for key in [
        "id",
        "name",
        "version",
        "engine",
        "createdAtMs",
        "lastRunAtMs",
        "runCount",
        "sample",
        "versionHash",
        "inputColumns",
    ] {
        assert!(v.get(key).is_some(), "missing {key}");
    }
    assert!(v.get("summaryRow").is_none());
    assert_eq!(v["mapping"]["on_error"], "stop_and_explain");
}

#[test]
fn build_state_uses_camel_case_and_snake_case_phases() {
    let state = BuildState {
        build_id: "b".into(),
        draft_id: "d".into(),
        route: ModelRoute::MusePlan,
        phase: BuildPhase::Candidate,
        events: vec![],
        candidate: None,
        checks: vec![],
        comparison: Some(Comparison {
            input_columns: vec![],
            input_rows: vec![],
            output_columns: vec![],
            output_rows: vec![],
            rows_in: 1,
            rows_out: 1,
        }),
        spent_micros: None,
        failure: Some(BuildFailure {
            kind: BuildFailureKind::NoModelAccess,
            message: "m".into(),
        }),
        route_label: "Muse plan".into(),
    };
    let v = serde_json::to_value(&state).unwrap();
    assert_eq!(v["phase"], "candidate");
    assert_eq!(v["route"], "muse_plan");
    assert_eq!(v["failure"]["kind"], "no_model_access");
    assert_eq!(v["comparison"]["rowsIn"], 1);
    assert!(v.get("routeLabel").is_some() && v.get("spentMicros").is_some());
}
