//! Full program product path with a fake Responses endpoint and explicit synthetic
//! worker launches. Production never chooses synthetic execution automatically.
#![cfg(feature = "synthetic-runtime-tests")]
use macroscope_app_core::{dto::*, AppCore, CoreConfig, NativeMuse};
use macroscope_builder::Program;
use macroscope_runner::Limits;
use macroscope_vault::MemoryStore;
use serde_json::json;
use std::{path::Path, sync::Arc};
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};
const INPUT: &str = "2037-12-19 INFO CustomerPrivate731 started\n2037-12-19 ERROR AccountSecret928 failed\n2037-12-20 INFO CustomerPrivate732 done\n2037-12-20 ERROR AccountSecret929 failed\n";
const EXPECTED: &str =
    "2037-12-19 ERROR AccountSecret928 failed\n2037-12-20 ERROR AccountSecret929 failed\n";
const CODE: &str = r#"function run() { const input = macroscope.input("input_csv"); macroscope.output("errors.txt", input.split("\n").filter(line => line.includes("ERROR")).join("\n") + "\n"); }"#;
fn runner_path() -> std::path::PathBuf {
    std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("macroscope-runner")
}
fn core(root: &Path, endpoint: String) -> AppCore {
    let core = AppCore::new(CoreConfig {
        data_dir: root.join("data"),
        vault: Arc::new(MemoryStore::default()),
        gateway_base_url: endpoint,
        home: root.join("empty-home"),
        muse: Arc::new(NativeMuse {
            home: root.join("empty-home"),
        }),
        app_version: "test".into(),
        platform: "macos".into(),
        now_ms: CoreConfig::system_clock(),
    })
    .unwrap();
    core.use_synthetic_runtime_for_tests(
        &runner_path(),
        Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../runner/profile.sb")),
    );
    core
}
async fn wait(core: &AppCore, id: &str) -> BuildState {
    for _ in 0..1000 {
        let state = core.build_state(id).unwrap();
        if state.phase.is_terminal() {
            return state;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!("program build timed out");
}
#[tokio::test]
async fn model_program_ready_save_stage_approve_commit_exact_bytes_and_no_private_packet_tokens() {
    let temp = tempfile::tempdir().unwrap();
    let server = MockServer::start().await;
    let program = Program {
        summary: "Extract ERROR lines".into(),
        output_name: "errors.txt".into(),
        code: CODE.into(),
    };
    Mock::given(method("POST")).and(path("/responses")).respond_with(ResponseTemplate::new(200).set_body_json(json!({"status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":serde_json::to_string(&program).unwrap()}]}],"usage":{"input_tokens":3000,"output_tokens":400}}))).mount(&server).await;
    let core = core(temp.path(), server.uri());
    core.save_api_key("mk-program-test-canary-1234567890")
        .await
        .unwrap();
    let source = temp.path().join("selected.log");
    std::fs::write(&source, INPUT).unwrap();
    let file = core.register_picked_file(&source).unwrap();
    let preview = core.preview_handle(&file.handle_id).unwrap();
    assert!(preview.text.is_some());
    let draft = core
        .draft_task(
            &file.handle_id,
            "extract every line containing ERROR from a log into errors.txt",
        )
        .unwrap();
    assert_eq!(draft.kind, TaskKind::Program);
    let draft = core
        .update_task(&draft.draft_id, TaskKind::Program, "errors.txt", None)
        .unwrap();
    let outbound = core.outbound_preview(&draft.draft_id).await.unwrap();
    assert!(!outbound.contains_real_rows);
    assert!(core.start_build(&draft.draft_id, false).await.is_err());
    let started = core.start_build(&draft.draft_id, true).await.unwrap();
    let state = wait(&core, &started.build_id).await;
    assert_eq!(state.phase, BuildPhase::Ready, "{state:?}");
    assert!(state.checks.iter().all(|c| c.passed));
    assert_eq!(
        state.comparison.as_ref().unwrap().output_text.as_deref(),
        Some(EXPECTED.trim_end())
    );
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    for token in [
        "CustomerPrivate731",
        "CustomerPrivate732",
        "AccountSecret928",
        "AccountSecret929",
        "2037",
    ] {
        assert!(!outbound.payload_text.contains(token));
        assert!(
            !String::from_utf8_lossy(&requests[0].body).contains(token),
            "leaked {token}"
        );
    }
    let packet: serde_json::Value = serde_json::from_str(
        outbound
            .payload_text
            .split_once("Message (synthetic shape; example is real user data):\n")
            .unwrap()
            .1,
    )
    .unwrap();
    assert_eq!(
        packet,
        serde_json::from_str::<serde_json::Value>(body["input"][1]["content"].as_str().unwrap())
            .unwrap()
    );
    let tool = core
        .save_tool(&started.build_id, "Extract errors")
        .await
        .unwrap();
    assert_eq!(tool.summary_row.engine, "js-quickjs");
    assert_eq!(tool.program.as_ref(), Some(&program));
    let version = core
        .database()
        .version(&tool.summary_row.id, &tool.version_hash)
        .unwrap()
        .unwrap();
    assert_eq!(version.manifest["runtime"]["engine"], "js-quickjs");
    assert_eq!(
        version.manifest["capabilities"]["fileCreate"][0]["overwrite"],
        false
    );
    let folder = core.register_picked_dir(temp.path()).unwrap();
    let staged = core
        .stage_run(
            &tool.summary_row.id,
            &file.handle_id,
            &folder.handle_id,
            "errors.txt",
        )
        .unwrap();
    let StageResult::Staged(staged) = staged else {
        panic!("{staged:?}")
    };
    assert!(!temp.path().join("errors.txt").exists());
    assert_eq!(staged.preview_text.as_deref(), Some(EXPECTED.trim_end()));
    let receipt = core.commit_run(&staged.run_id, &staged.plan_hash).unwrap();
    assert!(matches!(receipt, CommitResult::Committed(_)), "{receipt:?}");
    assert_eq!(
        std::fs::read(temp.path().join("errors.txt")).unwrap(),
        EXPECTED.as_bytes()
    );
    assert_eq!(std::fs::read_to_string(source).unwrap(), INPUT);
    assert!(matches!(
        core.stage_run(
            &tool.summary_row.id,
            &file.handle_id,
            &folder.handle_id,
            "errors.txt"
        )
        .unwrap(),
        StageResult::Failed(_)
    ));
    assert_eq!(core.list_receipts().unwrap().len(), 1);
}

#[tokio::test]
async fn program_checks_reject_forbidden_code_limits_bad_names_and_wrong_examples() {
    let temp = tempfile::tempdir().unwrap();
    let core = core(temp.path(), "http://127.0.0.1:9".into());
    let source = temp.path().join("selected.log");
    std::fs::write(&source, INPUT).unwrap();
    let file = core.register_picked_file(&source).unwrap();
    let draft = core
        .draft_task(&file.handle_id, "extract ERROR lines")
        .unwrap();
    let mut draft = core
        .update_task(&draft.draft_id, TaskKind::Program, "errors.txt", None)
        .unwrap();
    assert!(core
        .start_build(&draft.draft_id, true)
        .await
        .unwrap_err()
        .to_string()
        .contains("Writing a program needs a model"));
    let engine = macroscope_app_core::runner::RunnerEngine::new_synthetic(
        &runner_path(),
        Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../runner/profile.sb")),
    )
    .unwrap();
    let execute = |input: &[u8], code: &str| {
        engine
            .execute(
                input,
                code,
                Limits {
                    cpu_ms: 50,
                    wall_ms: 200,
                    ..Limits::default()
                },
            )
            .map_err(|e| e.to_string())
    };
    let read = || Ok(INPUT.as_bytes().to_vec());
    for (code, message) in [
        (
            "function run(){import('data:text/javascript,1')}",
            "imports are forbidden",
        ),
        (
            "function run(){macroscope.output('errors.txt','x'.repeat(21000000))}",
            "output",
        ),
        ("function run(){while(true){}}", "budget"),
        ("function run(){eval('1')}", "JavaScript execution failed"),
        (
            "function run(){new Worker('data:text/javascript,1')}",
            "JavaScript execution failed",
        ),
        (
            "function run(){macroscope.output('errors.txt',String(Math.random()))}",
            "JavaScript execution failed",
        ),
        (
            "function run(){macroscope.output('errors.txt',String(Date.now()))}",
            "JavaScript execution failed",
        ),
        ("function run(){macroscope.output('wrong.txt','wrong')}", ""),
    ] {
        let candidate = Program {
            summary: "test".into(),
            output_name: "errors.txt".into(),
            code: code.into(),
        };
        let outcome = macroscope_app_core::checks::run_program(&draft, &candidate, &read, &execute);
        assert!(!outcome.all_passed(), "accepted {code}");
        assert!(
            outcome.checks.iter().any(|c| !c.passed
                && (message.is_empty()
                    || c.detail.as_deref().is_some_and(|d| d.contains(message)))),
            "{code}: {:?}",
            outcome.checks
        );
    }
    let candidate = Program {
        summary: "errors".into(),
        output_name: "errors.txt".into(),
        code: CODE.into(),
    };
    draft.example_output = Some(EXPECTED.into());
    assert!(
        macroscope_app_core::checks::run_program(&draft, &candidate, &read, &execute).all_passed()
    );
    draft.example_output = Some(EXPECTED.lines().rev().collect::<Vec<_>>().join("\n"));
    assert!(
        !macroscope_app_core::checks::run_program(&draft, &candidate, &read, &execute).all_passed()
    );
    let ticks = std::cell::Cell::new(0);
    let nondeterministic = |_: &[u8], _: &str| {
        ticks.set(ticks.get() + 1);
        Ok(vec![macroscope_runner::Output {
            name: "errors.txt".into(),
            text: ticks.get().to_string(),
        }])
    };
    let outcome =
        macroscope_app_core::checks::run_program(&draft, &candidate, &read, &nondeterministic);
    assert!(outcome
        .checks
        .iter()
        .any(|c| c.id == "deterministic" && !c.passed));
    draft.example_output = None;
    for (name, text, valid) in [
        ("errors.txt", "", false),
        ("errors.json", "{broken", false),
        ("errors.json", "{\"ok\":true}", true),
        ("errors.tsv", "a\tb\n1\n", false),
        ("errors.tsv", "a\tb\n1\t2\n", true),
        ("errors.csv", "a,b\n1\n", false),
    ] {
        draft.output_name = name.into();
        let candidate = Program {
            summary: "format test".into(),
            output_name: name.into(),
            code: format!(
                "function run(){{macroscope.output({},{});}}",
                serde_json::to_string(name).unwrap(),
                serde_json::to_string(text).unwrap()
            ),
        };
        let outcome = macroscope_app_core::checks::run_program(&draft, &candidate, &read, &execute);
        assert_eq!(outcome.all_passed(), valid, "{name}: {:?}", outcome.checks);
    }
    draft.output_name = "errors.txt".into();
    let calls = std::cell::Cell::new(0);
    let changed = || {
        calls.set(calls.get() + 1);
        Ok(if calls.get() == 1 {
            INPUT.as_bytes().to_vec()
        } else {
            b"changed".to_vec()
        })
    };
    let outcome = macroscope_app_core::checks::run_program(&draft, &candidate, &changed, &execute);
    assert!(outcome
        .checks
        .iter()
        .any(|c| c.id == "source_unchanged" && !c.passed));
    core.save_api_key("mk-program-test-canary-1234567890")
        .await
        .unwrap();
    let draft = core
        .update_task(
            &draft.draft_id,
            TaskKind::Program,
            "errors.txt",
            Some("OwnerExampleRealText".into()),
        )
        .unwrap();
    let outbound = core.outbound_preview(&draft.draft_id).await.unwrap();
    assert!(outbound.contains_real_rows && outbound.payload_text.contains("OwnerExampleRealText"));
    assert!(core.start_build(&draft.draft_id, false).await.is_err());
}
