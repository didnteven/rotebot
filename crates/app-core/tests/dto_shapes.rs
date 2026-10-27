//! JSON shapes must match apps/desktop/src/backend/types.ts exactly.

use macroscope_app_core::dto::*;
use serde_json::json;

#[test]
fn app_info_identifies_the_actual_store_and_build() {
    let info = AppInfo {
        version: "0.1.0".into(),
        platform: "macos".into(),
        mock: false,
        secret_store: macroscope_vault::SecretStoreKind::File,
        debug_build: true,
    };
    assert_eq!(
        serde_json::to_value(info).unwrap(),
        json!({
            "version": "0.1.0", "platform": "macos", "mock": false,
            "secretStore": "file", "debugBuild": true
        })
    );
}

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
        muse_unverified_ok: false,
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
            "model": "muse-spark-1.3",
            "museUnverifiedOk": false
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
        program: None,
        summary_row: ToolSummary {
            id: "t".into(),
            name: "n".into(),
            version: 1,
            engine: "csv-mapping".into(),
            created_at_ms: 1,
            last_run_at_ms: None,
            run_count: 0,
            sample: true,
            state: "enabled".into(),
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
        no_input: false,
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
    assert_eq!(v["state"], "enabled");
}

#[test]
fn build_state_uses_camel_case_and_snake_case_phases() {
    let mut state = BuildState {
        program: None,
        build_id: "b".into(),
        draft_id: "d".into(),
        route: ModelRoute::MusePlan,
        phase: BuildPhase::Candidate,
        events: vec![],
        candidate: None,
        checks: vec![],
        comparison: Some(Comparison {
            input_text: None,
            output_text: None,
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
    // buildTryInput returns this same DTO, including check failures on the new input.
    state.phase = BuildPhase::Failed;
    state.failure = Some(BuildFailure {
        kind: BuildFailureKind::Checks,
        message: "Row 1, column created: invalid date".into(),
    });
    let checked = serde_json::to_value(&state).unwrap();
    assert_eq!(checked["phase"], "failed");
    assert_eq!(checked["failure"]["kind"], "checks");
    assert_eq!(
        checked["failure"]["message"],
        "Row 1, column created: invalid date"
    );
}

#[test]
fn show_state_matches_the_contract() {
    let rec = ShowState::Recording {
        session_id: "s".into(),
        started_at_ms: 1,
        max_seconds: 300,
        paused: false,
        apps_seen: 2,
        matches: 1,
    };
    assert_eq!(
        serde_json::to_value(&rec).unwrap(),
        json!({"kind": "recording", "sessionId": "s", "startedAtMs": 1, "maxSeconds": 300,
               "paused": false, "appsSeen": 2, "matches": 1})
    );
    let fin = ShowState::Finished {
        session_id: "s".into(),
        duration_seconds: 9,
        apps_seen: vec!["Numbers".into()],
        suggestions: vec![ConnectionSuggestion {
            connector_id: "gmail".into(),
            connector_name: "Gmail".into(),
            message: "m".into(),
            signal_kind: "window_title".into(),
            count: 2,
        }],
    };
    let v = serde_json::to_value(&fin).unwrap();
    assert_eq!(v["kind"], "finished");
    assert_eq!(v["durationSeconds"], 9);
    assert_eq!(v["suggestions"][0]["connectorName"], "Gmail");
    assert_eq!(v["suggestions"][0]["signalKind"], "window_title");
    assert_eq!(
        serde_json::to_value(ShowState::Idle {
            accessibility: "denied".into()
        })
        .unwrap(),
        json!({"kind": "idle", "accessibility": "denied"})
    );
}

#[test]
fn connector_info_matches_the_contract() {
    let info = ConnectorInfo {
        id: "gmail".into(),
        name: "Gmail".into(),
        category: "Email".into(),
        mcp_url: "https://gmailmcp.googleapis.com/mcp/v1".into(),
        registration: "preregistered".into(),
        verification: "verified-live".into(),
        docs_url: None,
        state: ConnectionState::Connected {
            since_ms: 5,
            tool_count: 12,
        },
        has_client_credentials: true,
        client_setup: Some("x".into()),
        redirect_uri: None,
    };
    let v = serde_json::to_value(&info).unwrap();
    assert_eq!(
        v["state"],
        json!({"kind": "connected", "sinceMs": 5, "toolCount": 12})
    );
    for key in [
        "mcpUrl",
        "docsUrl",
        "hasClientCredentials",
        "clientSetup",
        "redirectUri",
    ] {
        assert!(v.get(key).is_some(), "missing {key}");
    }
    assert_eq!(
        serde_json::to_value(ConnectionState::NeedsReauth).unwrap(),
        json!({"kind": "needs_reauth"})
    );
    let start = ConnectStart {
        connector_id: "c".into(),
        authorization_url: "u".into(),
        pending_id: "p".into(),
    };
    assert_eq!(
        serde_json::to_value(start).unwrap(),
        json!({"connectorId": "c", "authorizationUrl": "u", "pendingId": "p"})
    );
}

#[test]
fn privacy_control_shapes_match_the_contract() {
    assert_eq!(
        serde_json::to_value(DeletePreview {
            items: vec![DeleteItem {
                label: "Drafts".into(),
                count: 3
            }],
            tools_included: false
        })
        .unwrap(),
        json!({"items": [{"label": "Drafts", "count": 3}], "toolsIncluded": false})
    );
    assert_eq!(
        serde_json::to_value(DeleteOptions {
            include_tools: true
        })
        .unwrap(),
        json!({"includeTools": true})
    );
    assert_eq!(
        serde_json::to_value(StageResult::Failed(FailureEnvelope {
            failure: RunFailure::Revoked {
                message: "Paused".into()
            }
        }))
        .unwrap(),
        json!({"failure": {"kind": "revoked", "message": "Paused"}})
    );
    assert_eq!(
        serde_json::to_value(Some("/Exports/tool-v1".to_string())).unwrap(),
        json!("/Exports/tool-v1")
    );
    assert_eq!(serde_json::to_value(None::<String>).unwrap(), json!(null));
}
