# Muse Code review — Task 10

Reviewed on 2026-10-27 from the owner's supplied read-only Muse Code findings
(`muse-spark-1.3`, contributor mode). Seven defects fixed; finding 5 is intentional
local-work behavior, now documented and tested. Each confirmed defect had a failing
regression before its fix. Existing tests were neither removed nor relaxed.

| # | Finding | Verdict and change | Regression test(s) |
|---|---|---|---|
| 1 | OAuth client secret follows a URL edit | **Fixed.** Changed URLs delete `mcp-client:<id>` before publishing the URL; service URL edits also invalidate tokens and pending sign-ins. Stored clients bind the exact resource URL, and begin reads them under the service lock, covering delayed saves and sign-in races. The UI asks “Add your OAuth client ID” again. Legacy unbound client records require re-entry. | `changed_url_deletes_credentials_and_never_sends_the_old_secret`; `stale_server_bound_and_legacy_unbound_clients_require_reentry`; UI: `URL edits ask for an OAuth client ID again` |
| 2 | One broken PRM candidate aborts discovery | **Fixed.** Per-candidate HTTP, JSON, size, and transport errors permit trying later PRM URLs. If none succeeds, retain the error; the existing catalog/origin fallback remains available when all PRMs are absent. Resource and issuer validation still fail closed. | `discovery_tries_root_prm_after_a_broken_path_prm`; `discovery_tries_fallback_after_transport_failure_and_fails_when_all_prm_fail`; existing `mismatched_prm_resource_is_rejected` |
| 3 | Sample restoration writes through a symlink | **Fixed.** On Unix, walk every component with pinned directory descriptors and `NOFOLLOW`; open the leaf without truncation, verify a regular file, then restore through that descriptor. Reads are bounded. Refuse both leaf and ancestor links. | `sample_setup_refuses_file_and_directory_symlinks_without_modifying_targets` |
| 4 | Persistence errors are silently dropped | **Fixed.** Build transitions return errors and persist before publication. Admission fails before dispatch if its intent cannot be saved. Later write/cost failures terminate with an internal failure; if even that failure cannot persist, publish it in memory and log the failed persistence. Preserve the last durable intent and unknown-cost reservation for recovery. Stop/privacy paths propagate errors after cancellation. Best-effort activity failures emit a content-free diagnostic. | `build_admission_propagates_persistence_failure_before_dispatch`; `failed_ready_persistence_never_publishes_ready_and_recovers_as_failed`; `outbound_cost_persistence_failure_fails_the_build_and_keeps_reservation`; `total_transition_write_failure_stops_before_egress_and_restart_recovers_intent` |
| 5 | Local-only does not revoke staged deterministic runs | **Not a defect.** PLAN §2.7 closes model egress while deterministic local tools continue. A model consent change should not revoke an already reviewed CSV create. Name the deliberate fixed `LOCAL_RUN_POLICY_EPOCH` and explain it at both broker calls. Grant revisions are still reread at commit, including pause/restore revocation. The broker retains its general epoch comparison for callers that require it. | `local_only_preserves_staged_local_runs_but_grant_revocation_blocks_commit`; existing `pause_and_restore_never_reauthorize_a_previously_staged_run` |
| 6 | OAuth transient clones are not zeroized | **Fixed.** Authorization-code, PKCE, refresh, revocation and client-secret forms borrow zeroizing `AccessToken` values instead of cloning their inner Strings. Percent-encoded request bytes have a zeroizing owner retained through transport clones, with enough initial capacity to avoid reallocating secret-bearing form bytes. Temporary Basic encoding is zeroizing. Callback reads and decoded secret parameters also zeroize on drop/cancellation. | `encoded_oauth_secrets_are_wiped_on_drop_error_and_cancellation`; existing `preregistered_client_and_refresh_on_expiry`, `full_dynamic_flow_json_and_sse` |
| 7 | Core and broker basename checks differ | **Fixed.** Both use the broker's shared validator, including controls and platform restrictions. The original control-character example already failed with the correct invalid-basename category at the broker; the observable inconsistency was an existing invalid name returning a destination-conflict error first. | `invalid_control_names_are_rejected_before_destination_checks` |
| 8 | Show start sends a fixed policy epoch | **Fixed.** The controller supplies its core's current epoch to the start-message constructor. | `observer_start_carries_the_current_policy_epoch_after_privacy_changes`; `start_message_carries_catalog_signatures_and_the_cap` |

## Reproduction evidence

Before fixes, focused test runs exited 101 with these assertions/errors:

- #1: `old secret reached changed server` — the new fake token endpoint captured
  a `client_secret`. After the fix it receives a dynamic-registration token request
  without `client_secret`; the old stored token and client are deleted on URL change.
- #2: `Mcp(HTTP request failed (500))` despite valid root PRM metadata. The test
  also covers malformed JSON and a body larger than 1 MiB.
- #3: `sample setup overwrote a symlink target` — the target's original bytes were
  replaced by the sample CSV.
- #4: `admission ignored failed intent write`, `unpersisted Ready was published`,
  and `cost failure was silently ignored`. These use real SQLCipher connections
  and SQLite failure triggers, not production failure-injection hooks.
- #6: `encoded OAuth body dropped without zeroization`. The fixed test observes
  the body owner's destruction and asserts its bytes are zero before release,
  also checking transport error and cancellation paths.
- #7: `DestinationExists` for a control-character basename that should be
  `InputInvalid` before checking the destination.
- #8: observer start carried `Number(1)` after two privacy changes; expected 3.

The #5 test passed before any production edit, consistent with the local-work
policy decision. The UI copy regression failed before the requested wording change
and passed afterward.

## Preserved guarantees and limits

The existing host-rule checks, synthetic-only outbound packets, exact-plan approval,
staged-byte commit, grant revocation, no-overwrite publication, authenticated staging,
journal recovery, cancellation acknowledgment, reservation accounting, renderer
safety and credential redaction remain covered by their existing suites described in
[codex-review-of-core.md](codex-review-of-core.md).

Secret lifetime tests establish application-owned buffer zeroization; they do not
claim that TLS/socket buffers or reqwest's Basic header storage can be scrubbed by
this code, or establish a forensic whole-process-memory guarantee. No personal
Muse configuration or Keychain was read; tests use memory vaults and fake servers.
No Muse sign-in/account mutation, branch change, commit or push was performed.

## Final verification

All three final requested runs passed with exit 0. Each passed **199 Rust tests**
with **4 existing integration tests ignored** (real Swift observer, live MCP
discovery, and two real Muse echo checks). Both script runs also passed formatting,
clippy with warnings denied, TypeScript typechecking, **104 UI tests across 11
files**, and the Vite production build. Node was v24.10.0; npm was 11.6.0;
`CARGO_TARGET_DIR` was left unchanged.

Preliminary verification also exposed intermittent failures: the first workspace
run exited 101 with four Show tests failing while fake observers produced no
startup events within their waits; a focused Show rerun exited 101 with three
SQLCipher `PRAGMA key or rekey` initialization errors. The subsequent full
workspace run passed without any intervening code or assertion change. These
failures' root cause is not established; no waits were extended, tests serialized,
or assertions relaxed to obtain the passing result. The final runs below use the
commands as requested, with their normal test concurrency.



### `cargo test --workspace`

**PASS — exit 0.** Exact output tail:

```text
running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests macroscope_vault

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

```

### `bash scripts/check.sh — run 1`

**PASS — exit 0.** Exact output tail:

```text
vite v8.3.5 building client environment for production...
transforming...
✓ 31 modules transformed.
rendering chunks...
computing gzip size...
dist/index.html                   0.44 kB │ gzip:  0.28 kB
dist/assets/index-tDK369K_.css    6.66 kB │ gzip:  2.15 kB
dist/assets/index-CGVTFSVK.js   306.63 kB │ gzip: 92.45 kB

✓ built in 790ms
```

### `bash scripts/check.sh — run 2`

**PASS — exit 0.** Exact output tail:

```text
vite v8.3.5 building client environment for production...
transforming...
✓ 31 modules transformed.
rendering chunks...
computing gzip size...
dist/index.html                   0.44 kB │ gzip:  0.28 kB
dist/assets/index-tDK369K_.css    6.66 kB │ gzip:  2.15 kB
dist/assets/index-CGVTFSVK.js   306.63 kB │ gzip: 92.45 kB

✓ built in 85ms
```

Both check-script logs include:

```text
 Test Files  11 passed (11)
      Tests  104 passed (104)
```

The initial workspace failure's exact output tail was:

```text
failures:
    concurrent_start_spawns_only_one_observer
    deleting_history_stops_show_and_clears_session_detections
    observer_start_carries_the_current_policy_epoch_after_privacy_changes
    recording_gmail_produces_a_connect_suggestion_and_counts_only

test result: FAILED. 4 passed; 4 failed; 1 ignored; 0 measured; 0 filtered out; finished in 5.80s

error: test failed, to rerun pass `-p macroscope-app-core --test show`
```

`git diff --check` passed. All changes remain in the working tree. Full command logs,
including the failing reproductions, are retained locally under the ignored
`.dev/muse-review/` directory.
