use super::*;
use tempfile::TempDir;

struct Fixture {
    _dir: TempDir,
    input: PathBuf,
    root: PathBuf,
    staging: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        // macOS /var and /tmp are symlinks; selections use their real paths.
        let base = dir.path().canonicalize().unwrap();
        let input = base.join("input.csv");
        fs::write(&input, include_bytes!("../../../fixtures/orders/input.csv")).unwrap();
        let root = base.join("output");
        fs::create_dir(&root).unwrap();
        let staging = base.join("staging");
        fs::create_dir(&staging).unwrap();
        Self {
            _dir: dir,
            input,
            root,
            staging,
        }
    }
    fn request(&self) -> StageRequest {
        let contract: TaskContract =
            serde_json::from_str(include_str!("../../../fixtures/orders/task.json")).unwrap();
        StageRequest {
            tool_id: ToolId("orders".into()),
            version_hash: "immutable-version-7".into(),
            mapping: contract.mapping.clone(),
            contract,
            input: SelectedFile::from_picker(&self.input).unwrap(),
            destination: SelectedRoot::from_picker(&self.root).unwrap(),
            basename: "orders_ready.csv".into(),
            limits: Limits::default(),
            epochs: epochs(),
            staging_parent: self.staging.clone(),
            key: [42; 32],
        }
    }
    fn stage(&self) -> StagedPlan {
        stage(self.request()).unwrap()
    }
    fn output(&self) -> PathBuf {
        self.root.join("orders_ready.csv")
    }
}
fn epochs() -> Epochs {
    Epochs {
        grant_revision: 3,
        policy_epoch: 7,
    }
}
fn approved(staged: &mut StagedPlan) -> Approval {
    let approval = Approval {
        plan_hash: staged.plan_hash().into(),
        approved_at_ms: now_ms(),
    };
    let hash = staged.plan_hash().to_owned();
    approve(staged, &hash, approval.clone()).unwrap();
    approval
}

#[test]
fn full_run_preserves_source_and_commits_reviewed_bytes_without_executor() {
    let f = Fixture::new();
    let before = fs::read(&f.input).unwrap();
    let mut staged = f.stage();
    assert_eq!(staged.rows(), 18);
    let reviewed = staged.preview().unwrap();
    assert_eq!(
        reviewed,
        include_bytes!("../../../fixtures/orders/expected.csv")
    );
    let reviewed_hash = digest(&reviewed);
    let run_dir = staged.run_dir.clone();
    let plan_hash = staged.plan_hash().to_owned();
    let encrypted = fs::read(run_dir.join(CIPHERTEXT_FILE)).unwrap();
    assert!(!encrypted.windows(20).any(|w| w == &reviewed[..20]));
    assert!(fs::metadata(run_dir.join(CIPHERTEXT_FILE))
        .unwrap()
        .permissions()
        .readonly());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&run_dir).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }
    let approval = approved(&mut staged);
    // This candidate would produce different bytes, but StagedPlan has no
    // mapping and commit has no mapping/callback argument. It cannot be rerun.
    let mut different = f.request();
    different.mapping.output_columns[0].transform = macroscope_domain::Transform::Constant {
        value: "DIFFERENT".into(),
    };
    assert_ne!(
        apply_mapping(&different.mapping, &before, &Limits::default())
            .unwrap()
            .sha256,
        reviewed_hash
    );
    drop(different);
    let receipt = commit(staged, approval, epochs()).unwrap();
    assert_eq!(fs::read(f.output()).unwrap(), reviewed);
    assert_eq!(receipt.committed_digest, reviewed_hash);
    assert_eq!(receipt.plan_hash, plan_hash);
    assert_eq!(digest(&fs::read(&f.input).unwrap()), digest(&before));
    assert_eq!(receipt.input_digest, digest(&before));
    assert_eq!(receipt.status, ReceiptStatus::Committed);
    let serialized = serde_json::to_vec(&receipt).unwrap();
    assert_eq!(
        serde_json::from_slice::<Receipt>(&serialized).unwrap(),
        receipt
    );
    let events = fs::read_to_string(run_dir.join(JOURNAL_FILE)).unwrap();
    let names: Vec<_> = events
        .lines()
        .map(|l| {
            serde_json::from_str::<serde_json::Value>(l).unwrap()["event"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert_eq!(
        names,
        ["staged", "approved", "intent", "published", "receipt"]
    );
    assert_eq!(
        reconcile(&run_dir).unwrap(),
        ReconcileOutcome::ReconciledPublished
    );
}

#[test]
fn generated_text_input_skips_csv_requirements_but_csv_input_enforces_them() {
    let f = Fixture::new();
    let input = b"INFO synthetic start\nERROR failed,with,commas\n";
    fs::write(&f.input, input).unwrap();
    let mut request = f.request();
    request.basename = "errors.txt".into();
    request.contract.input_kind = macroscope_domain::InputKind::Text;
    // Even a stale schema cannot turn a text input into CSV at this boundary.
    request.contract.input_schema = vec!["INFO synthetic start".into(), "missing".into()];
    request.contract.output_columns.clear();
    let expected = b"ERROR failed,with,commas\n";
    let mut staged = stage_generated(request, "execution-pin".into(), |snapshot| {
        assert_eq!(snapshot, input);
        Ok(expected.to_vec())
    })
    .unwrap();
    assert_eq!(staged.preview().unwrap(), expected);
    assert_eq!(staged.plan().engine, "js-quickjs");
    let approval = approved(&mut staged);
    let receipt = commit(staged, approval, epochs()).unwrap();
    assert_eq!(receipt.input_digest, digest(input));
    assert_eq!(receipt.committed_digest, digest(expected));
    assert_eq!(fs::read(f.root.join("errors.txt")).unwrap(), expected);

    let mut request = f.request();
    request.basename = "other.txt".into();
    request.contract.input_kind = macroscope_domain::InputKind::Csv;
    assert!(matches!(
        stage_generated(request, "execution-pin".into(), |_| {
            panic!("missing CSV headers must fail before execution")
        }),
        Err(BrokerError::Mapping(MappingError::MissingColumns(_)))
    ));
    assert!(!f.root.join("other.txt").exists());
}

#[test]
fn staging_ciphertext_nonce_tag_and_truncation_tamper_are_rejected() {
    for tamper in [0, 24, 40, 100, usize::MAX] {
        let f = Fixture::new();
        let mut staged = f.stage();
        let approval = approved(&mut staged);
        let path = staged.run_dir.join(CIPHERTEXT_FILE);
        let mut bytes = fs::read(&path).unwrap();
        if tamper == usize::MAX {
            bytes.pop();
        } else {
            bytes[tamper] ^= 1;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        }
        #[cfg(windows)]
        {
            let mut p = fs::metadata(&path).unwrap().permissions();
            p.set_readonly(false);
            fs::set_permissions(&path, p).unwrap();
        }
        fs::write(&path, &bytes).unwrap();
        assert!(matches!(
            commit(staged, approval, epochs()),
            Err(BrokerError::StagingTampered)
        ));
        assert!(!f.output().exists());
    }
}

#[test]
fn input_changed_in_place_and_same_bytes_replaced_identity_are_rejected() {
    for replace in [false, true] {
        let f = Fixture::new();
        let mut staged = f.stage();
        let approval = approved(&mut staged);
        if replace {
            let bytes = fs::read(&f.input).unwrap();
            // Keep the old inode alive so it cannot immediately be reused.
            fs::rename(&f.input, f.input.with_extension("old")).unwrap();
            fs::write(&f.input, bytes).unwrap();
        } else {
            fs::write(&f.input, b"changed").unwrap();
        }
        assert!(matches!(
            commit(staged, approval, epochs()),
            Err(BrokerError::InputChanged)
        ));
        assert!(!f.output().exists());
    }
}

#[cfg(unix)]
#[test]
fn symlink_input_and_destination_are_rejected() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    let mut staged = f.stage();
    let approval = approved(&mut staged);
    let original = f.input.with_extension("original");
    fs::rename(&f.input, &original).unwrap();
    symlink(&original, &f.input).unwrap();
    assert!(matches!(
        commit(staged, approval, epochs()),
        Err(BrokerError::InputChanged)
    ));
    assert!(SelectedFile::from_picker(&f.input).is_err());
    assert!(!f.output().exists());
    let f = Fixture::new();
    let mut staged = f.stage();
    let approval = approved(&mut staged);
    let moved = f.root.with_extension("old");
    fs::rename(&f.root, &moved).unwrap();
    symlink(&moved, &f.root).unwrap();
    assert!(matches!(
        commit(staged, approval, epochs()),
        Err(BrokerError::DestinationChanged)
    ));
    assert!(!moved.join("orders_ready.csv").exists());
}

#[test]
fn destination_parent_swap_and_collision_do_not_write() {
    let f = Fixture::new();
    let mut staged = f.stage();
    let approval = approved(&mut staged);
    let moved = f.root.with_extension("old");
    fs::rename(&f.root, &moved).unwrap();
    fs::create_dir(&f.root).unwrap();
    assert!(matches!(
        commit(staged, approval, epochs()),
        Err(BrokerError::DestinationChanged)
    ));
    assert!(!f.output().exists());
    assert!(!moved.join("orders_ready.csv").exists());
    let f = Fixture::new();
    let mut staged = f.stage();
    let approval = approved(&mut staged);
    fs::write(f.output(), b"someone else's file").unwrap();
    assert!(matches!(
        commit(staged, approval, epochs()),
        Err(BrokerError::DestinationExists)
    ));
    assert_eq!(fs::read(f.output()).unwrap(), b"someone else's file");
}

#[test]
fn atomic_primitive_refuses_collisions_even_after_absence_check() {
    let f = Fixture::new();
    let root = platform::open(&f.root, true).unwrap();
    assert!(!platform::destination_exists(&root, "orders_ready.csv").unwrap());
    let mut temp = platform::create_temp(&root, "test.tmp").unwrap();
    temp.write_all(b"approved").unwrap();
    temp.sync_all().unwrap();
    fs::write(f.output(), b"competitor").unwrap();
    assert!(matches!(
        platform::publish(&root, "test.tmp", "orders_ready.csv"),
        Err(BrokerError::DestinationExists)
    ));
    assert_eq!(fs::read(f.output()).unwrap(), b"competitor");
}

#[test]
fn revocation_checks_both_revisions() {
    for current in [
        Epochs {
            grant_revision: 4,
            ..epochs()
        },
        Epochs {
            policy_epoch: 8,
            ..epochs()
        },
    ] {
        let f = Fixture::new();
        let mut staged = f.stage();
        let approval = approved(&mut staged);
        assert!(matches!(
            commit(staged, approval, current),
            Err(BrokerError::Revoked)
        ));
        assert!(!f.output().exists());
    }
}

#[test]
fn approval_must_match_and_be_recorded() {
    let f = Fixture::new();
    let mut staged = f.stage();
    let wrong = Approval {
        plan_hash: "wrong".into(),
        approved_at_ms: 1,
    };
    let hash = staged.plan_hash.clone();
    assert!(matches!(
        approve(&mut staged, &hash, wrong),
        Err(BrokerError::ApprovalMismatch)
    ));
    let right = Approval {
        plan_hash: hash.clone(),
        approved_at_ms: 1,
    };
    assert!(matches!(
        approve(&mut staged, "different", right.clone()),
        Err(BrokerError::ApprovalMismatch)
    ));
    assert!(matches!(
        commit(staged, right, epochs()),
        Err(BrokerError::ApprovalMismatch)
    ));
    assert!(!f.output().exists());
}

fn intent(staged: &StagedPlan) {
    append_journal(
        &staged.run_dir,
        &JournalEvent::Intent {
            plan_hash: staged.plan_hash.clone(),
            destination: staged
                .destination
                .path
                .join(&staged.plan.destination.basename),
            parent: staged.plan.destination.parent.clone(),
            length: staged.plan.effects[0].staged_length,
            digest: staged.plan.effects[0].sha256.clone(),
        },
    )
    .unwrap();
}
fn crash_after_publish(staged: &StagedPlan) {
    intent(staged);
    let root = platform::open(&staged.destination.path, true).unwrap();
    let mut file = platform::create_temp(&root, "crash.tmp").unwrap();
    file.write_all(&staged.preview().unwrap()).unwrap();
    file.sync_all().unwrap();
    platform::publish(&root, "crash.tmp", &staged.plan.destination.basename).unwrap();
    platform::sync_directory(&root).unwrap();
}

#[test]
fn reconcile_crash_after_publish_is_idempotent_and_does_not_retry() {
    let f = Fixture::new();
    let mut staged = f.stage();
    approved(&mut staged);
    crash_after_publish(&staged);
    assert_eq!(
        reconcile(staged.run_dir()).unwrap(),
        ReconcileOutcome::ReconciledPublished
    );
    let journal = fs::read(staged.run_dir.join(JOURNAL_FILE)).unwrap();
    assert_eq!(
        reconcile(staged.run_dir()).unwrap(),
        ReconcileOutcome::ReconciledPublished
    );
    assert_eq!(
        fs::read(staged.run_dir.join(JOURNAL_FILE)).unwrap(),
        journal
    );
    assert_eq!(fs::read(f.output()).unwrap(), staged.preview().unwrap());
    fs::write(f.output(), b"different content").unwrap();
    assert_eq!(
        reconcile(staged.run_dir()).unwrap(),
        ReconcileOutcome::OutcomeUnknown
    );
    assert_eq!(fs::read(f.output()).unwrap(), b"different content");
    fs::remove_file(f.output()).unwrap();
    assert_eq!(
        reconcile(staged.run_dir()).unwrap(),
        ReconcileOutcome::OutcomeUnknown
    );
    assert!(!f.output().exists());
}

#[test]
fn reconcile_intent_with_different_content_and_absent_destination() {
    let f = Fixture::new();
    let mut staged = f.stage();
    approved(&mut staged);
    intent(&staged);
    assert_eq!(
        reconcile(staged.run_dir()).unwrap(),
        ReconcileOutcome::NotPublished
    );
    assert!(!f.output().exists());
    fs::write(f.output(), b"different").unwrap();
    assert_eq!(
        reconcile(staged.run_dir()).unwrap(),
        ReconcileOutcome::OutcomeUnknown
    );
    assert_eq!(fs::read(f.output()).unwrap(), b"different");
}

#[test]
fn reconcile_root_swap_is_unknown() {
    let f = Fixture::new();
    let mut staged = f.stage();
    approved(&mut staged);
    crash_after_publish(&staged);
    fs::rename(&f.root, f.root.with_extension("old")).unwrap();
    fs::create_dir(&f.root).unwrap();
    assert_eq!(
        reconcile(staged.run_dir()).unwrap(),
        ReconcileOutcome::OutcomeUnknown
    );
    assert!(!f.output().exists());
}

#[test]
fn canonical_keys_are_order_independent_and_all_plan_preconditions_are_bound() {
    use serde_json::{json, Value};
    let a: Value = serde_json::from_str(r#"{"z":{"b":2,"a":"é"},"a":[3,2,1]}"#).unwrap();
    let b: Value = serde_json::from_str(r#"{"a":[3,2,1],"z":{"a":"é","b":2}}"#).unwrap();
    assert_eq!(canonical_json(&a).unwrap(), canonical_json(&b).unwrap());
    assert_eq!(
        String::from_utf8(canonical_json(&a).unwrap()).unwrap(),
        r#"{"a":[3,2,1],"z":{"a":"é","b":2}}"#
    );
    let f = Fixture::new();
    let staged = f.stage();
    let original = serde_json::to_value(staged.plan()).unwrap();
    assert_eq!(
        digest(&canonical_json(staged.plan()).unwrap()),
        staged.plan_hash()
    );
    let changes = [
        ("/tool_id", json!("other")),
        ("/version_hash", json!("other")),
        ("/engine", json!("other")),
        ("/mapping_hash", json!("other")),
        ("/input/digest", json!("other")),
        ("/input/size", json!(1)),
        ("/input/dev", json!(1)),
        ("/input/inode", json!(1)),
        ("/input/mtime_seconds", json!(1)),
        ("/input/mtime_nanoseconds", json!(999)),
        ("/destination/parent/dev", json!(1)),
        ("/destination/parent/inode", json!(1)),
        ("/destination/basename", json!("other.csv")),
        ("/effects/0/staged_length", json!(1)),
        ("/effects/0/sha256", json!("other")),
        ("/limits/max_rows", json!(1)),
        ("/limits/max_input_bytes", json!(1)),
        ("/limits/max_output_bytes", json!(1)),
        ("/grant_revision", json!(1)),
        ("/policy_epoch", json!(1)),
    ];
    for (pointer, replacement) in changes {
        let mut changed = original.clone();
        *changed.pointer_mut(pointer).unwrap() = replacement;
        assert_ne!(
            digest(&canonical_json(&changed).unwrap()),
            staged.plan_hash(),
            "{pointer}"
        );
    }
}

#[test]
fn selections_basename_and_contract_required_columns_fail_closed() {
    let f = Fixture::new();
    assert!(SelectedFile::from_picker(&f.root).is_err());
    assert!(SelectedRoot::from_picker(&f.input).is_err());
    assert!(SelectedFile::from_picker("relative.csv").is_err());
    for bad in [
        "../escape.csv",
        "x/y.csv",
        "x\\y.csv",
        ".",
        "..",
        "file.txt",
        "a\0.csv",
        "x\n.csv",
    ] {
        let mut request = f.request();
        request.basename = bad.into();
        assert!(matches!(stage(request), Err(BrokerError::InvalidBasename)));
    }
    let request = f.request();
    fs::write(&f.input, b"order_id,created,amount\nx,01/01/26,1\n").unwrap();
    assert!(
        matches!(stage(request),Err(BrokerError::Mapping(MappingError::MissingColumns(c))) if c==["customer"])
    );
    assert!(!f.output().exists());
}

#[cfg(unix)]
#[test]
fn devices_and_fifo_are_rejected_without_blocking() {
    assert!(SelectedFile::from_picker("/dev/null").is_err());
    let f = Fixture::new();
    let fifo = f.root.join("fifo.csv");
    use std::os::unix::ffi::OsStrExt;
    let fifo_c = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
    // SAFETY: the NUL-terminated path lives through this fixture-only call;
    // mkfifo creates a FIFO without opening or reading it.
    assert_eq!(unsafe { libc::mkfifo(fifo_c.as_ptr(), 0o600) }, 0);
    assert!(SelectedFile::from_picker(fifo).is_err());
}

fn action(body: &str) -> PlannedAction {
    PlannedAction {
        call: macroscope_domain::PlannedCall {
            connector_id: "gmail".into(),
            tool_name: "create_draft".into(),
            args: serde_json::json!({"body":body}),
        },
        service: "Gmail".into(),
        resource: "https://example.invalid/mcp".into(),
        read_only_hint: Some(false),
        destructive_hint: None,
    }
}
#[test]
fn action_arguments_and_order_are_bound_to_hash_and_default_commit_cannot_send() {
    let f = Fixture::new();
    let make = |calls| {
        stage_generated_actions(f.request(), "source-hash".into(), |_| {
            Ok((
                include_bytes!("../../../fixtures/orders/expected.csv").to_vec(),
                calls,
            ))
        })
        .unwrap()
    };
    let mut one = make(vec![action("private body")]);
    let mut changed = one.plan().clone();
    changed.planned_calls[0].call.args["body"] = serde_json::json!("changed");
    assert_ne!(digest(&canonical_json(&changed).unwrap()), one.plan_hash());
    changed = one.plan().clone();
    changed.planned_calls.push(action("second"));
    changed.planned_calls.reverse();
    assert_ne!(digest(&canonical_json(&changed).unwrap()), one.plan_hash());
    let two = make(vec![action("different body")]);
    assert_ne!(one.plan_hash(), two.plan_hash());
    let wrong = Approval {
        plan_hash: two.plan_hash().into(),
        approved_at_ms: 1,
    };
    assert!(approve(&mut one, two.plan_hash(), wrong).is_err());
    let approval = approved(&mut one);
    assert!(commit(one, approval, epochs()).is_err());
    assert!(!f.root.join("orders_ready.csv").exists());
}
#[test]
fn crash_after_send_is_unknown_never_replayed_and_private_args_are_encrypted() {
    let f = Fixture::new();
    let mut staged = stage_generated_actions(f.request(), "source-hash".into(), |_| {
        Ok((
            include_bytes!("../../../fixtures/orders/expected.csv").to_vec(),
            vec![action("private-secret-body"), action("later")],
        ))
    })
    .unwrap();
    let dir = staged.run_dir().to_path_buf();
    let hash = staged.plan_hash().to_string();
    let approval = approved(&mut staged);
    let sent = std::cell::Cell::new(0);
    let crash = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = commit_with_actions(staged, approval, epochs(), |_| {
            sent.set(sent.get() + 1);
            panic!("simulated crash after send, before response/receipt")
        });
    }));
    assert!(crash.is_err());
    assert_eq!(sent.get(), 1);
    for _ in 0..2 {
        assert_eq!(reconcile(&dir).unwrap(), ReconcileOutcome::OutcomeUnknown);
        let results = recover_action_results(&dir, &[42; 32], &hash).unwrap();
        assert_eq!(results[0].status, ActionStatus::OutcomeUnknown);
        assert_eq!(results[1].status, ActionStatus::NotRun);
    }
    assert_eq!(sent.get(), 1);
    assert!(!f.root.join("orders_ready.csv").exists());
    for path in [dir.join("journal.jsonl"), dir.join("actions.xchacha20")] {
        assert!(!String::from_utf8_lossy(&fs::read(path).unwrap()).contains("private-secret-body"));
    }
}

#[test]
fn crash_after_action_results_before_file_publication_keeps_the_action_receipt() {
    let f = Fixture::new();
    let mut staged = stage_generated_actions(f.request(), "source-hash".into(), |_| {
        Ok((
            include_bytes!("../../../fixtures/orders/expected.csv").to_vec(),
            vec![action("sent")],
        ))
    })
    .unwrap();
    let dir = staged.run_dir().to_path_buf();
    let hash = staged.plan_hash().to_string();
    approved(&mut staged);
    append_journal(
        &dir,
        &JournalEvent::ActionIntent {
            index: 0,
            plan_hash: hash.clone(),
        },
    )
    .unwrap();
    append_journal(
        &dir,
        &JournalEvent::ActionResult {
            index: 0,
            status: ActionStatus::Succeeded,
        },
    )
    .unwrap();
    intent(&staged);
    // The external effect finished; no output was published before the crash.
    for _ in 0..2 {
        assert_eq!(reconcile(&dir).unwrap(), ReconcileOutcome::OutcomeUnknown);
        let results = recover_action_results(&dir, &[42; 32], &hash).unwrap();
        assert_eq!(results[0].status, ActionStatus::Succeeded);
        assert_eq!(results[0].action.call.args["body"], "sent");
    }
    assert!(!f.output().exists());
}
