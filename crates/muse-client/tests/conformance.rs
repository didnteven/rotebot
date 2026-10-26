#![cfg(unix)]
use macroscope_muse_client::*;
use macroscope_vault::Secret;
use serde_json::json;
use std::{os::unix::fs::PermissionsExt, path::Path};

const UNKNOWN: &str = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
fn fake(root: &Path) -> BinaryInfo {
    let script = root.join("native");
    let source = r#"#!/usr/bin/python3
import json, os, pathlib, sys
root = pathlib.Path(__file__).parent
assert sys.argv[1:4] == ['serve', '--provider', 'echo']
assert os.environ['HOME'].startswith(str(root))
assert all(os.environ[n].startswith(os.environ['HOME']) for n in ['XDG_CONFIG_HOME','XDG_DATA_HOME','XDG_STATE_HOME','XDG_CACHE_HOME'])
assert 'MACROSCOPE_PARENT_CANARY' not in os.environ
with (root/'launches').open('a') as f: f.write('echo\n')
mode = (root/'mode').read_text() if (root/'mode').exists() else ''
turn = 0
def send(v): print(json.dumps(dict(jsonrpc='2.0', **v)), flush=True)
def event(m, **p): send(dict(method=m, params=dict(sessionId='s', **p)))
for line in sys.stdin:
    v = json.loads(line); m = v['method']
    assert m in ['initialize','initialized','model/list','session/start','turn/start','turn/interrupt','macroscope/conformanceUnknown']
    if m == 'initialized': continue
    if m == 'initialize': r = dict(schema=dict(version=2 if mode=='major' else 1, fingerprint='FINGERPRINT'), sandboxAvailability='available')
    elif m == 'model/list': r = dict(models=[])
    elif m == 'session/start':
        assert (pathlib.Path(v['params']['workspaceRoot'])/'.git').is_dir()
        r = dict(session=dict(sessionId='s'))
    elif m == 'turn/start':
        turn += 1
        r = dict(status='accepted',turnId=str(turn))
    elif m == 'turn/interrupt':
        if mode=='interrupt':
            send(dict(id=v['id'], error=dict(code=-32000, message='rejected'))); continue
        r = dict(status='accepted')
    else:
        if mode=='unknown': r = dict(unexpected=True)
        else:
            send(dict(id=v['id'],error=dict(code=-32601, message='unknown method'))); continue
    send(dict(id=v['id'],result=r))
    if m == 'turn/start':
        text = 'wrong' if mode=='text' else 'echo: ' + v['params']['input'][0]['text']
        event('item/completed',item=dict(itemId=str(turn),turnId=str(turn),kind='agentMessage',revision=1,status='completed',text=text,additiveField=True))
        event('turn/completed',turnId=str(turn),terminal='completed')
"#.replace("FINGERPRINT", UNKNOWN);
    std::fs::write(&script, source).unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
    BinaryInfo::pin(script, "1.4.99-test".into()).unwrap()
}
fn init() -> serde_json::Value {
    json!({"schema":{"version":1,"fingerprint":UNKNOWN}})
}
fn launches(root: &Path) -> usize {
    std::fs::read_to_string(root.join("launches"))
        .unwrap()
        .lines()
        .count()
}

#[tokio::test]
async fn unknown_schema_conformance_is_cached_by_hash_and_rechecked_after_replacement() {
    let root = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let binary = fake(root.path());
    let store = JsonAcceptanceStore::new(root.path().join("app-state/muse"));
    let ready = SetupState::Ready {
        fingerprint: UNKNOWN.into(),
        verified: Verification::Conformance,
    };
    assert_eq!(
        ensure_conformance(&binary, &init(), &store, root.path())
            .await
            .unwrap(),
        ready
    );
    assert_eq!(launches(root.path()), 1);
    assert_eq!(
        ensure_conformance(&binary, &init(), &store, root.path())
            .await
            .unwrap(),
        ready
    );
    assert_eq!(launches(root.path()), 1);
    let record = store.load(&binary.binary_sha256).unwrap().unwrap();
    assert_eq!(record.fingerprint, UNKNOWN);
    assert_eq!(record.version, binary.version);
    assert_eq!(record.result, ConformanceResult::Passed);
    let mut changed = std::fs::read_to_string(&binary.path).unwrap();
    changed.push_str("\n# a replacement executable\n");
    std::fs::write(&binary.path, changed).unwrap();
    assert_eq!(binary.validate(), Err(MuseError::BinaryChanged));
    assert_eq!(
        ensure_conformance(&binary, &init(), &store, root.path())
            .await
            .unwrap_err(),
        MuseError::BinaryChanged
    );
    let replacement = BinaryInfo::pin(binary.path.clone(), binary.version.clone()).unwrap();
    assert_ne!(replacement.binary_sha256, binary.binary_sha256);
    assert_eq!(
        ensure_conformance(&replacement, &init(), &store, root.path())
            .await
            .unwrap(),
        ready
    );
    assert_eq!(launches(root.path()), 2);
    std::fs::remove_file(&replacement.path).unwrap();
    assert_eq!(replacement.validate(), Err(MuseError::BinaryChanged));
}

#[tokio::test]
async fn failed_steps_remain_required_and_cannot_launch_meta() {
    for (mode, step) in [
        ("text", "turn/completed echo text"),
        ("interrupt", "turn/interrupt"),
        ("unknown", "unknown method JSON-RPC error"),
    ] {
        let root = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
        let binary = fake(root.path());
        std::fs::write(root.path().join("mode"), mode).unwrap();
        let store = JsonAcceptanceStore::new(root.path().join("app-state/muse"));
        assert_eq!(
            ensure_conformance(&binary, &init(), &store, root.path())
                .await
                .unwrap(),
            SetupState::ConformanceRequired {
                fingerprint: UNKNOWN.into(),
                failing_step: Some(step.into())
            }
        );
        assert_eq!(
            store.load(&binary.binary_sha256).unwrap().unwrap().result,
            ConformanceResult::Failed { step: step.into() }
        );
        let env = ChildEnv::for_route(
            CredentialRoute::ApiKey,
            &root.path().join("home"),
            root.path(),
            Some(Secret::new(b"fake-only".to_vec())),
        )
        .unwrap();
        let error = MspClient::spawn_with_store(&binary, Provider::Meta, env, root.path(), &store)
            .await
            .err()
            .expect("Meta must remain blocked");
        assert_eq!(
            error,
            MuseError::ConformanceRequired {
                fingerprint: UNKNOWN.into(),
                step: step.into()
            }
        );
        // Failures are not acceptances: fixing the server must trigger a new run.
        std::fs::write(root.path().join("mode"), "").unwrap();
        assert!(matches!(
            ensure_conformance(&binary, &init(), &store, root.path())
                .await
                .unwrap(),
            SetupState::Ready {
                verified: Verification::Conformance,
                ..
            }
        ));
    }
}

#[tokio::test]
async fn major_version_blocks_before_conformance_and_stale_records_are_not_acceptances() {
    let root = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let binary = fake(root.path());
    let store = JsonAcceptanceStore::new(root.path().join("app-state/muse"));
    assert_eq!(
        ensure_conformance(
            &binary,
            &json!({"schema":{"version":2,"fingerprint":UNKNOWN}}),
            &store,
            root.path()
        )
        .await
        .unwrap(),
        SetupState::SchemaMajorMismatch
    );
    assert!(!root.path().join("launches").exists());
    for fingerprint in [PINNED_FINGERPRINT, CURRENT_FINGERPRINT] {
        assert_eq!(
            ensure_conformance(
                &binary,
                &json!({"schema":{"version":1,"fingerprint":fingerprint}}),
                &store,
                root.path()
            )
            .await
            .unwrap(),
            SetupState::Ready {
                fingerprint: fingerprint.into(),
                verified: Verification::Known
            }
        );
    }
    let report = run_echo_conformance(&binary, UNKNOWN, root.path()).await;
    persist_conformance(&binary, &store, &report).unwrap();
    let mut record = store.load(&binary.binary_sha256).unwrap().unwrap();
    record.version = "wrong version".into();
    store.save(&record).unwrap();
    ensure_conformance(&binary, &init(), &store, root.path())
        .await
        .unwrap();
    assert_eq!(launches(root.path()), 2);
    record = store.load(&binary.binary_sha256).unwrap().unwrap();
    record.fingerprint = "other".into();
    store.save(&record).unwrap();
    ensure_conformance(&binary, &init(), &store, root.path())
        .await
        .unwrap();
    assert_eq!(launches(root.path()), 3);
    let files = std::fs::read_dir(root.path().join("app-state/muse"))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(
        files.len(),
        1,
        "atomic store leaves one complete JSON record"
    );
    std::fs::write(files[0].path(), "partial JSON").unwrap();
    assert_eq!(
        ensure_conformance(&binary, &init(), &store, root.path())
            .await
            .unwrap_err(),
        MuseError::AcceptanceStore
    );
}

#[tokio::test]
async fn exec_refuses_missing_or_replaced_binary_before_launch() {
    let root = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let binary = fake(root.path());
    let exec = ExecClient::pinned(&binary);
    std::fs::write(&binary.path, "replaced").unwrap();
    let env = ChildEnv::for_route(
        CredentialRoute::ApiKey,
        &root.path().join("home"),
        root.path(),
        Some(Secret::new(b"fake-only".to_vec())),
    )
    .unwrap();
    assert_eq!(
        exec.exec_echo_structured("hello", &root.path().join("schema"), root.path(), &env)
            .await
            .unwrap_err(),
        MuseError::BinaryChanged
    );
    std::fs::remove_file(&binary.path).unwrap();
    assert_eq!(
        exec.exec_echo_structured("hello", &root.path().join("schema"), root.path(), &env)
            .await
            .unwrap_err(),
        MuseError::BinaryChanged
    );
}
