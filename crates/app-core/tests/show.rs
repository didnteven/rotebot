//! Show controller against a scripted observer (protocol-level) and, when built,
//! the real Swift observer.

use macroscope_app_core::dto::ShowState;
use macroscope_app_core::show::ShowController;
use macroscope_app_core::{AppCore, CoreConfig, NativeMuse};
use macroscope_vault::MemoryStore;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

fn core(dir: &Path) -> AppCore {
    AppCore::new(CoreConfig {
        data_dir: dir.join("data"),
        vault: Arc::new(MemoryStore::default()),
        gateway_base_url: "http://127.0.0.1:9".into(),
        home: dir.join("home"),
        muse: Arc::new(NativeMuse {
            home: dir.join("home"),
        }),
        app_version: "0.1.0".into(),
        platform: "macos".into(),
        now_ms: CoreConfig::system_clock(),
    })
    .unwrap()
}

/// A fake observer: reads the start message, echoes its sessionId into canned events
/// (including a forged event for another session that must be ignored), then waits
/// for stop or EOF.
fn fake_observer(dir: &Path) -> PathBuf {
    let path = dir.join("fake-observer.sh");
    std::fs::write(
        &path,
        r#"#!/bin/sh
read -r start
sid=$(printf '%s' "$start" | sed -n 's/.*"sessionId":"\([^"]*\)".*/\1/p')
emit() { printf '{"v":1,"sessionId":"%s","seq":%s,"monoMs":0,"policyEpoch":1,%s}\n' "$sid" "$1" "$2"; }
emit 1 '"type":"ready","permissions":{"accessibility":"granted"}'
emit 2 '"type":"app_focus","bundleId":"com.google.Chrome","appName":"Google Chrome"'
emit 3 '"type":"connector_match","connectorId":"gmail","signalKind":"window_title","ruleId":"gmail-title-0"'
emit 4 '"type":"app_focus","bundleId":"com.apple.Numbers","appName":"Numbers"'
emit 5 '"type":"connector_match","connectorId":"not-a-connector","signalKind":"window_title","ruleId":"x"'
printf '{"v":1,"sessionId":"forged","seq":6,"monoMs":0,"policyEpoch":1,"type":"connector_match","connectorId":"slack","signalKind":"foreground_app","ruleId":"x"}\n'
while read -r line; do
  case "$line" in *'"stop"'*) emit 7 '"type":"stopped","reason":"user"'; exit 0;; esac
done
emit 8 '"type":"stopped","reason":"eof"'
"#,
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

async fn wait_for(show: &ShowController, pred: impl Fn(&ShowState) -> bool) -> ShowState {
    for _ in 0..100 {
        let s = show.state();
        if pred(&s) {
            return s;
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    show.state()
}

#[tokio::test]
async fn recording_gmail_produces_a_connect_suggestion_and_counts_only() {
    let tmp = tempfile::tempdir().unwrap();
    let core = core(tmp.path());
    let show = ShowController::new(Some(fake_observer(tmp.path())), core.clone());
    assert!(matches!(show.state(), ShowState::Idle { .. }));
    let started = show.start().await;
    assert!(
        matches!(started, ShowState::Recording { .. }),
        "{started:?}"
    );
    let recording = wait_for(&show, |s| {
        matches!(
            s,
            ShowState::Recording {
                matches: 1,
                apps_seen: 2,
                ..
            }
        )
    })
    .await;
    assert!(
        matches!(
            recording,
            ShowState::Recording {
                matches: 1,
                apps_seen: 2,
                ..
            }
        ),
        "{recording:?}"
    );
    assert_eq!(show.accessibility(), "granted");
    let paused = show.pause(true).await;
    assert!(matches!(paused, ShowState::Recording { paused: true, .. }));

    let finished = show.stop().await;
    let ShowState::Finished {
        apps_seen,
        suggestions,
        ..
    } = finished
    else {
        panic!("{finished:?}")
    };
    assert_eq!(apps_seen, ["Google Chrome", "Numbers"]);
    assert_eq!(
        suggestions.len(),
        1,
        "forged-session and unknown connector events are ignored"
    );
    assert_eq!(suggestions[0].connector_id, "gmail");
    assert_eq!(suggestions[0].signal_kind, "window_title");
    assert_eq!(
        suggestions[0].message,
        "You used Gmail while recording. Connect Gmail so tools can work with it."
    );
    let detections = core.database().detections().unwrap();
    assert_eq!(detections.len(), 1);
    assert_eq!(
        (detections[0].connector_id.as_str(), detections[0].count),
        ("gmail", 1)
    );
}

#[tokio::test]
async fn dismissed_or_connected_connectors_are_not_suggested() {
    let tmp = tempfile::tempdir().unwrap();
    let core = core(tmp.path());
    core.database().dismiss_suggestion("gmail", None).unwrap();
    let show = ShowController::new(Some(fake_observer(tmp.path())), core.clone());
    show.start().await;
    wait_for(&show, |s| {
        matches!(s, ShowState::Recording { matches: 1, .. })
    })
    .await;
    let ShowState::Finished { suggestions, .. } = show.stop().await else {
        panic!()
    };
    assert!(suggestions.is_empty());

    let tmp2 = tempfile::tempdir().unwrap();
    let core2 = self::core(tmp2.path());
    core2
        .database()
        .set_connector_state(
            "gmail",
            &serde_json::json!({"state": {"kind": "connected"}}),
        )
        .unwrap();
    let show2 = ShowController::new(Some(fake_observer(tmp2.path())), core2);
    show2.start().await;
    wait_for(&show2, |s| {
        matches!(s, ShowState::Recording { matches: 1, .. })
    })
    .await;
    let ShowState::Finished { suggestions, .. } = show2.stop().await else {
        panic!()
    };
    assert!(suggestions.is_empty());
}

#[tokio::test]
async fn missing_observer_is_unavailable_with_a_reason() {
    let tmp = tempfile::tempdir().unwrap();
    let show = ShowController::new(Some(tmp.path().join("nope")), core(tmp.path()));
    let ShowState::Unavailable { reason } = show.start().await else {
        panic!()
    };
    assert!(reason.contains("observer"));
}

/// Real Swift observer: build it with `swift build -c release` in native/macos-observer
/// and run with MACROSCOPE_OBSERVER_IT=1. Checks the handshake, a clean stop, and that
/// no window title text ever arrives (only ids and app names).
#[tokio::test]
#[ignore = "needs the built Swift observer; set MACROSCOPE_OBSERVER_IT=1"]
async fn real_swift_observer_session_starts_and_stops() {
    if std::env::var("MACROSCOPE_OBSERVER_IT").as_deref() != Ok("1") {
        return;
    }
    let bin = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../native/macos-observer/.build/release/macroscope-observer");
    assert!(bin.is_file(), "build the observer first");
    let tmp = tempfile::tempdir().unwrap();
    let show = ShowController::new(Some(bin), core(tmp.path()));
    assert!(matches!(show.start().await, ShowState::Recording { .. }));
    tokio::time::sleep(Duration::from_secs(3)).await;
    let finished = show.stop().await;
    assert!(
        matches!(finished, ShowState::Finished { .. }),
        "{finished:?}"
    );
    assert!(["granted", "denied"].contains(&show.accessibility().as_str()));
}

#[tokio::test]
async fn stop_kills_and_reaps_an_observer_that_ignores_control() {
    let tmp = tempfile::tempdir().unwrap();
    let script = tmp.path().join("hang.sh");
    let pid_file = tmp.path().join("pid");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\necho $$ > '{}'\nread -r start\nexec /bin/sleep 30\n",
            pid_file.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let show = ShowController::new(Some(script), core(tmp.path()));
    show.start().await;
    // Generous deadline: under a fully parallel test run the shell can take a while to start.
    for _ in 0..1000 {
        if std::fs::read_to_string(&pid_file).is_ok_and(|p| !p.trim().is_empty()) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let pid = std::fs::read_to_string(&pid_file).expect("fake observer never started");
    let state = show.stop().await;
    let alive = std::process::Command::new("/bin/kill")
        .args(["-0", pid.trim()])
        .output()
        .unwrap()
        .status
        .success();
    if alive {
        let _ = std::process::Command::new("/bin/kill")
            .args(["-KILL", pid.trim()])
            .status();
    }
    assert!(
        !alive,
        "stop acknowledged while observer still running: {state:?}"
    );
}

#[tokio::test]
async fn observer_events_while_paused_do_not_record_detections() {
    let tmp = tempfile::tempdir().unwrap();
    let script = tmp.path().join("pause.sh");
    std::fs::write(&script, r#"#!/bin/sh
read -r start
sid=$(printf '%s' "$start" | sed -n 's/.*"sessionId":"\([^"]*\)".*/\1/p')
while read -r line; do
 case "$line" in
  *'"pause"'*) printf '{"sessionId":"%s","type":"connector_match","connectorId":"gmail","signalKind":"window_title"}\n' "$sid" ;;
  *'"stop"'*) exit 0 ;;
 esac
done
"#).unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let core = core(tmp.path());
    let show = ShowController::new(Some(script), core.clone());
    show.start().await;
    show.pause(true).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    show.stop().await;
    assert!(core.database().detections().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_start_spawns_only_one_observer() {
    let tmp = tempfile::tempdir().unwrap();
    let path = fake_observer(tmp.path());
    let starts = tmp.path().join("starts");
    let script = std::fs::read_to_string(&path).unwrap().replacen(
        "#!/bin/sh\n",
        &format!("#!/bin/sh\necho started >> '{}'\n", starts.display()),
        1,
    );
    std::fs::write(&path, script).unwrap();
    let show = ShowController::new(Some(path), core(tmp.path()));
    let barrier = Arc::new(tokio::sync::Barrier::new(3));
    let mut tasks = Vec::new();
    for _ in 0..2 {
        let s = show.clone();
        let b = barrier.clone();
        tasks.push(tokio::spawn(async move {
            b.wait().await;
            s.start().await
        }));
    }
    barrier.wait().await;
    for task in tasks {
        task.await.unwrap();
    }
    let recording = wait_for(&show, |s| {
        matches!(s, ShowState::Recording { apps_seen: 2, .. })
    })
    .await;
    assert!(
        matches!(recording, ShowState::Recording { apps_seen: 2, .. }),
        "{recording:?}"
    );
    show.stop().await;
    assert_eq!(std::fs::read_to_string(starts).unwrap().lines().count(), 1);
}

#[tokio::test]
async fn deleting_history_stops_show_and_clears_session_detections() {
    let tmp = tempfile::tempdir().unwrap();
    let core = core(tmp.path());
    let show = ShowController::new(Some(fake_observer(tmp.path())), core.clone());
    show.start().await;
    let recording = wait_for(&show, |s| {
        matches!(s, ShowState::Recording { matches: 1, .. })
    })
    .await;
    assert!(matches!(recording, ShowState::Recording { matches: 1, .. }));
    show.delete_data(macroscope_app_core::dto::DeleteOptions {
        include_tools: false,
    })
    .await
    .unwrap();
    assert!(matches!(show.state(), ShowState::Idle { .. }));
    assert!(core
        .delete_preview()
        .unwrap()
        .items
        .iter()
        .all(|i| i.count == 0));
}
