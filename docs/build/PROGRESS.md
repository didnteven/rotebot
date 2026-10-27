# Build progress — 2026-10-27

What was built against `plan/PLAN.md`, with the owner's overrides (ADR 0001), and the
status of every gate touched: **passed** (evidence in a test or recorded run),
**failed** (evidence of failure, decision recorded), or **unverified** (needs something
this build could not supply). Branch order and PR plan: [BRANCHES.md](BRANCHES.md).

Who did what: Codex (`gpt-6.1-sol`) wrote the foundation, CSV engine and broker, model
access, connections, observer, runtime experiment, and UI. Claude reviewed and tested
every branch (including outside the agent sandbox), fixed what the reviews found, and
wrote storage, the app core, the Tauri shell, Show, and connections wiring while Codex was
rate-limited; Codex then reviewed that code independently (`m3/hardening`).
Muse Code (`muse-spark-1.3`, contributor mode) helped once the owner signed in at 18:43: it
reviewed the code read-only and found 8 defects, which Codex then fixed (`m3/muse-findings`,
report in `muse-review.md`). Muse's plan route also produced the first real model-built tool.

## Task 11: text programs through runtime v2 (`m4/general-tasks`)

Generated execution is enabled by default in app-core and desktop at the accepted
**developer boundary** ([ADR 0003](../adr/0003-programs-through-runtime-v2.md)).
Release admission still requires a signed immutable bundle and fresh release receipts.
The macOS bundle script includes the runner, launcher and Seatbelt profile next to
the app executable; Tauri development/build hooks build the runner binaries in the
inherited target directory. Production uses mandatory Seatbelt with no synthetic fallback.

The create flow now says “Describe a task”, accepts selected UTF-8 text up to 10 MB,
proposes CSV mapping only for parsed CSV and finite mapping descriptions, and lets
the user switch to a sandboxed program. Programs create one confirmed .csv, .tsv,
.txt, .md or .json output. Their strict model schema requires summary/output_name/code,
with no extra properties or unsupported dialect keywords. The host replaces every
input value token with invented shape samples, including headers, JSON keys and XML
tags. An optional user example is identified as real data in the outbound review and
requires the checkbox. Muse plan and direct Meta key routes share the builder;
offline programs explain that a model is required.

Host checks run two fresh bounded children, require identical bytes, the confirmed
single output name, nonempty UTF-8 and CSV/TSV/JSON syntax where selected, ordered
example lines, and unchanged source. Date is absent and Math.random is removed and
frozen. Imports, eval, Worker, oversized output and infinite loops fail checks with
safe diagnostics. Saved source is part of the immutable version record/tree hash.
Saved programs stage through RunnerEngine and the existing broker approval,
create-only commit and receipt path. Text comparisons and Advanced code details are
inert renderer text. Program export includes its versioned source.

Validation on 2026-10-27:

| Command | Exact result | Output tail / evidence |
|---|---|---|
| `cargo test --workspace` | **PASS**, exit 0; 215 passed, 0 failed, 4 existing opt-in tests ignored | Final doc-test tail: `test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s`; full log `.dev/task11/cargo-test-workspace.log` |
| `bash scripts/check.sh` (first) | **PASS**, exit 0; formatting, Clippy with warnings denied, Rust tests, TypeScript, 106 UI tests, production UI build | `✓ built in 92ms`; `.dev/task11/check-1.log` |
| `bash scripts/check.sh` (second) | **PASS**, exit 0; same checks, 106 UI tests | `✓ built in 99ms`; `.dev/task11/check-2.log` |
| `bash experiments/runtime-gate-v2/run-canaries.sh` | **FAIL / admission UNKNOWN**, exit 1; all 57 synthetic expectations matched (55 DENIED, 2 PASS); 57 OS rows UNKNOWN because this was the requested synthetic-only run | `orders: synthetic=PASS, OS=UNKNOWN` / `stdin-flood: synthetic=DENIED, OS=UNKNOWN` / `Verdict: UNKNOWN; evidence: docs/build/runtime-lock-v2.md`; archived [synthetic report](task11-runtime-canaries.md), [raw receipts](../../experiments/runtime-gate-v2/task11-synthetic-results.json), `.dev/task11/runtime-canaries.log` |
| `cd apps/desktop && npx tauri build --debug --no-bundle` | **PASS**, exit 0 | ``Finished `dev` profile [unoptimized + debuginfo] target(s) in 45.09s``; `Built application at: /Volumes/250GB SAMSUNG 970 EVO PLUS /dev-tools/macroscope-cache/targets/11-general-tasks/debug/macroscope-desktop`; `.dev/task11/tauri-build.log` |

The canary harness returns a nonzero exit for UNKNOWN; its admission rule was not
weakened. Its synthetic report was archived separately so the original runtime-v2
OS admission evidence and hashes remain intact in `runtime-lock-v2.md` and the
original `results.json`. This run does not claim fresh OS or signed-release admission.

A routing regression also covers duplicate removal, row removal, arithmetic and search requests that include CSV cleaning words. Developer discovery handles both test and example executable directories.

New tests cover the fake Responses log extractor through ready → save → stage →
approve → exact commit, private-token exclusion and exact reviewed packet equality,
malicious/limit/clock/random programs, determinism, examples, format parsing and
source changes. UI coverage includes program creation, real-example consent,
offline explanation and inert source/output rendering; test worker concurrency is
bounded to four. Existing assertions were retained, with create-flow label updates.

The real-Muse owner example is `crates/app-core/examples/muse_program_build.rs`:
`cargo build -p macroscope-runner --bins`, then
`cargo run -p macroscope-app-core --example muse_program_build`. It was **not run**
in this task. No account/authentication changes were performed.

The backend contract is updated in `apps/desktop/src/backend/types.ts`. The explicit
worktree-only constraint prevents updating the external shared copy, so an identical
fallback is at `.dev/task11/backend-types.ts` (verified with `cmp`).

## What works now

- **Tell flow, end to end, in the real app process**: choose or sample a CSV → task
  summary with the date-meaning question → outbound review (exact payload) → build →
  host checks on your own file plus synthesized stop cases → compare → save an immutable
  tool → stage a run → review → create the new CSV (never overwrites) → receipt.
  The fixture produces `expected.csv` byte for byte through the same IPC commands the UI
  calls (`frontend_argument_shapes_drive_the_tell_flow_over_ipc`).
- **Model access settings** (owner request): Meta API key in a native hidden dialog →
  app-owned Keychain item; Muse plan sign-in status from the read-only `muse usage`;
  route selection with an honest fallback to the offline demo; Local only persisted.
- **Show** (owner request): a 5-minute, metadata-only recording through the Swift
  observer. Using Gmail in a browser during the recording yields "You used Gmail while
  recording. Connect Gmail so tools can work with it." Titles are matched inside the
  observer; only connector ids and counts are kept.
- **Connections** (owner request): catalog of 9 MCP connectors whose endpoints and OAuth
  discovery were checked live (unauthenticated) on 2026-10-27; OAuth with PKCE, dynamic
  registration or user-supplied client, tokens only in the Keychain; suggestions from
  task text and from Show, with dismiss/snooze. Wired through the core and shell; tested
  end to end against an in-process OAuth + MCP server.

## Independent review (`m3/hardening`)

After the Claude-written parts landed, Codex reviewed them as a skeptic and fixed 28
findings, each with a regression test that failed first
([codex-review-of-core.md](codex-review-of-core.md)). The most important: host checks now
enforce every confirmed rule (a schema-valid model answer could otherwise change a date's
meaning and still pass), stopping a cloud build can no longer wedge Local only, the
$25/month build ceiling is enforced, saved tools must match the draft revision they were
built from, and Show always kills an unresponsive observer. Claude reran everything outside
the agent sandbox: 176 Rust tests and 56 UI tests pass, plus the real Swift observer and
Muse 1.4.13 echo tests, and the app bundle builds and launches.

## Gates

| Gate (plan section) | Status | Evidence |
|---|---|---|
| Secret never in argv, logs, DB, files (P6) | passed | `canary_parent_env_never_reaches_child_and_key_never_argv`, `api_key_route_requires_review_and_key_is_never_stored_in_the_database`, osascript pipe key entry |
| App-owned credential item, never Muse's (§4.5) | passed | `rejects_muse_service_and_bad_refs`; app run created `dev.macroscope.app` items only |
| Data-protection Keychain in a signed build | unverified | Unsigned dev builds fall back to the login keychain (`-34018`); recheck once signed |
| Local history deletion and restartable cleanup (P4, §§2.7/3.3) | passed | `history_wipe_keeps_tools_credentials_and_user_outputs_then_optional_tools_wipe`, `startup_resumes_both_tombstone_scopes_before_recovering_jobs`, `deletion_during_dispatch_awaits_workers_and_cannot_resurrect_history`; persistent tombstone before row/file cleanup, admission/cancellation barrier first |
| Tool export (§2.7) | passed | `export_is_structural_create_only_and_refuses_symlink_swaps`; native folder picker, four structural files, pinned no-follow directory writes, create-only destination; no samples or row data |
| Tool revoke/restore (§2.7) | passed | `pause_and_restore_never_reauthorize_a_previously_staged_run`; paused tools refuse staging/commit, restoration uses a fresh grant revision; real IPC regression |
| Encrypted metadata, staging (P3) | passed | `wrong_key_is_rejected_and_file_has_no_plaintext`; XChaCha20-Poly1305 staging tamper tests |
| Stage once → approve exact bytes → atomic no-overwrite commit (§7.3) | passed | broker adversarial suite (tamper, input/destination swap, revocation, crash reconcile, no rerun) |
| Host-owned checks gate "ready" (§7.4) | passed | `a_schema_valid_but_wrong_answer_is_stopped_by_host_checks`, `candidates_cannot_replace_confirmed_rules_even_when_rows_still_look_valid` |
| Spend admission limits (§9.1: $2/build, $25/month) | passed | `unknown_charges_keep_the_monthly_build_reservation_and_block_admission` |
| Local-only barrier during active work (§2.7) | passed | `local_only_during_a_request_cancels_and_acknowledges_the_barrier`, `local_only_refuses_api_key_network_checks` |
| Outbound review shows the exact payload; synthetic only (P5) | passed | `a_correct_model_answer_becomes_ready_with_cost_and_a_synthetic_packet` |
| Renderer safety on shipped text surfaces (§8.4, P11) | passed | `apps/desktop/src/renderer-adversarial.test.tsx`: each of 10 payloads across CSV cells/headers, file names/input/output/export paths, model mapping columns/sources/constants, outbound review/history, MCP names/URLs/authorization fields/tools/descriptions, errors (build/run/connection/Settings/Show), activity titles/details, receipt paths, Show app names and suggestions. Payloads: img/onerror, script, javascript URL (text and URL fields), SVG/onload, CSS resource URL, 10,000 characters, RTL override, zero-width controls, terminal ESC, Markdown javascript link. Literal text, computed wrapping/width, visible file/path control escapes in `bdi`, inert DOM, HTTPS-only/confirmed external opening, and network/navigation/clipboard/privileged-dispatch canaries. `renderer-safety.test.ts` pins local-resource/IPC CSP and object/base/form/ancestor denial; unsafe HTML API guard retained. Automated DOM/style evidence uses jsdom; packaged WebKit resource enforcement is not independently instrumented. |
| Responses request shape (`text.format`, `store:false`) (§4.1) | passed | gateway and cloud-build tests against a local stand-in |
| Real Meta API call with a real key, billing attribution (M0 day 5) | unverified | No key supplied yet |
| Muse MSP transport against the installed binary (§4.3) | passed | echo conformance on Muse 1.4.13 (fingerprint changed from the plan's 1.4.11 pin; additive) |
| Muse self-update hazard | found and handled | Launcher auto-updated 1.4.11 → 1.4.13 and deleted 1.4.11; app runs only native binaries (ADR 0002) |
| Muse plan-route build with a real sign-in | **passed** | 2026-10-27 18:5x: `cargo run -p macroscope-app-core --example muse_plan_build` → Ready in 39 s via `muse-spark-1.3` on the owner's plan; all 7 host checks pass on the sample; mapping equals the confirmed rules |
| Meta structured-output schema dialect | **passed (real service)** | Real 400s found and fixed: no numeric `format`, no `oneOf` (use `anyOf`), no `default`, `required` lists every property (`model_schema_uses_the_strict_dialect_meta_accepts`). Applies to the direct API route too |
| Muse native-tool containment (P7) | unverified | Needs a signed-in Muse to exercise tools; plan route requires an explicit owner opt-in meanwhile |
| Generated TypeScript runtime admission (§7.3, P8) | **failed** | Seatbelt denied all effect canaries, but `data:` imports run and RSS polling overshoots 256 MiB; finite CSV mapping ships (ADR 0002) |
| Generated JavaScript runtime v2 (QuickJS + Seatbelt) | **passed (developer boundary)** | `docs/build/runtime-lock-v2.md`: 57 canaries under the real sandbox, 55 denied/killed, 2 benign tools exact; memory canary peak 69.7 MB vs 64 MiB cap. Release admission pending a signed bundle |
| Show observer: no titles/pixels leave the process (P1) | passed | observer tests; core ignores forged-session events; real observer start/stop test |
| Show with Accessibility granted on a real Gmail tab | unverified | Mac was locked during the build; grant Accessibility to Macroscope and try Show |
| Signed bundle TCC attribution, N-1 update (§5.3, P9) | unverified | No signing identity yet |
| MCP discovery matches real servers | passed | `live_discovery_matches_catalog` (8 providers) |
| A real OAuth sign-in to a provider | unverified | Needs the owner; Gmail/Slack/GitHub need an OAuth client of your own |
| Background footprint under 350 MiB, quiescent CPU (§9.2) | passed (initial sample) | Debug bundle idle 60 s on 2026-10-27: app process 73 MiB RSS, 0.40 s CPU including startup. WebKit helper processes not counted; release build and an 8-hour soak still to do |
| Windows code paths compile | passed (partial) | `cargo check` + `clippy -D warnings` for `x86_64-pc-windows-msvc` on broker (MoveFileExW no-replace), vault (Credential Manager), executor, domain, policy. Crates with C deps (TLS, SQLCipher) need Windows CI |
| Windows runtime behaviour | unverified | Plan M6: packaged MSIX, interactive and physical testing |

## Owner to-do (only things the build could not do)

1. `muse login` in Terminal if you want the Muse plan route; then Settings → Check again.
2. A Meta API key when it arrives: Settings → Enter key… (stored in your Keychain), then
   `cargo run -p macroscope-model-gateway --example smoke` for one capped real call
   (catalog + ~64 output tokens) and check the charge in your Meta dashboard.
3. Try Show with a browser on Gmail; macOS will ask to allow Accessibility for Macroscope.
4. For Gmail's MCP server: Google Workspace Developer Preview, a Cloud project with the
   Gmail API and Gmail MCP API enabled, and a desktop OAuth client (Connections → Gmail).


## Privacy controls (Task 07)

Added local deletion previews and confirmation, restartable deletion journals,
structural tool export, pause/resume with fresh grant revisions, another-sample
host checks, and the five-tool tray menu with Run/Create navigation. P4 deletion,
export, and revoke evidence is listed in the gates table above.

Final verification: **185 Rust tests passed, 4 existing checks ignored; 61 UI tests
passed**. `cargo test --workspace`, both final `bash scripts/check.sh` runs, and
`npx tauri build --debug --no-bundle` all exited 0. Exact tails and the external
contract-mirror scope clarification are recorded in
[Task 07 verification](task-07-verification.md).

## Renderer safety (Task 08)

P11 coverage is recorded in the gates table. The new adversarial suite has 40
surface/payload cases plus HTTPS documentation confirmation/cancel coverage.
File/path display exposes hidden controls without changing backend values;
long text wraps, external documentation requires confirmation, and CSP blocks
object, base, form, and framing escape routes.

Final verification: both `bash scripts/check.sh` runs exited 0, each with
**185 Rust tests passed, 4 existing tests ignored, and 103 UI tests passed**.
Their build tails were `✓ built in 664ms` and `✓ built in 89ms`.
`cd apps/desktop && npx tauri build --debug --no-bundle` exited 0:
`Finished dev profile [unoptimized + debuginfo] target(s) in 52.29s`.
No Rust source changes were needed.


## Muse Code review (Task 10)

Muse Code (`muse-spark-1.3`, contributor mode) reviewed the repository read-only.
Codex reproduced and fixed seven defects: URL-bound OAuth clients and deletion on
URL edits, PRM fallback after per-URL failures, no-follow sample restoration,
propagated build/outbound persistence failures, zeroizing OAuth request bodies,
shared basename admission, and the current Show policy epoch. Finding 5 is
intentional: Local-only revokes model egress while deterministic runs continue;
a new regression confirms commit still works and grant revocation still blocks it.
The UI returns to “Add your OAuth client ID” after a URL edit. Legacy unbound
client records require re-entry.

Each confirmed finding failed a regression before its fix. Existing tests and the
[core review guarantees](codex-review-of-core.md) remain in place. Full verdicts,
test names, reproduction evidence and final verification are recorded in
[muse-review.md](muse-review.md).


Final Task 10 verification: `cargo test --workspace` and both required
`bash scripts/check.sh` runs exited 0. Each passed **199 Rust tests**, with **4
existing integration tests ignored**; each script also passed **104 UI tests**,
formatting, clippy, typechecking, and the production build. Earlier intermittent
Show startup and SQLCipher initialization failures, which were not reproduced in
these three final runs, are recorded with the verification evidence in the report.
No test was weakened or deleted.
