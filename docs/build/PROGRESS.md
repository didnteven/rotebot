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
  app-owned vault (private file in debug, Keychain in release); Muse plan sign-in status from the read-only `muse usage`;
  route selection with an honest fallback to the offline demo; Local only persisted.
- **Show** (owner request): a 5-minute, metadata-only recording through the Swift
  observer. Using Gmail in a browser during the recording yields "You used Gmail while
  recording. Connect Gmail so tools can work with it." Titles are matched inside the
  observer; only connector ids and counts are kept.
- **Connections** (owner request): catalog of 9 MCP connectors whose endpoints and OAuth
  discovery were checked live (unauthenticated) on 2026-10-27; OAuth with PKCE, dynamic
  registration or user-supplied client, tokens only in the app-owned vault; suggestions from
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
| Secret never in argv, logs, DB, or ordinary files (P6) | passed | `canary_parent_env_never_reaches_child_and_key_never_argv`, `api_key_route_requires_review_and_key_is_never_stored_in_the_database`, osascript pipe key entry; owner-approved debug exception: private 0600 vault file in a 0700 directory |
| App-owned credential item, never Muse's (§4.5) | passed | `rejects_muse_service_and_bad_refs`; app run created `dev.macroscope.app` items only |
| Data-protection Keychain in a signed build | unverified | Owner decision (2026-10-27): debug builds use a private owner-only directory (0700) and file (0600), avoiding Keychain prompts on rebuild; release builds use the Keychain; recheck the data-protection path once signed |
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
| Tasks beyond CSV with a real model (owner request) | **passed** | 2026-10-27 20:2x: `examples/muse_program_build` — Muse plan wrote an ERROR-line log filter in 57 s, all 5 program checks passed, staged, approved, committed `errors.txt` |
| Web sources: "check the weather" (ADR 0004) | **passed (real)** | 2026-10-27 22:5x: `examples/muse_weather_build` — Muse plan built an Auckland forecast tool in 76 s; host fetched live Open-Meteo; all checks pass; `weather.txt` committed; fetches logged |
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
2. A Meta API key when it arrives: Settings → Enter key… (private file in debug, Keychain in release), then
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

## Task 13: approved connection actions

Runtime v2's synchronous `macroscope.connection.request` records plain JSON
arguments (64 KiB per call; five calls per run), returns `undefined`, and has no
network/token access. The runner protocol includes `planned_calls`. Explicit
connected-service selections carry bounded descriptions and input schemas as
untrusted build data. Host checks run in two fresh workers, compare planned
calls, and validate every call against the selected schemas. Tool manifests and
version grants bind the selected connector/tool names.

Staging freezes calls, server URLs and labelled annotation claims into the plan
hash with the output. Action-only programs need no file output. Create offers
“Let this tool use …” with the service's tools; Run renders full JSON arguments
as inert text and requires a new “I reviewed these actions” tick for each plan
with destructive or non-read-only claims. Trusted commit preflights all stored
connections before sending anything, journals intent/result in order, stops on
the first failure, and never retries uncertain requests. Receipts render bounded
untrusted server summaries as text. Call arguments are encrypted in staging;
crash recovery reconstructs unknown/not-run action receipts without resending.

Tests cover the fake OAuth/MCP server plus a fake program-model endpoint, exact
approved arguments and exactly one send, action-only receipts, undeclared tools,
input schemas, call/argument limits, hash tampering, re-authentication, partial
failure, response loss, inert HTML/script rendering, per-plan checkbox reset,
and crash reconciliation after a send. The desktop backend types are mirrored
in `.dev/task13/backend-types.ts` (the requested in-worktree fallback).

### Task 13b completion — 2026-10-28

Merged `m4/general-tasks` (including `7f17bf1`'s startup backup dialog and
`6b1b15b`'s ADR 0004) before final verification. The merge and all connection-action
changes remain uncommitted. Resumed work fixes recovery after completed actions but
before file publication, preserves an immediate action receipt if file publication
fails, and invalidates outbound review when connection selection changes. Added
regressions verify both interruption windows and that neither path resends.

Final verification: **PASS**, exit **0** for `cargo test --workspace`, both runs of
`bash scripts/check.sh`, the synthetic `bash experiments/runtime-gate-v2/run-canaries.sh`,
and `cd apps/desktop && npx tauri build --debug --no-bundle`. Each workspace run had
**242 passed, 0 failed, 4 pre-existing opt-in tests ignored**; each check run had
**14 UI files / 114 tests passed**. All **60 synthetic canaries** matched their
expected outcomes. Synthetic success is reported separately from OS admission
(**UNKNOWN** in this rerun); existing admission evidence is preserved. No tests
were weakened or deleted. The backend types fallback is byte-identical.

Exact command output tails: [Task 13b verification](task13-verification.md).
Synthetic runtime evidence: [Task 13 canaries](task13-runtime-canaries.md).

## m4/web-sources: merge of m4/connection-actions (2026-10-28)

Codex (gpt-6.1-sol) merged `m4/connection-actions` into `m4/web-sources` and resolved seven
conflicting files (backend.test.ts, tauri.ts, checks.rs, lib.rs, runner.rs, program.rs,
runner tests/runtime.rs). It added a combined end-to-end test: a no-input program reads a
web source and plans a connection call. The test covers a fetch at build and at stage,
refuses to commit until the action is approved, checks the exact output bytes, and asserts
the fake MCP server receives exactly the approved arguments. The canary harness now
requires `planned_calls` to match exactly on every row. Three new connection canaries were
added (planned, argument limit, call limit). QuickJS out-of-memory errors that arrive as a
thrown `null` are now classified as memory errors.

Claude's review found no added skips or ignores and no removed tests (16 tests added).
Backend types are byte-identical to the master contract.

Verification (Claude, rerun): `cargo build -p macroscope-runner --bins` and both runs of
`bash scripts/check.sh` exited **0**. Each run passed **376 Rust tests and 116 UI tests**.
`bash experiments/runtime-gate-v2/run-canaries.sh --os` reported synthetic suite **PASS**
and admission **PASS** (developer boundary; release admission is still pending a signed
bundle).

## m4/schedules — Task 16 (2026-10-28)

Added encrypted schedule grants and Today entries (ADR 0004 §4–5). Daily, weekly,
and month-end-clamped monthly triggers use the local IANA timezone and injected
clock, resolve DST gaps to the first valid minute and overlaps to one occurrence,
and coalesce missed occurrences at launch. The shell evaluates on launch and every
30 seconds, including after wake. Grants bind the tool version, program/sources,
exact host grant and picked folder identity; changes require explicit re-approval.
Emergency stop pauses every schedule; Local only records a skipped network run.
Admission also prevents overlapping runs for one tool during asynchronous fetching.

Scheduled runs use the normal sandbox, source fetch/outbound log, frozen plan and
broker commit. Date-named files remain create-only, using suffixes (2) through (9).
Digest-only runs commit authenticated staged bytes without publishing a file.
Any planned connection calls hold all outputs in Needs you until exact-call review
and approval, or discard. Receipts retain the schedule trigger and schedule id.
Today includes manual no-input runs, bounded output text, output paths, run ids,
dismissal and seven previous days, and participates in Privacy counts and deletion.

The desktop adds Today first in navigation, exact planned-call review, a Schedule
section with frequency/output controls and explicit unattended consent, and an
opt-in login setting. Tauri notification/autostart v2 plugins are registered;
notification permission is requested only on the first explicit notification
schedule approval. Notification bodies contain at most the first output line
(120 characters) or status; clicks focus the app and open Today. A notifier trait
keeps native notification delivery outside core tests. Every added command is
registered in handlers, build COMMANDS, capabilities and generated permissions.
Backend types, mock and Tauri adapters are synchronized; the contract copy uses
`.dev/task16/backend-types.ts` to keep all writes inside this worktree.

Regression coverage includes fake-clock weather runs with exact bytes, receipts,
outbound logs and notifications; weekly/monthly/DST evaluation; persistent missed
runs after reopening; re-approval after tool/source/host/folder changes; Local only
and emergency pause; approved-once/discarded fake MCP calls; collisions and Privacy
wipe. UI tests cover Today, Needs you, unattended approval, next-run/re-approval
states and the login toggle. Verification results are recorded in
[task16-verification.md](task16-verification.md).

Final verification: **PASS**, exit **0** for the runner binaries build, both
`bash scripts/check.sh` runs, all **60/60 synthetic canaries**, and the Tauri debug
build (`--no-bundle`). Each check passed **402 Rust test executions** (283 workspace
+ 119 synthetic app-core) and **120 UI tests across 16 files**, with **0 failures**.
Four pre-existing opt-in tests account for five ignored executions per check;
all new tests run without skips or ignores. `git diff --check` and the backend
contract copy comparison pass. Exact output tails and ignored-test details are
in [task16-verification.md](task16-verification.md).

Claude's review of task 16: no added skips or ignores and no removed tests (17 Rust/UI tests
added). The `navigation.test.tsx` listener count rose from 2 to 3 because of the new
`navigate://today` listener. Checked by reading the code:
- A new schedule's watermark is set to the approval time, so turning a schedule on never
  fires it at once.
- Each occurrence is persisted before any fetch, so a crash cannot replay it.
- Approval, outputs and binding are rechecked after the async fetch and before the unattended
  commit.
- `create_file` is part of the hashed plan.

Fix: the schedule file name template was hard-coded to `weather-{date}.txt` for every tool.
It now derives from the tool's output name (`datedFilename`, with a test).

Rerun by Claude: both `check.sh` runs passed with **402 Rust + 121 UI tests** each. `run-canaries.sh --os`
reported synthetic **PASS** and admission **PASS** (developer boundary).

## Task 17 — connection sources and run-time model steps (2026-10-28)

Branch `m4/model-step`, based on `m4/schedules`.

Programs can declare up to five connection reads (eight sources combined), sharing
`macroscope.source(name)` with web sources. The host preflights connected services,
live tools/list hints and input schemas, substitutes only the documented local-time
templates, caps result text at 2 MiB, and keeps credentials outside the worker. A
candidate pauses for review before private build-time reads or model calls. Saving
binds exact connector/tool/templates into the grant; the staged plan includes those
identities and fetched-result digests. Missing tools request “Reconnect or rebuild”;
auth failures create Needs you entries. Privacy records each read's service, tool
and argument template, without credentials.

`macroscope.model.ask` uses SHA-256 canonical request keys and host-owned replay.
The host admits at most three requests/rounds, validates strict schemas and answers,
and uses the selected Meta or Muse plan route. The demo route produces deterministic
schema-valid answers. Checks replay with identical answers; staged output still
uses normal preview/commit authority. Model logs contain route and byte counts only.
The default per-tool monthly cap is 60 calls, editable in tool detail; durable budget
reservations and counters precede dispatch. Local only blocks both kinds of egress;
Emergency stop cancels in-flight reads/model requests.

In-process fake calendar/Gmail/model coverage includes exact substituted arguments,
review and grants, result caps, auth isolation, three-round replay, invalid requests
and answers, caps, cancellation, and unattended daily/monthly local commits with
Today and notifications. UI coverage checks source/model disclosures and cap editing.
The SDK documents replay and limits; backend types and Tauri commands/adapters are
synchronized. The contract copy is `.dev/task17/backend-types.ts`, within this task's
required workspace boundary.

Verification results and exact tails: [task17-verification.md](task17-verification.md).

Final verification: **PASS**, exit **0** for the runner binaries, both full checks,
all **62/62 synthetic canaries**, and the Tauri debug build (`--no-bundle`). Each
check passed **431 Rust executions** (301 workspace + 130 synthetic core) and
**126 UI tests across 17 files**, with **0 failures**. Five pre-existing ignored
executions remain per check; no new tests skip or ignore. Exact tails and details
are in [task17-verification.md](task17-verification.md).

Claude's review of task 17: no added skips or ignores and no removed tests or assertions
(23 tests added). Edits to existing tests only add the new schema fields.
- The MCP read guard is unchanged. A tool result that echoes the access token is now rejected.
- `tools/call` gets a 16 MB transport cap, so a 2 MB source survives JSON escaping.
- Host checks on model replay:
  - each key matches its request, with at most 3 requests and 3 rounds;
  - Local only and emergency stop apply;
  - the answer must match its schema;
  - the outbound log holds byte counts only.

Fix: the schedule consent sentence always said "each morning". It now states the real
frequency and time ("every day at 7:00").

Open nit: the monthly API-key budget window starts at 00:00 UTC on the 1st, not local
midnight.

Rerun by Claude: both `check.sh` runs passed with **431 Rust + 126 UI tests** each.
`run-canaries.sh --os` reported synthetic **PASS** and admission **PASS** (developer
boundary); the memory canaries are still denied at the OS layer.


## Task 18: Muse Code's second review (`m4/muse-findings-2`)

Branch `m4/muse-findings-2`, based on `m4/model-step`. Muse Code reviewed the
schedules/connection/model-step paths read-only; its report is
[muse-review-2.md](muse-review-2.md). Findings 1–4 and 6–9 were fixed by the
previous worker (staged changes in this tree, kept as-is); finding 5 (bound
model prompt) is fixed on top (unstaged changes). Each fix has a regression
test that fails before the fix. No test was skipped, weakened, or deleted.

- Finding 1 (query-string secrets in web sources): save validation rejects
  credential-like query parameter names with "Web sources can't include keys or
  tokens in the URL", and the outbound log/Privacy keep only `https://host/path`.
  Tests: `query_secret_names_are_rejected_case_insensitively_after_decoding`
  (builder), `QueryCredentials` assertion inside
  `redirects_require_https_same_host_and_effective_port` (websource), updated
  `today_substitution_and_log_shape` (web log strips the query).
- Finding 2 (redirect downgrade/port change): the redirect policy and final-URL
  check require the same scheme (`https`), host, and effective port; release
  clients set `https_only(true)` while the `http://127.0.0.1` test stand-in keeps
  working. Test: `redirects_require_https_same_host_and_effective_port`.
- Finding 3 (month boundaries in local time): the model budget window and every
  other monthly window share one `month_window()` helper using local midnight on
  the 1st in the injected schedule timezone. Test:
  `local_november_cap_and_budget_count_spend_before_utc_november` (Pacific/Auckland,
  2026-11-01 00:30 local counts in November for cap and budget).
- Finding 4 (DST gap in source boundaries): `{today_start}`/`{month_start}` and
  similar resolve gaps by advancing to the first valid minute, reusing
  `schedules::local_boundary`. Test:
  `source_midnight_dst_gaps_advance_to_first_valid_minute` (America/Santiago).
- Finding 5 (bind the model prompt at approval): the program schema's
  `model_step` gains `prompt` (exact text, strict dialect, ≤2,000 chars, length
  validated at build). `macroscope.model.ask(data, schema)` uses the declared
  prompt; the 3-argument form works only when `prompt === declared`, otherwise
  the run fails with "Model prompt does not match the approved instruction".
  The runner takes the declared prompt with the request (`Request`,
  `model.js`, `sdk.rs`, `lib.rs`); the host re-checks every request before any
  model call (`model_step.rs`). The prompt is part of the tool version hash and
  the schedule binding, so changing it requires re-approval, and every review
  surface shows `Sends this instruction: "<prompt>"` (`SourceReview`, save
  screen, schedule approval, tool access list). SDK.md, the builder packet, the
  desktop mock, and existing `model_step` fixtures use the new form. Tests:
  `model_step_needs_an_exact_prompt_of_at_most_2000_chars` (builder),
  `two_argument_ask_uses_the_declared_prompt`,
  `three_argument_ask_with_the_same_prompt_as_declared_still_works`,
  `three_argument_ask_with_a_different_prompt_fails_the_run`,
  `two_argument_ask_without_a_declared_prompt_fails_the_run` (runner),
  `model_prompt_is_bound_at_approval_version_hash_and_schedule` and
  `model_request_with_an_unapproved_prompt_fails_before_any_model_call` (host:
  the fake model endpoint received 0 requests),
  `Sends this instruction: "Summarize these events"` assertion in the outbound
  review UI test (`model-sources.test.tsx`).
- Finding 6 (no model input in argv): the Muse exec passes the prompt via
  `--prompt-file` (0600 file in the private workspace, deleted after the run).
  Test: extended `exec_canary_is_stdin_only_env_is_allowlisted_outputs_scrubbed_and_help_cached`
  (neither the key nor `PRIVATE-MODEL-DATA-CANARY` appears in argv).
- Finding 7 (bound model answers): `validate_schema` rejects `maxLength` over
  20,000, `maxItems`/`minItems` over 500, and depth over 8; the host rejects
  answers over 64 KiB serialized. Tests:
  `schema_string_and_array_bounds_and_depth_are_limited` (runner),
  `oversized_model_answer_is_rejected_before_replay` (host).
- Finding 8 (token echo on the action path): `call_approved_tool` applies the
  same bearer-token echo rejection; the action fails safely with no token in
  receipts, journal, Today, logs, or UI DTOs. Test:
  `action_token_echo_fails_without_persisting_or_exposing_token`.
- Finding 9 (typed run status): staging returns a typed status/reason instead of
  substring-matching messages (`contains("Reconnect")`, …); wording and behavior
  are unchanged. Test:
  `diagnostic_words_do_not_change_typed_today_failure_status`.

Final verification: **PASS** — `cargo build -p macroscope-runner --bins` and
`bash scripts/check.sh` both exited 0. The gate passed **453 Rust tests
(316 workspace + 137 synthetic core; 0 failed)** with 5 pre-existing
environment-gated ignores (Swift observer, live MCP, real-Muse echo, and one
synthetic observer test), and **126 UI tests across 17 files**, plus
formatting, clippy, typechecking, and the production desktop build. No new
skips or ignores; no existing test was weakened or deleted.

Claude's review of task 18. Codex fixed findings 1–4 and 6–9 before hitting its usage limit,
and Muse Code fixed finding 5 and wrote the docs. No skips or ignores were added. No tests
were removed, and the assertion count only grew in every changed test file. Muse's edits to
Codex's files were rustfmt reflows only. The host checks a model request's prompt against the
approved instruction before any model call, in addition to the runner's check.

Rerun by Claude: both `check.sh` runs passed with **453 Rust + 126 UI tests** each.
`run-canaries.sh --os` reported synthetic **PASS** and admission **PASS** (developer boundary).
