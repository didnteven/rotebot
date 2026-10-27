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
Muse Code (`muse-spark-1.3`, contributor mode) was set up as a helper (`.dev/muse_help.sh`,
native binary only), but Muse was never signed in during the build, so it could not run.

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
| Muse plan-route build with a real sign-in | unverified | `muse usage`: not logged in. Run `muse login`, then Settings → Check again |
| Muse native-tool containment (P7) | unverified | Needs a signed-in Muse to exercise tools; plan route requires an explicit owner opt-in meanwhile |
| Generated TypeScript runtime admission (§7.3, P8) | **failed** | Seatbelt denied all effect canaries, but `data:` imports run and RSS polling overshoots 256 MiB; finite CSV mapping ships (ADR 0002) |
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
