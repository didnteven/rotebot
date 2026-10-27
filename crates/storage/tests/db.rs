use macroscope_storage::db::*;
use serde_json::json;

const KEY: [u8; 32] = [7; 32];

fn tool(id: &str, created_ms: i64) -> ToolRow {
    ToolRow {
        id: id.into(),
        name: format!("Tool {id}"),
        active_version: Some("v1".into()),
        state: "enabled".into(),
        sample: false,
        created_ms,
        last_run_ms: None,
        run_count: 0,
        detail: json!({"summary": ["Map rows"]}),
    }
}

#[test]
fn wrong_key_is_rejected_and_file_has_no_plaintext() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.db");
    let marker = "PLAINTEXT-MARKER-ada-example";
    {
        let db = Db::open(&path, &KEY).unwrap();
        db.put_tool(&ToolRow {
            name: marker.into(),
            ..tool("t1", 1)
        })
        .unwrap();
        db.add_activity(&ActivityRow {
            id: "a1".into(),
            at_ms: 1,
            kind: "build".into(),
            title: marker.into(),
            detail: None,
        })
        .unwrap();
    }
    for entry in std::fs::read_dir(dir.path()).unwrap() {
        let bytes = std::fs::read(entry.unwrap().path()).unwrap();
        assert!(!bytes.windows(marker.len()).any(|w| w == marker.as_bytes()));
        assert!(!bytes.starts_with(b"SQLite format 3"));
    }
    assert!(matches!(
        Db::open(&path, &[8; 32]),
        Err(DbError::KeyRejected)
    ));
    let reopened = Db::open(&path, &KEY).unwrap();
    assert_eq!(reopened.tool("t1").unwrap().unwrap().name, marker);
}

#[test]
fn migrations_are_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.db");
    assert_eq!(
        Db::open(&path, &KEY).unwrap().schema_version().unwrap(),
        SCHEMA_VERSION
    );
    assert_eq!(
        Db::open(&path, &KEY).unwrap().schema_version().unwrap(),
        SCHEMA_VERSION
    );
}

#[test]
fn tools_versions_grants_and_cascading_delete() {
    let db = Db::open_in_memory(&KEY).unwrap();
    db.put_tool(&tool("old", 1)).unwrap();
    db.put_tool(&tool("new", 2)).unwrap();
    assert_eq!(
        db.list_tools()
            .unwrap()
            .iter()
            .map(|t| t.id.as_str())
            .collect::<Vec<_>>(),
        ["new", "old"]
    );
    let v = VersionRow {
        tool_id: "new".into(),
        hash: "h1".into(),
        manifest: json!({"schema": "macroscope.tool/v1"}),
        mapping: json!({"output_columns": []}),
        created_ms: 2,
    };
    db.insert_version(&v).unwrap();
    assert!(db.insert_version(&v).is_err(), "versions are immutable");
    assert_eq!(db.version("new", "h1").unwrap(), Some(v));

    db.put_grant("g1", "new", "h1", 1, &json!({"read": "input"}))
        .unwrap();
    db.put_grant("g2", "new", "h1", 2, &json!({"read": "input2"}))
        .unwrap();
    assert_eq!(db.active_grant("new", "h1").unwrap().unwrap().1, 2);
    assert_eq!(db.revoke_grants("new", 9).unwrap(), 2);
    assert!(db.active_grant("new", "h1").unwrap().is_none());

    db.put_run(&RunRow {
        id: "r1".into(),
        tool_id: "new".into(),
        version_hash: "h1".into(),
        state: "committed".into(),
        plan_hash: Some("p".into()),
        receipt: Some(json!({"committedDigest": "abc"})),
        failure: None,
        created_ms: 3,
        run_dir: None,
    })
    .unwrap();
    assert_eq!(db.runs_for_tool("new").unwrap().len(), 1);
    db.delete_tool("new").unwrap();
    assert!(db.tool("new").unwrap().is_none());
    assert!(db.version("new", "h1").unwrap().is_none());
    assert!(db.run("r1").unwrap().is_none());
}

#[test]
fn drafts_builds_and_budget_reservations() {
    let db = Db::open_in_memory(&KEY).unwrap();
    db.put_draft("d1", 1, &json!({"goal": "x"}), 5).unwrap();
    db.put_draft("d1", 2, &json!({"goal": "y"}), 6).unwrap();
    assert_eq!(db.draft("d1").unwrap(), Some((2, json!({"goal": "y"}))));
    let build = |id: &str, phase: &str, created_ms, spent| BuildRow {
        id: id.into(),
        draft_id: "d1".into(),
        route: "offline_demo".into(),
        phase: phase.into(),
        reserved_micros: 2_000_000,
        spent_micros: spent,
        state: json!({}),
        created_ms,
        updated_ms: created_ms,
    };
    db.put_build(&build("b1", "ready", 10, Some(340_000)))
        .unwrap();
    db.put_build(&build("b2", "writing", 20, None)).unwrap();
    db.put_build(&build("b0", "failed", 1, None)).unwrap();
    assert_eq!(db.unfinished_builds().unwrap(), ["b2"]);
    // Spent replaces the reservation once known; unknown spend keeps the reservation.
    assert_eq!(db.reserved_micros_since(5).unwrap(), 340_000 + 2_000_000);
    assert_eq!(db.build("b1").unwrap().unwrap().spent_micros, Some(340_000));
}

#[test]
fn outbound_bodies_expire_after_seven_days_and_rows_after_thirty() {
    let db = Db::open_in_memory(&KEY).unwrap();
    let row = |id: &str, at_ms| OutboundRow {
        id: id.into(),
        at_ms,
        purpose: "build".into(),
        destination: "Meta API".into(),
        route: "api_key".into(),
        synthetic_only: true,
        cost_micros: None,
        payload_text: Some("synthetic packet".into()),
    };
    db.insert_outbound(&row("o1", 0)).unwrap();
    db.insert_outbound(&row("o2", OUTBOUND_PAYLOAD_TTL_MS))
        .unwrap();
    assert_eq!(
        db.purge_expired_outbound(OUTBOUND_PAYLOAD_TTL_MS).unwrap(),
        (1, 0)
    );
    let rows = db.list_outbound().unwrap();
    assert_eq!(
        rows.iter().find(|r| r.id == "o1").unwrap().payload_text,
        None
    );
    assert!(rows
        .iter()
        .find(|r| r.id == "o2")
        .unwrap()
        .payload_text
        .is_some());
    assert_eq!(
        db.purge_expired_outbound(OUTBOUND_METADATA_TTL_MS)
            .unwrap()
            .1,
        1
    );
    db.set_outbound_cost("o2", Some(12)).unwrap();
    assert_eq!(db.list_outbound().unwrap()[0].cost_micros, Some(12));
    assert_eq!(db.delete_outbound_history().unwrap(), 1);
}

#[test]
fn detections_keep_counts_only_and_dismissals_snooze() {
    let db = Db::open_in_memory(&KEY).unwrap();
    db.record_detection("gmail", "window_title", 1, 100)
        .unwrap();
    db.record_detection("gmail", "window_title", 2, 50).unwrap();
    db.record_detection("slack", "foreground_app", 1, 70)
        .unwrap();
    let d = db.detections().unwrap();
    assert_eq!(
        d[0],
        DetectionRow {
            connector_id: "gmail".into(),
            signal_kind: "window_title".into(),
            count: 3,
            first_ms: 100,
            last_ms: 100
        }
    );
    db.dismiss_suggestion("gmail", None).unwrap();
    db.dismiss_suggestion("slack", Some(200)).unwrap();
    assert_eq!(db.hidden_suggestions(150).unwrap(), ["gmail", "slack"]);
    assert_eq!(db.hidden_suggestions(250).unwrap(), ["gmail"]);
}

#[test]
fn settings_connectors_activity_and_stats() {
    let db = Db::open_in_memory(&KEY).unwrap();
    db.set_setting("selected_route", &json!("muse_plan"))
        .unwrap();
    assert_eq!(
        db.setting("selected_route").unwrap(),
        Some(json!("muse_plan"))
    );
    assert_eq!(db.setting("missing").unwrap(), None);
    db.set_connector_state("gmail", &json!({"state": "connected"}))
        .unwrap();
    assert_eq!(db.connector_states().unwrap()[0].0, "gmail");
    for i in 0..3 {
        db.add_activity(&ActivityRow {
            id: format!("a{i}"),
            at_ms: i,
            kind: "run".into(),
            title: "t".into(),
            detail: None,
        })
        .unwrap();
    }
    assert_eq!(
        db.list_activity(2)
            .unwrap()
            .iter()
            .map(|a| a.at_ms)
            .collect::<Vec<_>>(),
        [2, 1]
    );
    let stats = db.stats().unwrap();
    assert_eq!(
        stats.iter().find(|s| s.table == "activity").unwrap().rows,
        3
    );
}

#[test]
fn usage_legs_deduplicate_by_id() {
    let db = Db::open_in_memory(&KEY).unwrap();
    let leg = |micros| UsageRow {
        leg_id: "build-1:turn-1".into(),
        parent_id: Some("build-1".into()),
        model: "muse-spark-1.3".into(),
        usage: json!({"input_tokens": 100, "output_tokens": 20}),
        price_version: "meta-standard-2026-10-27".into(),
        estimated_micros: Some(micros),
        reconciliation: "estimated".into(),
    };
    db.record_usage(&leg(10)).unwrap();
    db.record_usage(&leg(12)).unwrap();
    assert_eq!(db.usage_estimated_total("build-1").unwrap(), Some(12));
    db.record_usage(&UsageRow {
        leg_id: "build-1:reminder".into(),
        estimated_micros: None,
        reconciliation: "unknown".into(),
        ..leg(0)
    })
    .unwrap();
    assert_eq!(db.usage_estimated_total("build-1").unwrap(), None);
}

#[test]
fn missing_usage_legs_keep_the_reservation_unknown() {
    let db = Db::open_in_memory(&KEY).unwrap();
    assert_eq!(db.usage_estimated_total("not-recorded").unwrap(), None);
}

#[test]
fn a_future_schema_is_refused_without_modifying_it() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("future.db");
    drop(Db::open(&path, &KEY).unwrap());
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch(&format!(
        "PRAGMA key = \"x'{}'\"; PRAGMA user_version = 999;",
        hex::encode(KEY)
    ))
    .unwrap();
    drop(conn);
    assert!(Db::open(&path, &KEY).is_err());
}
