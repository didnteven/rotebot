//! Eval-suite tests (plan §9.3): frozen corpus integrity and replay scoring.
//!
//! The corpus test checks every task folder has all four files, manifest
//! hashes match, every `expected.csv` parses with a header,
//! `hidden.json` row_count/columns match `expected.csv`, and no `@`
//! address lives outside `example.invalid`. The scoring test calls the
//! scoring functions directly with two recorded replay answers (one correct
//! pass, one wrong `wrong_output`) plus a missing-attempt INCOMPLETE case.
//! Neither test runs the suite against Muse or Meta; only replay scoring.

use macroscope_app_core::eval::{self, HiddenChecks};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../evals/csv-family")
}

fn task_dirs(root: &Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(root)
        .expect("corpus dir is readable")
        .flatten()
    {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.len() > 3
            && name.as_bytes()[2] == b'-'
            && name[..2].chars().all(|c| c.is_ascii_digit())
            && e.path().is_dir()
        {
            out.push((name[..2].to_string(), e.path()));
        }
    }
    out.sort();
    out
}

#[test]
fn corpus_files_hashes_and_hidden_checks_match() {
    let root = corpus_dir();
    let tasks = task_dirs(&root);
    assert_eq!(tasks.len(), 30, "corpus has 30 distinct tasks");
    for (id, dir) in &tasks {
        for f in ["task.md", "input.csv", "expected.csv", "hidden.json"] {
            assert!(dir.join(f).is_file(), "{id} is missing {f}");
        }
        for f in ["input.csv", "expected.csv"] {
            let bytes = std::fs::read(dir.join(f)).unwrap();
            assert!(bytes.ends_with(b"\n"), "{id}/{f} needs a trailing newline");
            assert!(
                !bytes.contains(&b'\r'),
                "{id}/{f} must use \\n line endings"
            );
        }
        let expected = std::fs::read(dir.join("expected.csv")).unwrap();
        let (columns, rows) = eval::parse_csv(&expected).expect("expected.csv parses");
        assert!(!columns.is_empty(), "{id} expected.csv needs a header");
        let hidden: HiddenChecks =
            serde_json::from_slice(&std::fs::read(dir.join("hidden.json")).unwrap())
                .expect("hidden.json parses");
        assert_eq!(hidden.row_count, rows, "{id} hidden row_count");
        assert_eq!(hidden.columns, columns, "{id} hidden columns");
        let input = std::fs::read(dir.join("input.csv")).unwrap();
        let (_, in_rows) = eval::parse_csv(&input).expect("input.csv parses");
        assert!((8..=60).contains(&in_rows), "{id} input has 8-60 rows");
        let text = std::fs::read_to_string(dir.join("expected.csv")).unwrap();
        for banned in &hidden.must_not_contain {
            assert!(
                !text.contains(banned.as_str()),
                "{id} leaks a banned string"
            );
        }
        for f in ["task.md", "input.csv", "expected.csv", "hidden.json"] {
            let content = std::fs::read_to_string(dir.join(f)).unwrap();
            for (i, _) in content.match_indices('@') {
                let lo = i.saturating_sub(80);
                let hi = (i + 80).min(content.len());
                assert!(
                    content[lo..hi].to_lowercase().contains("example.invalid"),
                    "{id}/{f} has an @ address outside example.invalid"
                );
            }
        }
    }
    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(root.join("manifest.json")).expect("manifest.json exists"),
    )
    .expect("manifest.json parses");
    assert_eq!(manifest["repetitions"], 3);
    assert_eq!(manifest["pass_threshold"], 0.75);
    assert_eq!(manifest["route"], "muse_plan");
    assert_eq!(manifest["model"], "muse-spark-1.3");
    let files = manifest["files"]
        .as_array()
        .expect("manifest files is a list");
    assert_eq!(files.len(), 120, "manifest freezes 120 corpus files");
    for entry in files {
        let rel = entry["path"].as_str().expect("file path");
        let want = entry["sha256"].as_str().expect("file sha256");
        let bytes = std::fs::read(root.join(rel)).unwrap_or_else(|_| panic!("{rel} exists"));
        assert_eq!(
            format!("sha256:{:x}", Sha256::digest(&bytes)),
            want,
            "{rel} hash"
        );
    }
}

#[test]
fn replay_scoring_passes_correct_rejects_wrong_and_marks_missing_incomplete() {
    let root = corpus_dir();
    for (id, want_pass) in [("01", true), ("02", false)] {
        let task = task_dirs(&root)
            .into_iter()
            .find(|(t, _)| t == id)
            .unwrap_or_else(|| panic!("task {id} exists"))
            .1;
        let recorded: serde_json::Value = serde_json::from_slice(
            &std::fs::read(root.join("replay").join(format!("{id}.json")))
                .unwrap_or_else(|_| panic!("replay/{id}.json is recorded")),
        )
        .expect("replay answer parses");
        let output = recorded["output_csv"].as_str().expect("replay output_csv");
        let expected = std::fs::read(task.join("expected.csv")).unwrap();
        let hidden: HiddenChecks =
            serde_json::from_slice(&std::fs::read(task.join("hidden.json")).unwrap()).unwrap();
        let result = eval::score_output(output.as_bytes(), &expected, &hidden);
        if want_pass {
            assert!(result.is_ok(), "replay {id} passes");
        } else {
            let (class, summary) = result.expect_err("replay 02 fails");
            assert_eq!(class, eval::FailureClass::WrongOutput);
            assert!(summary.contains("line"), "{summary}");
            assert!(summary.chars().count() <= 201, "{summary}");
        }
    }
    assert_eq!(eval::verdict(1, 1, 1, 0.75), "INCOMPLETE");
    assert_eq!(eval::verdict(1, 2, 0, 0.75), "FAIL");
    assert_eq!(eval::verdict(3, 4, 0, 0.75), "PASS");
}

#[test]
fn sandbox_rejections_are_generation_failures_not_infrastructure() {
    use macroscope_app_core::eval::{classify_failure, FailureClass};
    let sandbox = "Checks failed: generated runner failed: runtime: JavaScript execution failed: use synchronous SDK code only; eval, Function, Worker, Date, Math.random and OS APIs are unavailable.";
    assert_eq!(classify_failure(sandbox, "run"), FailureClass::ChecksFailed);
    assert_eq!(
        classify_failure("Muse usage limit reached; try again later", "build"),
        FailureClass::Infrastructure
    );
    assert_eq!(
        classify_failure("The model route is unavailable", "build"),
        FailureClass::Infrastructure
    );
    assert_eq!(
        classify_failure("forbidden: output path escapes the folder", "run"),
        FailureClass::ForbiddenEffect
    );
    assert_eq!(
        classify_failure("Build produced no program", "build"),
        FailureClass::BuildFailed
    );
    use macroscope_app_core::dto::BuildFailureKind;
    use macroscope_app_core::eval::classify_build_failure;
    for kind in [BuildFailureKind::Checks, BuildFailureKind::Internal] {
        assert_eq!(
            classify_build_failure(kind, sandbox),
            FailureClass::ChecksFailed
        );
    }
    assert_eq!(
        classify_build_failure(
            BuildFailureKind::Internal,
            "The build failed: Muse deadline exceeded"
        ),
        FailureClass::Infrastructure
    );
}
