# Independent core review — Task 06

Reviewed on 2026-10-27 against PLAN §§2.7, 3.2–3.3, 4.5, 7.3–7.4,
8.2–8.4, ADRs 0001/0002, and `PROGRESS.md`. Findings below use the final
working-tree line numbers. Each finding family has a regression that failed before
its fix. No existing tests were deleted or relaxed. Existing UI assertions for the
invented underscore Google IDs and Gmail documentation URL were corrected to the
shared catalog's values.

| Severity | File:line | Finding | Fix | Regression test |
|---|---|---|---|---|
| High | `crates/app-core/src/checks.rs:110` | Schema-valid candidates could change confirmed date meaning, copy instead of transform, or substitute constants and still pass checks. Ambiguous dates made the date-order bypass invisible on the selected file. | Check every confirmed mapping rule in addition to parsing, row counts, and negative cases. | `candidates_cannot_replace_confirmed_rules_even_when_rows_still_look_valid` |
| Medium | `crates/app-core/src/checks.rs:213` | The missing-column fixture falsely failed valid one-column mappings; a constant did not require its unused contract inputs as staging does. | Check required contract columns through the executor, including the zero-column negative case. | `confirmed_constant_outputs_and_single_column_inputs_can_pass_checks` |
| High | `crates/app-core/src/lib.rs:877` | Aborting a cloud worker lost its policy acknowledgment, leaving Local-only permanently stopping. Admission closed after the worker scan, and jobs fetched a fresh epoch at execution. | Close admission first; bind the admission epoch; signal cooperative cancellation; drop transports, kill process groups, await reapers, then acknowledge. Serialize privacy/key controls and build admission. Persist transitions under the build-state lock. | `local_only_during_a_request_cancels_and_acknowledges_the_barrier`; supplemental `cancellation_scope_waits_for_dropped_children_to_be_reaped` |
| High | `crates/app-core/src/lib.rs:789` | Test key still dispatched network requests in Local-only. | Admit catalog checks through policy and cancel their transport on revocation. | `local_only_refuses_api_key_network_checks` |
| High | `crates/app-core/src/lib.rs:1071` | The reservation query existed but paid admission never enforced the plan's $25 monthly build ceiling. | Check UTC calendar-month spend plus outstanding reservations before reserving another $2 build, while admission is locked. | `unknown_charges_keep_the_monthly_build_reservation_and_block_admission` |
| Medium | `crates/app-core/src/lib.rs:465` | Preview, drafting, and checks used ordinary path reads: replacing the selected file with a symlink read outside the selection; metadata-before-read also did not bound a growing file. | Reuse the broker's bounded snapshot reader with no-follow traversal and regular-file checks. | `preview_and_drafting_refuse_symlink_swaps_after_selection` |
| High | `crates/app-core/src/lib.rs:1473` | Saving a ready build reread a subsequently edited draft, mixing unchecked rules/summary with the old candidate. | Persist the admitted draft revision privately with build state and require a new build after edits, including after restart. Public DTO shapes stay unchanged. | `saving_a_build_after_editing_its_draft_requires_a_new_build` |
| Medium | `crates/app-core/src/lib.rs:1578` | Any number of staged runs could be admitted, including several for one tool. | Serialize staging/publication admission; enforce one staged run per tool and two total. Mismatched approvals also clean their abandoned staging. | `staging_enforces_per_tool_and_global_run_limits`; existing `approval_must_match_and_staged_runs_can_be_discarded` |
| Medium | `crates/app-core/src/lib.rs:339` | Restart left unpublished encrypted staging directories and unreachable runs behind. | Reconcile journals; discard and remove staging proven unpublished, including directories with no journal. Preserve uncertain publication journals. | `unfinished_staging_is_discarded_and_cleaned_on_restart` |
| Medium | `crates/app-core/src/lib.rs:1863` | Outbound expiry only ran at startup, exposing expired payloads/rows during a long app session. | Purge before reading outbound records and computing Privacy counts. | `outbound_expiry_applies_without_restarting_the_app` |
| High | `crates/app-core/src/lib.rs:188` | An existing encryption key of the wrong length was silently overwritten with a new key. | Fail closed and preserve the existing vault item. | `malformed_existing_vault_keys_are_never_silently_replaced` |
| Medium | `crates/app-core/src/drafts.rs:32` | Draft parsing trimmed header names although execution requires exact names; duplicate/empty headers and extremely wide schemas were admitted. | Preserve exact names, reject blank/duplicate headers, cap width at 1,024 columns. | `headers_keep_exact_names_and_reject_empty_duplicate_or_excessive_width` |
| Medium | `crates/app-core/src/drafts.rs:69` | Preview truncation also truncated the contract's input column names, making long-header drafts unusable. | Keep full header identities; truncate preview cell values only. | `long_headers_remain_exact_in_the_contract_and_mapping` |
| Medium | `crates/app-core/src/drafts.rs:252` | Only the first ambiguous date column got a question; later columns silently defaulted to month-first. | Ask a distinct question for every ambiguous column and apply each answer to its own source. | `every_ambiguous_date_column_gets_its_own_question` |
| Medium | `crates/storage/src/db.rs:250` | An older app accepted a database with an unknown future schema. | Reject newer schema versions before applying migrations. | `a_future_schema_is_refused_without_modifying_it` |
| Medium | `crates/storage/src/db.rs:711` | No usage legs was reported as known zero cost. | Return unknown until at least one leg exists and all legs have known usage. | `missing_usage_legs_keep_the_reservation_unknown` |
| High | `crates/vault/src/macos.rs:53` | A signed upgrade could not find a development login-keychain key when the protected store returned not-found, risking database-key replacement. | Find the app-owned legacy item, promote it to the device-only protected store, then remove the legacy copy. Entitlement failures still use the development fallback. | `signed_upgrade_migrates_the_development_key_instead_of_generating_a_new_one` |
| Medium | `crates/vault/src/macos.rs:72` | Deletion checked just one keychain variant, leaving an older app-owned credential copy recoverable. | Delete both variants; successful protected writes also remove the legacy item. | `deletion_removes_both_app_owned_keychain_variants` |
| High | `crates/app-core/src/show.rs:258` | Stop could report Finished while an unresponsive observer continued running; a reader finishing later could affect a newer session. | One supervised reader owns kill/reap and completion; serialize start/pause/stop and scope completion to the session. Bound lines before allocation, cap session events/app counts, and yield during floods. | `stop_kills_and_reaps_an_observer_that_ignores_control`; supplemental `concurrent_start_spawns_only_one_observer` |
| Medium | `crates/app-core/src/show.rs:311` | In-flight observer events were persisted even after pause. | Drop focus/match events while paused. | `observer_events_while_paused_do_not_record_detections` |
| Medium | `crates/app-core/src/connections.rs:268` | Failed discovery changed service state without updating the read snapshot. Refreshing after releasing the service lock could also publish an older snapshot. Cancellation registration had gaps around discovery/completion. | Publish snapshots under the service lock on success and failure; register abort handles before yielding. Pending authorization URLs expire with the pending flow. | `failed_discovery_updates_the_snapshot_and_cancel_revokes_browser_authority`; existing pending-cancel and end-to-end connection tests |
| Medium | `crates/connections/src/oauth.rs:699` | Sequential callback reads let one slow client block the real redirect for five seconds; many clients could starve the complete sign-in. | Up to 32 concurrent bounded readers, evict stalled clients at saturation, and bound response writes. | `callback_accepts_the_real_redirect_while_many_clients_stall` |
| Medium | `apps/desktop/src-tauri/src/main.rs:514` | A lagged broadcast receiver permanently stopped forwarding build/Show events. | Recover from lag using a fresh state snapshot; also snapshot after build subscription. | `event_forwarding_recovers_from_a_lagged_receiver` |
| Low | `apps/desktop/src-tauri/src/main.rs:489` | The shell refused Slack's catalog documentation link. | Admit its exact documentation host. | `every_catalog_documentation_link_can_be_opened` |
| Medium | `apps/desktop/src/backend/mock.ts:237` | The mock invented endpoints, IDs, categories, and setup text. | Import `contracts/connectors.json`; mirror `Connections::info`, including registration, verification, setup and fixed-port redirect semantics. | `mock connector DTOs match every catalog field exposed by the core` |
| Medium | `apps/desktop/src/Settings.tsx:167` | The Muse opt-in existed in the backend but had no UI. | Show the signed-in checkbox and exact requested explanation; default unchecked; bind both enabling and revoking. | `Muse containment opt-in defaults off, persists, and can be revoked` |
| Low | `apps/desktop/src/backend/tauri.ts:17` | Malformed rejection payloads were stringified as displayable errors, including arrays of arbitrary data. | Accept string messages; use a fixed fallback for malformed objects. Preserve typed backend messages/codes and existing Error/string behavior. | `malformed rejection objects never stringify arbitrary payloads as errors` |
| Low | `apps/desktop/src/Connections.tsx:152` | Connection success always claimed 12 tools. | Display the count returned by the backend. | `successful connections show the tool count reported by the backend` |

Checked and found fine:

- Shell commands resolve native picker handle IDs; forged display paths do not authorize
  reads/writes. Reveal accepts receipt IDs. External links use exact documentation hosts
  or pending provider URLs; there is no generic webview process/path launch command.
- `build.rs`, the handler list and `capabilities/main.json` enroll product commands only
  for `main`; no wildcard preview window or plugin filesystem/shell/opener grant is exposed.
  Existing IPC tests drive the real command argument shapes and ACL. No changes were
  needed to `build.rs` or the capability file.
- SQLCipher rejects wrong keys; DB/WAL files have no plaintext marker. Temporary SQL
  storage is memory-only, foreign keys are enabled, versions are immutable, money uses
  integer microdollars, and unknown-cost reservations survive terminal build failures.
- Broker tests cover the exact plan hash, authenticated staging, reviewed-byte commit
  without rerunning, input/destination swaps, grant revocation, no-overwrite publication,
  and crash reconciliation without retrying an uncertain effect.
- CSV UTF-8 BOM, CRLF, RFC quoting and quoted newlines work through the CSV parser;
  `quoted_newlines_and_crlf_are_csv_records` and the header regression pin these cases.
- Current DTO tags, camelCase fields, nullable fields and flattened tool summaries match
  `backend/types.ts` in the existing DTO/IPC suites. New stored `draftRevision` metadata
  is not added to the public DTO.
- Candidate/build packets contain synthetic rows, not selected row values. Existing
  canary tests cover credential argv/environment/output redaction and token/key storage.
  Connection diagnostics use content-free typed errors rather than provider bodies.
- OAuth checks state before accepting denial, rejects duplicate state/code parameters,
  keeps waiting after favicon/wrong-state requests, does not reflect queries into HTML,
  and closes callback tasks with pending handles. Added denial/stray-request regression
  `callback_ignores_stray_requests_and_returns_matching_denial` passes.
- Connection Cancel already called `disconnect(connectorId)` and inline notices already
  displayed `BackendError.message`; the added UI regression verifies both paths.
- Tools already loaded `connectionSuggestions()` and displayed banners on initial load;
  `Tools loads and displays persisted connection suggestions` verifies this behavior.
- Show ignores forged session IDs and unknown connectors, keeps titles in the observer,
  stores only connector detections/counts, and suppresses dismissed/connected suggestions.
- Vault operations remain under the Macroscope service, non-synchronizing, with
  `AccessibleWhenUnlockedThisDeviceOnly` for protected items. New vault tests inject
  operations: they do not access the Keychain. No Muse authentication mutation or
  personal Muse configuration reads were performed.

Limits of this review: signed-bundle Keychain/TCC behavior, an interactive real provider
sign-in, real model billing, and Muse file-read confinement remain unverified, as recorded
in `PROGRESS.md`. The checkbox is explicit owner consent, not evidence of confinement.
Logical SQL retention is not a claim of cryptographic selective erasure or backup removal.

## Verification

Final runs: all four required verification commands passed with exit code 0.
The workspace run passed **176 Rust tests**, with **4 explicitly ignored** integration
checks; the requested observer check was then run separately and passed. The check
script passed formatting, clippy with warnings denied, those Rust tests, TypeScript,
**56 UI tests across 8 files**, and the Vite production build.

The plain observer build `cd native/macos-observer && swift build -c release` failed
inside this sandbox (exit 1):

```text
sandbox-exec: sandbox_apply: Operation not permitted
```

The same observer built successfully without SwiftPM's nested sandbox, keeping its
caches in the workspace (exit 0):

```sh
cd native/macos-observer
CLANG_MODULE_CACHE_PATH="$PWD/.build/module-cache" swift build -c release --disable-sandbox --cache-path .build/cache --config-path .build/config --security-path .build/security
```

```text
[6/7] Linking macroscope-observer
Build complete! (9.32s)
```

That built observer was used for the passing real integration test below. No permission
escalation was needed. Signed/TCC attribution remains a separate, unverified gate.

### `cargo test --workspace`

**PASS — exit 0.** Exact output tail:

```text
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests macroscope_vault

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

```

### `MACROSCOPE_OBSERVER_IT=1 cargo test -p macroscope-app-core --test show -- --ignored`

**PASS — exit 0.** Exact output tail:

```text
    Finished `test` profile [unoptimized + debuginfo] target(s) in 6.73s
     Running tests/show.rs (/Volumes/250GB SAMSUNG 970 EVO PLUS /dev-tools/macroscope-cache/targets/06-review-hardening/debug/deps/show-b912d9bb3d6e4c6f)

running 1 test
test real_swift_observer_session_starts_and_stops ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 6 filtered out; finished in 3.02s

```

### `bash scripts/check.sh`

**PASS — exit 0.** Exact output tail:

```text
✓ 31 modules transformed.
rendering chunks...
computing gzip size...
dist/index.html                   0.44 kB │ gzip:  0.28 kB
dist/assets/index-hv0yZwWy.css    6.49 kB │ gzip:  2.09 kB
dist/assets/index-Nt7OgBY5.js   298.78 kB │ gzip: 90.54 kB

✓ built in 96ms
```

### `cd apps/desktop && npx tauri build --debug --no-bundle`

**PASS — exit 0.** Exact output tail:

```text
   Compiling tauri-plugin-dialog v2.8.3
   Compiling libsqlite3-sys v0.39.1
   Compiling rusqlite v0.41.0
   Compiling macroscope-storage v0.1.0 (/Users/todd/StudioProjects/macroscope-wt/hardening/crates/storage)
   Compiling macroscope-app-core v0.1.0 (/Users/todd/StudioProjects/macroscope-wt/hardening/crates/app-core)
   Compiling macroscope-desktop v0.1.0 (/Users/todd/StudioProjects/macroscope-wt/hardening/apps/desktop/src-tauri)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 44.00s
       Built application at: /Volumes/250GB SAMSUNG 970 EVO PLUS /dev-tools/macroscope-cache/targets/06-review-hardening/debug/macroscope-desktop
```

The final `scripts/check.sh` output also includes:

```text
 Test Files  8 passed (8)
      Tests  56 passed (56)
```

`git diff --check` passed. All edits remain in the working tree.
