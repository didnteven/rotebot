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
Muse Code was never signed in during the build, so it could not help (see below).

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

## Gates

| Gate (plan section) | Status | Evidence |
|---|---|---|
| Secret never in argv, logs, DB, files (P6) | passed | `canary_parent_env_never_reaches_child_and_key_never_argv`, `api_key_route_requires_review_and_key_is_never_stored_in_the_database`, osascript pipe key entry |
| App-owned credential item, never Muse's (§4.5) | passed | `rejects_muse_service_and_bad_refs`; app run created `dev.macroscope.app` items only |
| Data-protection Keychain in a signed build | unverified | Unsigned dev builds fall back to the login keychain (`-34018`); recheck once signed |
| Encrypted metadata, staging (P3) | passed | `wrong_key_is_rejected_and_file_has_no_plaintext`; XChaCha20-Poly1305 staging tamper tests |
| Stage once → approve exact bytes → atomic no-overwrite commit (§7.3) | passed | broker adversarial suite (tamper, input/destination swap, revocation, crash reconcile, no rerun) |
| Host-owned checks gate "ready" (§7.4) | passed | `a_schema_valid_but_wrong_answer_is_stopped_by_host_checks`, `checks::tests` |
| Outbound review shows the exact payload; synthetic only (P5) | passed | `a_correct_model_answer_becomes_ready_with_cost_and_a_synthetic_packet` |
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
