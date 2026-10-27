# csv-family eval suite (plan §9.3)

What it measures: private generated execution on one admitted family — 30
distinct local CSV tasks, three repetitions each, scored against independently
authored `expected.csv` bytes plus `hidden.json` checks (row count, columns,
banned strings). The gate needs ≥75% bounded hidden-test success, zero
forbidden effects, all accepted requests counted, and infrastructure failures
labeled separately.

Layout: `NN-short-name/` holds `task.md` (the plain-language request),
`input.csv` (fabricated data, 8–60 rows, `example.invalid` emails),
`expected.csv` (exact bytes, `\n` endings, trailing newline, authored by
reasoning about the request — never by running generated code), and
`hidden.json` (`row_count`, `columns`, `must_not_contain`). `manifest.json`
freezes the 120 corpus files (sha256), route (`muse_plan`), model
(`muse-spark-1.3`), Muse binary version (from the native binary file name),
runner/QuickJS versions, the §9.3 price constants ($1.20 + $0.016 = $1.216
per attempt), repetitions (3), threshold (0.75), and permissions (input read,
one output create, no network, no connections). `replay/` holds recorded
model answers for scoring tests only.

Run:

```sh
cargo build -p macroscope-runner --bins
cargo run -p macroscope-app-core --example eval_suite -- --route muse_plan --tasks all --reps 3 --out results/muse-plan
cargo run -p macroscope-app-core --example eval_suite -- --route api_key --tasks all --reps 3 --out results/api-key --confirm-spend
cargo run -p macroscope-app-core --example eval_suite -- --route replay --tasks 01,02 --reps 1 --out /tmp/eval-replay
```

`--route muse_plan` uses the owner's existing Muse plan sign-in read-only
(the same check the app uses) and never calls login/logout/auth; a
usage-limit failure stops the whole run as INCOMPLETE. `--route api_key`
reads the key only from the app's vault flow or `META_API_KEY` (never argv),
refuses to start without one, prints the planned maximum spend
(tasks × reps × $1.216), and requires `--confirm-spend`. `--route replay`
scores `replay/<id>.json` without any model; results are labeled
`"replay": true, "generation_quality": "not measured"`. The suite was not run
against Muse or Meta here; only the replay route runs in tests.

Cost note: at planning rates 30 tasks × 3 reps × $1.216 = $109.44 maximum;
measured per-attempt costs are recorded in `results.json` (`cost_micros` for
`api_key`). Paid ceilings are funding gates, not spend authorization.

Read results: `results.json` holds every attempt (pass/fail, failure class,
diff summary of the first differing line ≤200 chars, duration, model calls,
cost), per-task and overall rates, infrastructure counted separately, and the
verdict PASS/FAIL/INCOMPLETE (INCOMPLETE when any attempt is missing).
`results.md` is the short table. Results never store input rows or outputs.
