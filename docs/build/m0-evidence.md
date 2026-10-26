# M0 model-access evidence — 2026-10-27

Task 03 implementation is confined to the five owned crates, their dependencies in
`Cargo.lock`, and this evidence directory. No authentication mutation, account RPC,
real-key inference, direct credential-store inspection, commit, push, or branch change was run.
Tests use fake services, fabricated canaries, recorded transcripts, and the echo provider.

| Gate | Status | Evidence |
| --- | --- | --- |
| Secret never in argv/logs (canary tests) | passed | `memory_secret_lifecycle_and_redaction`; `canary_parent_env_never_reaches_child_and_key_never_argv`; `exec_canary_is_stdin_only_env_is_allowlisted_outputs_scrubbed_and_help_cached`; `error_classification_and_secret_never_in_errors_or_debug`; `fake_msp_handshake_correlates_turn_and_reaps_without_credential_disclosure` |
| App-owned credential namespace | passed | `rejects_muse_service_and_bad_refs`; macOS implementation compiled with service `dev.macroscope.app`, device-only unlocked access control, and synchronization disabled. No runtime Keychain operation was performed. |
| Responses `text.format` request shape | passed | `responses_text_format_shape_store_false_and_usage`: exact schema format, `strict:true`, `store:false`, system/user messages, and no top-level `response_format` |
| Error classification | passed | `error_classification_and_secret_never_in_errors_or_debug`; `incomplete_refusal_malformed_json_and_schema_mismatch`; `network_failure_is_transient_and_success_cannot_echo_key_in_debug` |
| Integer microdollar pricing | passed | `integer_microdollar_prices_no_reasoning_double_count_and_round_up`; version `meta-standard-2026-10-27`. Input includes its cached subset; output includes reasoning by default, with an explicit alternative flag. |
| Egress consent epochs and acknowledgments | passed | `admission_closes_stale_dispatch_refused_barrier_waits_for_all_acks`; `failed_or_lost_ack_keeps_stopping`; `failure_before_revocation_and_dropped_guard_still_block_barrier`; `epoch_never_wraps` |
| Synthetic mapping packet and independent host validation | passed | `direct_meta_fixture_synthetic_packet_and_host_validation`; `offline_is_labelled_and_validator_can_reject`. `SyntheticSamples` accepts no caller-provided CSV rows; the executor is connected through `CandidateValidator`, without a dependency on its crate. |
| One correction retry and budget admission | passed | `malformed_then_corrected_once_and_unknown_spend_stays_reserved`; `malformed_twice_fails_no_third_dispatch`; `budget_exhaustion_and_stale_epoch_prevent_dispatch` |
| Credential route selection and rejected-key/no-fallback behavior | passed | `route_selection_prioritizes_key_then_plan_and_types_no_access`; `rejected_api_key_revokes_admission_and_never_falls_back_to_plan` |
| MSP echo handshake on installed native Muse | passed (conformance) | Muse self-updated to 1.4.13-R6996.1 during the build; its fingerprint `sha256:7fddaf64…0e5b` differs from the plan pin. The fingerprint gate blocked it; the echo conformance check (initialize, model/list, session/start, turn to completed, interrupt, unknown-method error, clean shutdown) passed and admitted it as `Ready{verified: Conformance}`. `real_msp_echo_handshake_turn_and_clean_shutdown` passes. Schema vendored at `contracts/msp/1.4.13-R6996.1/`. |
| Exec lane JSONL on installed native Muse | passed | `real_exec_echo_jsonl_reaches_terminal`: reaches `run.terminal.completed` with text `echo: hello …`. Found: 1.4.13 rejects `--output-schema` with `--provider echo`; the echo path now omits it. |
| Fake MSP handshake, acknowledged turn correlation, shutdown | passed | `fake_msp_handshake_correlates_turn_and_reaps_without_credential_disclosure`; `process_deadline_kills_and_reaps_group`; `async_framing_rejects_oversize_and_partial_frames` |
| Transcript replay | passed | `transcript_replay_echo_auth_profile_and_negative_framing` (01, 02, 15, 21); `transcript_replay_orphan_reconciled_and_stale_terminal_ignored` (14 before/after); `replay_exec_empty_and_badkey_terminals` (05, 07); `revisions_deltas_unknowns_and_foreground_are_not_terminal`; `quota_branches_on_typed_detail_and_preserves_reset_instant` |
| Plan sign-in detection | passed | `plan_sign_in_detection`: captured not-signed-in stderr, exit-0 JSON allowance summary, invalid output, and scrubbed unknown diagnostics. This validates the parser; it does not claim a live sign-in check was performed. |
| Real-key smoke | unknown | UNKNOWN: no key yet. No paid request or billing-attribution check was run. |
| Serve permission profile with real key | unknown | UNKNOWN: no live real-key session was attempted. Probe 21's recorded session-start rejection is classified as `PermissionProfileUnavailable`; it is not successful inference evidence. |
| Containment gate | unknown | UNKNOWN: no native Muse file/tool/plugin/reminder/egress containment experiment was run. Echo and process cleanup do not establish containment. Mapping-builder Muse routes require host-supplied containment verification. |
| Native vault runtime behavior | unknown | UNKNOWN: native Keychain implementation compiled on macOS; credential reads/writes were deliberately not exercised. Windows Credential Manager is behind `cfg(windows)` and was not cross-compiled or run here. |

The installed launcher is a bash script. Resolution selects the native sibling
`/Users/todd/.local/bin/muse-bin-1.4.13-R6996.1` directly. The pinned MSP fingerprint
remains `sha256:f282ad16e21520d0cda68cb190149b86e2b062eef1dd282749360c05aa3d1082`.
The real integration test retains its fingerprint assertion and fails before a turn;
it was not relaxed to accommodate the newer installed binary.

Muse execution remains experimental until its containment and accounting evidence is
supplied. Unknown usage retains reservations. `BuildBudget` is the host's reserved
ceiling for the entire job; errors require the host to retain that reservation until
reconciliation. Muse estimates conservatively reserve repeated model calls, and can
therefore refuse a budget that would suffice for a single direct request. Missing or
rejected API credentials never select the owner's plan as an automatic fallback.

Verification used Node v24.10.0, npm 11.6.0, macOS arm64, and the preset
`CARGO_TARGET_DIR` unchanged. `npm ci` completed successfully before the full check.

| Command | Status | Exact result / output tail |
| --- | --- | --- |
| `cargo test -p macroscope-vault -p macroscope-policy -p macroscope-model-gateway -p macroscope-muse-client -p macroscope-builder` | passed | Exit 0; 33 tests passed, 0 failed, 1 ignored. Final tail: `test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s` (vault doc-tests). |
| `MACROSCOPE_MUSE_IT=1 cargo test -p macroscope-muse-client -- --ignored` | failed | Exit 101; `test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.26s`. |
| `cargo clippy --workspace --all-targets -- -D warnings` | passed | Exit 0; final tail: ``Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.08s``. |
| `cargo fmt --all --check` | passed | Exit 0; no output. |
| `bash scripts/check.sh` | passed | Exit 0; 54 Rust tests passed, 0 failed, 1 ignored; 2 frontend test files and 7 tests passed; desktop build completed. Final tail: `✓ built in 687ms`. |

## Task 03b follow-up (2026-10-27)

Codex implemented the conformance gate, acceptance store, binary pinning between jobs, vendored
schema, and a recorded 1.4.13 echo fixture (`fixtures/msp/echo-1.4.13.jsonl`, local paths
scrubbed) before hitting its usage limit. Claude finished the task: fixed the exec echo flags,
reran `MACROSCOPE_MUSE_IT=1 cargo test -p macroscope-muse-client -- --ignored` (2 passed, real
binary, 16 s), unit tests, clippy, and fmt. The rows above for the 1.4.11 fingerprint failure are
superseded by the two "passed" rows.

Lesson recorded for the app: invoking the `~/.local/bin/muse` launcher can auto-update Muse and
delete the previous native binary. Macroscope only ever executes the resolved native
`muse-bin-*` and re-hashes it per job.
