# Trusted CSV effect broker

The broker implements one selected input and exactly one create-only CSV output
per run. This is trusted host code for the finite `csv-mapping` engine, without
an embedded runtime, expressions, scripts, network operations, or callbacks.

1. The native host constructs `SelectedFile` and `SelectedRoot` from picker
   results, supplies the admitted tool/version, contract, mapping, CSV limits,
   grant revision, policy epoch, staging parent and a 32-byte encryption key.
   These constructors validate selections; their names alone do not confer a
   grant. Model output and renderer IPC must never call these as authority.
2. `stage(StageRequest)` validates the candidate against the contract, checks all
   required input columns, reads the source through a no-follow regular-file
   handle, and records device/inode, size, precise mtime and SHA-256. It executes
   the mapping once and encrypts output using XChaCha20-Poly1305 with a fresh
   random 24-byte nonce. The plan hash is authenticated associated data. Each
   run has a private random staging directory (0700 on Unix); the ciphertext
   becomes read-only and is synced. There is no plaintext staging fallback.
3. `StagedPlan::plan()`, `plan_hash()` and `preview()` support host-owned review.
   Preview decrypts and verifies the very same staged bytes; cells and filenames
   must be rendered as escaped text, never interpreted as HTML. The plan uses
   recursively sorted JSON keys, compact encoding and UTF-8, with ordered
   effects. Its SHA-256 binds the run, tool, immutable version, engine, mapping,
   input snapshot, destination parent identity and basename, output length and
   digest, CSV budgets, grant revision and policy epoch.
4. `approve(&mut staged, &hash, Approval { plan_hash, approved_at_ms })` records
   deliberate approval of that exact digest. A different hash, duplicate approval
   or commit without recorded approval fails. Approval is separate from artifact
   admission and is required for each new run.
5. `commit(staged, approval, current_epochs)` consumes the run. It authenticates
   and rehashes ciphertext, reopens and verifies the source, verifies the
   destination root and current revisions, and refuses an existing output name.
   `StagedPlan` retains no mapping, so commit cannot rerun the interpreter. It
   writes approved bytes to a 0600 temporary file in the destination, syncs it,
   durably journals intent, and publishes atomically without overwrite. macOS
   uses `renameatx_np(RENAME_EXCL)`, the directory-FD form of `renamex_np`; Linux
   uses `renameat2(RENAME_NOREPLACE)`. Both pin operations to the checked parent
   handle. Windows uses `MoveFileExW` without replacement or copy fallback, with
   a parent handle excluding delete-sharing. Unsupported atomic no-replace
   semantics cause refusal, never a check-then-rename fallback. The directory
   is synced, published bytes are hashed again, and a serializable receipt is
   journaled with input/approved/committed digests and timestamps.
6. The private run directory contains an append-only, synced JSON-lines journal:
   `staged`, `approved`, `intent`, `published`, `receipt`. Errors after publication
   return `OutcomeUnknown`; callers must reconcile and must not retry the write.
   `reconcile(run_dir)` checks the original parent identity and approved output
   length/digest. A matching destination yields `ReconciledPublished` and records
   publication if necessary. A conflicting file, swapped root, or missing file
   after recorded publication yields `OutcomeUnknown`. An intent with no output
   yields `NotPublished`, allowing a fresh stage and review. Reconciliation never
   creates, replaces, deletes, or automatically retries a destination.

The interpreter defaults to 10,000,000 input bytes, 1,000,000 data rows and
20,000,000 output bytes. `executor::Limits` describes these CSV budgets; the
existing domain manifest `Limits` continues to describe process/runtime budgets.
The caller must derive authorized CSV limits from the admitted manifest/grant.
Money is processed as decimal strings with half-away-from-zero rounding, strict
thousands grouping and USD/EUR/GBP symbols. Two-digit years mean 2000–2099;
`iso8601` denotes a date-only `YYYY-MM-DD`. Data rows preserve order and count;
errors return no partial output. Formula escaping covers output cells (including
labels), with valid signed decimal numbers exempt. The fixed domain permission
ceiling admits pure transforms only; it has no per-transform allowlist.

## Boundaries and non-guarantees

- The host remains responsible for tool admission, immutable version verification,
  native picker provenance, renderer isolation, grant storage and scheduling
  (one active run per tool, two globally). This crate is not a renderer IPC API.
- Approval records are not signatures. The caller supplies keys; vault integration,
  durable authenticated approvals, key rotation and deletion/retention are later
  work. Owned key storage is zeroized on drop; this is not a memory-dump defense.
- Windows staging requires a host-owned parent with a private ACL. Windows code
  is platform-gated; runtime adversarial tests are currently exercised on macOS.
  Linux/Windows need their own filesystem and durability validation before ship.
- No user-mode broker protects against same-user malware, administrators, or a
  hostile filesystem. Read-only files are not immutable to their owner. Metadata
  journals expose selected paths and digests within the private staging directory;
  they are neither encrypted nor signed and must not become public logs.
- Rechecking source/root identity reduces stale approvals, and pinned directory
  handles prevent publication into a substituted root. This does not lock out all
  concurrent source writes or parent renames, or guarantee a receipt path remains
  valid after publication. Epoch arguments must come from the host's authoritative
  state, serialized against revocation through the effect dispatch boundary.
- Commit necessarily uses a plaintext publication temporary file in the selected
  destination. A crash before rename can leave that private temporary file there.
  Cleanup and retention belong to the host and must not blindly retry publication.
- Fsync and atomicity depend on the OS and filesystem honoring their contracts;
  filesystems reporting unsupported operations are refused. Power-loss recovery
  does not establish who created an identical-content file: reconciliation proves
  matching bytes and parent identity, not authorship. Uncertain or conflicting
  outcomes require user review. There is no overwrite, rollback or general undo.

## Approved connection actions (runtime v2)

`stage_generated_actions` freezes up to five data-only calls alongside at most one
file create. Each call binds the connector, exact tool name and JSON arguments,
server URL and labelled annotation claims into the canonical plan hash. The
sandbox has no token or server transport. App-core validates calls against the
explicitly selected tool schemas and the version's manifest and grant.

`Connections` preflights every connector with stored authorization before any
call. `commit_with_actions` accepts only a trusted host executor, checks the
frozen digest and current file/grant preconditions, syncs an indexed intent,
executes each call once in order, and syncs its status. It stops at the first
failure or uncertain result. Generated code has exited before review and is
never called during commit. Action-only runs publish no file.

Private call arguments are stored in authenticated encrypted staging; journal
entries contain only indices, hashes and statuses. Server result summaries in
SQLCipher receipts are capped at 2,000 characters and must render as inert text.
An intent without a result reconciles to `OutcomeUnknown`; recovery reads the
encrypted call list to create a receipt, marks later unsent calls `NotRun`, and
never sends requests. Uncertain effects have no automatic retry or undo.
