# ADR 0002: Ship the finite CSV mapping; run only the native Muse binary (2026-10-27)

Status: accepted, from M0 evidence.

## 1. Generated-tool runtime: finite CSV mapping, not generated TypeScript

Plan §7.3 makes generated TypeScript conditional on a demonstrated runtime boundary and names
the fallback: a finite host-owned CSV mapping with no expressions.

Evidence (`docs/build/runtime-lock.md`): pinned Deno 2.9.8 under a deny-by-default Seatbelt
profile denied every file, network, environment, subprocess, FFI, and storage canary at the OS
layer. Two results still fail the plan's gate:

- `data:` URL imports and `data:` Worker bootstraps execute code that never went through
  module-graph validation (contained by the OS boundary, but outside the admitted graph).
- Process-tree RSS polling let a fast allocator overshoot the 256 MiB limit by ~400–750 MB
  before the kill. The plan rejects polling that permits unbounded growth.

Decision: the private MVP ships the `csv-mapping` engine (`crates/executor`). Model output is a
`CsvMapping` JSON object validated by the host; no generated code executes. The Deno experiment
stays in `experiments/runtime-gate/` for a later attempt with an import-graph validator that
rejects `data:`/`blob:`/Worker construction and an OS-enforced memory ceiling.

## 2. Muse: execute only the resolved native binary, gate schema drift by conformance

Evidence (`docs/build/m0-evidence.md`): running the `~/.local/bin/muse` bash launcher during this
build auto-updated Muse from 1.4.11 to 1.4.13 and deleted the 1.4.11 binary. The MSP schema
fingerprint changed (additive v1 changes, including computer-use approval types).

Decision:

- Macroscope never executes the launcher for builds or health checks. It resolves the newest
  `muse-bin-*` native binary, hashes it, and re-checks the hash before each job.
- An unknown fingerprint with `schemaVersion: 1` runs an echo-provider conformance check
  (handshake, session, turn, interrupt, unknown method, shutdown) and records the result per
  binary hash. A schema major-version change blocks.
- Developer tooling in this repository follows the same rule (`.dev/muse_help.sh` uses the
  native binary).
