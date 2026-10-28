use super::*;

use macroscope_domain::TaskContract;
use macroscope_model_gateway::MetaResponsesClient;
use macroscope_muse_client::{BinaryInfo, CredentialRoute, MUSE_WAITING_MESSAGE};
use macroscope_vault::MemoryStore;
use std::{os::unix::fs::PermissionsExt, path::Path, time::Duration};

fn fake_muse(root: &Path) -> BinaryInfo {
    let script = root.join("fake-muse");
    std::fs::write(
        &script,
        r#"#!/usr/bin/python3
import json, os, pathlib, sys, time
root = pathlib.Path(__file__).parent
if sys.argv[1] == 'serve':
    assert sys.argv[2:4] == ['--provider', 'echo']
    for line in sys.stdin:
        v = json.loads(line)
        assert v['method'] in ['initialize', 'initialized']
        if v['method'] == 'initialize':
            print(json.dumps(dict(jsonrpc='2.0', id=v['id'], result=dict(schema=dict(version=1, fingerprint='FINGERPRINT')))), flush=True)
elif '--help' in sys.argv:
    print('--json --model --contributor-mode --approval-mode --output-schema --workspace --prompt-file')
else:
    assert sys.argv[1] == 'exec'
    prompt = sys.argv[sys.argv.index('--prompt-file') + 1]
    (root/'prompt').write_text(prompt)
    (root/'prompt-text').write_text(open(prompt).read())
    (root/'running').write_text(str(os.getpid()))
    while not (root/'release').exists(): time.sleep(0.01)
    print(json.dumps(dict(payload_type='run.terminal.completed', payload=dict(terminal='completed', text=(root/'answer').read_text()))), flush=True)
"#
        .replace("FINGERPRINT", macroscope_muse_client::CURRENT_FINGERPRINT),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
    BinaryInfo::pin(script, "1.4.13-test".into()).unwrap()
}

async fn wait_running(root: &Path) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while !root.join("running").exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("fake Muse must start");
}

async fn exercise_long_call(stop_mode: u8) {
    let root = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let binary = fake_muse(root.path());
    let contract: TaskContract =
        serde_json::from_str(include_str!("../../../fixtures/orders/task.json")).unwrap();
    std::fs::write(
        root.path().join("answer"),
        serde_json::to_string(&contract.mapping).unwrap(),
    )
    .unwrap();
    let core = AppCore::new(CoreConfig {
        data_dir: root.path().join("data"),
        home: root.path().join("fake-home"),
        vault: Arc::new(MemoryStore::default()),
        gateway_base_url: "http://127.0.0.1:9".into(),
        muse: Arc::new(NativeMuse {
            home: root.path().join("fake-home"),
        }),
        app_version: "0.1.0".into(),
        platform: "macos".into(),
        now_ms: CoreConfig::system_clock(),
    })
    .unwrap();
    let input = core.load_sample_csv().unwrap();
    let draft = core.draft_task(&input.handle.handle_id, "").unwrap();
    let epoch = core.inner.policy.allow_cloud().unwrap();
    let progress = |message: &str| core.push_build_progress("muse-test", message).unwrap();
    let messages = || {
        core.build_state("muse-test")
            .unwrap()
            .events
            .into_iter()
            .filter(|e| e.message == MUSE_WAITING_MESSAGE)
            .count()
    };
    let builder = MappingBuilder {
        route: BuildRoute::MuseExec {
            route: CredentialRoute::PlanSignIn,
        },
        vault: &*core.inner.cfg.vault,
        gateway: MetaResponsesClient::new("http://127.0.0.1:9"),
        policy: &core.inner.policy,
        consent_epoch: epoch,
        muse: Some(MuseConfig {
            binary,
            private_home: root.path().join("private-home"),
            real_home: root.path().join("fake-home"),
            synthetic_root: root.path().join("workspaces"),
            key_ref: None,
            containment_verified: true,
        }),
        muse_progress: Some(&progress),
    };
    // A second call proves notification state belongs to each exec, rather
    // than to the builder (including schema/program correction calls).
    for _ in 0..if stop_mode > 0 { 1 } else { 2 } {
        let (stop, mut stopped) = tokio::sync::watch::channel(false);
        // Seed the host slot so real Stop/Emergency stop controls operate on
        // the fake binary without resolving any installed Muse or account.
        lock(&core.inner.builds).insert(
            "muse-test".into(),
            BuildSlot {
                state: BuildState {
                    build_id: "muse-test".into(),
                    draft_id: draft.draft_id.clone(),
                    route: ModelRoute::MusePlan,
                    phase: BuildPhase::Writing,
                    events: vec![],
                    candidate: None,
                    program: None,
                    checks: vec![],
                    comparison: None,
                    spent_micros: None,
                    failure: None,
                    route_label: "Muse plan".into(),
                },
                tx: broadcast::channel(64).0,
                task: None,
                cancel: stop,
                draft_revision: draft.revision,
                source_approval: tokio::sync::watch::channel(false).0,
            },
        );
        let mut events = core.subscribe_build("muse-test").unwrap();
        let call = builder.propose_cancellable(
            &contract,
            SyntheticSamples::from_contract(&contract, 1),
            BuildBudget::default(),
            &HostValidator,
            async {
                let _ = stopped.changed().await;
            },
        );
        let control = async {
            wait_running(root.path()).await;
            let before = messages();
            let prompt = std::fs::read_to_string(root.path().join("prompt")).unwrap();
            assert!(Path::new(&prompt).exists());
            // Muse gets the same system instructions the direct route sends.
            let text = std::fs::read_to_string(root.path().join("prompt-text")).unwrap();
            assert!(text.starts_with("System instructions:\n"));
            assert!(text.contains(macroscope_builder::SYSTEM_INSTRUCTIONS));
            tokio::time::pause();
            tokio::time::advance(Duration::from_secs(59)).await;
            assert_eq!(messages(), before);
            tokio::time::advance(Duration::from_secs(1)).await;
            // Give the timer wheel a turn to wake the exec read future.
            tokio::time::sleep(Duration::from_millis(1)).await;
            assert_eq!(messages(), before + 1);
            tokio::time::advance(Duration::from_secs(77)).await;
            assert_eq!(messages(), before + 1);
            tokio::time::resume();
            match stop_mode {
                1 => {
                    core.stop_build("muse-test").unwrap();
                }
                2 => {
                    core.emergency_stop().await.unwrap();
                }
                _ => {
                    std::fs::write(root.path().join("release"), "").unwrap();
                }
            }
        };
        tokio::pin!(call, control);
        let result = tokio::select! {
            result = &mut call => {
                tokio::time::timeout(Duration::from_secs(5), &mut control).await
                    .expect("stop must finish after cancellation");
                result
            },
            _ = &mut control => tokio::time::timeout(Duration::from_secs(5), call)
                .await
                .expect("cancellation must not wait for the 300 s deadline"),
        };
        if stop_mode > 0 {
            assert!(matches!(result, Err(BuildError::EgressDenied)));
        } else {
            assert_eq!(result.unwrap().mapping, contract.mapping);
        }
        let notified = events.try_recv().expect("waiting event must be published");
        assert_eq!(
            notified.events.last().unwrap().message,
            MUSE_WAITING_MESSAGE
        );
        assert_eq!(notified.phase, BuildPhase::Writing);
        let stored = core.database().build("muse-test").unwrap().unwrap();
        let stored: BuildState = serde_json::from_value(stored.state).unwrap();
        assert_eq!(
            stored
                .events
                .iter()
                .filter(|e| e.message == MUSE_WAITING_MESSAGE)
                .count(),
            1
        );
        assert_eq!(messages(), 1);
        if stop_mode > 0 {
            assert_eq!(
                core.build_state("muse-test").unwrap().phase,
                BuildPhase::Stopped
            );
            core.push_build_progress("muse-test", MUSE_WAITING_MESSAGE)
                .unwrap();
            assert_eq!(
                messages(),
                1,
                "a stopped build must not get late waiting events"
            );
        }
        let pid = std::fs::read_to_string(root.path().join("running")).unwrap();
        assert!(
            !std::process::Command::new("/bin/kill")
                .args(["-0", &pid])
                .output()
                .unwrap()
                .status
                .success(),
            "Muse must be killed and reaped before returning"
        );
        let prompt = std::fs::read_to_string(root.path().join("prompt")).unwrap();
        assert!(!Path::new(&prompt).exists());
        assert_eq!(
            std::fs::read_dir(root.path().join("workspaces"))
                .unwrap()
                .count(),
            0
        );
        std::fs::remove_file(root.path().join("running")).unwrap();
        if stop_mode == 0 {
            std::fs::remove_file(root.path().join("release")).unwrap();
        }
    }
}

#[tokio::test]
async fn muse_build_reports_waiting_once_per_long_call_and_succeeds_past_120_seconds() {
    exercise_long_call(0).await;
}

#[tokio::test]
async fn stopping_a_long_muse_build_after_waiting_kills_and_reaps_without_waiting_for_deadline() {
    exercise_long_call(1).await;
}

#[tokio::test]
async fn emergency_stop_cancels_a_long_muse_build_and_reaps_before_acknowledging() {
    exercise_long_call(2).await;
}

#[test]
fn muse_deadline_error_keeps_its_build_classification_and_message() {
    let (kind, message) = build_error_message(&BuildError::Muse(
        macroscope_muse_client::MuseError::Timeout,
    ));
    assert_eq!(kind, BuildFailureKind::Internal);
    assert_eq!(message, "The build failed: Muse deadline exceeded");
}
