# Task 17 verification — 2026-10-28

Final verification used the preset `CARGO_TARGET_DIR`, npm, and the existing toolchain. All commands below exited **0**. Changes remain in the working tree; no commit, push, or branch change was performed.

| Command | Result | Exact counts |
|---|---|---|
| `cargo build -p macroscope-runner --bins` | PASS (exit 0) | 2 binaries |
| `bash scripts/check.sh` (check-1) | PASS (exit 0) | 431 Rust passed (301 workspace + 130 synthetic core), 0 failed; 126 UI passed across 17 files |
| `bash scripts/check.sh` (check-2) | PASS (exit 0) | 431 Rust passed (301 workspace + 130 synthetic core), 0 failed; 126 UI passed across 17 files |
| `bash experiments/runtime-gate-v2/run-canaries.sh` | PASS (exit 0) | 62/62 synthetic canaries matched expectations; 0 failures |
| `cd apps/desktop && npx tauri build --debug --no-bundle` | PASS (exit 0) | Debug application built |

Each check includes five pre-existing ignored executions: the observer integration test runs in both core suites; live connector discovery and the two real Muse conformance tests are opt-in. All new tests run, with **0 new skips/ignores**: 11 core integration tests, 5 runner tests, 2 builder tests, and 5 UI/adapter tests. No tests were deleted or weakened.

## Exact output tails

### Runner binaries

```text
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.43s
```

### Check 1

```text
> vite build

vite v8.3.5 building client environment for production...
transforming...
✓ 34 modules transformed.
rendering chunks...
computing gzip size...
dist/index.html                   0.44 kB │ gzip:   0.28 kB
dist/assets/index-tDK369K_.css    6.66 kB │ gzip:   2.15 kB
dist/assets/index-C9nc46z0.js   339.49 kB │ gzip: 101.38 kB

✓ built in 108ms
```

```text
 Test Files  17 passed (17)
      Tests  126 passed (126)
```

### Check 2

```text
> vite build

vite v8.3.5 building client environment for production...
transforming...
✓ 34 modules transformed.
rendering chunks...
computing gzip size...
dist/index.html                   0.44 kB │ gzip:   0.28 kB
dist/assets/index-tDK369K_.css    6.66 kB │ gzip:   2.15 kB
dist/assets/index-C9nc46z0.js   339.49 kB │ gzip: 101.38 kB

✓ built in 103ms
```

```text
 Test Files  17 passed (17)
      Tests  126 passed (126)
```

### Synthetic canaries

```text
worker-read: synthetic=DENIED, OS=UNKNOWN
worker-remote: synthetic=DENIED, OS=UNKNOWN
worker-storage: synthetic=DENIED, OS=UNKNOWN
worker-write: synthetic=DENIED, OS=UNKNOWN
write: synthetic=DENIED, OS=UNKNOWN
orders: synthetic=PASS, OS=UNKNOWN
stdin-flood: synthetic=DENIED, OS=UNKNOWN
Synthetic suite: PASS; admission verdict: UNKNOWN; evidence: docs/build/task17-runtime-canaries.md
```

### Tauri debug build

```text
   Compiling macroscope-storage v0.1.0 (/Volumes/250GB SAMSUNG 970 EVO PLUS /dev-tools/macroscope-wt/model-step/crates/storage)
   Compiling macroscope-app-core v0.1.0 (/Volumes/250GB SAMSUNG 970 EVO PLUS /dev-tools/macroscope-wt/model-step/crates/app-core)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1m 05s
       Built application at: /Volumes/250GB SAMSUNG 970 EVO PLUS /dev-tools/macroscope-cache/targets/wt-model-step/debug/macroscope-desktop
```

## New canaries

```text
model-ask-limit: synthetic=DENIED, OS=UNKNOWN
model-ask-no-network: synthetic=DENIED, OS=UNKNOWN
```

The limit canary returns `model_limit` on the fourth request even when the program catches internal exceptions. The network canary verifies that fetch, WebSocket, XMLHttpRequest, socket, Deno and process APIs are absent. Synthetic evidence does not claim OS admission: the harness reports **admission UNKNOWN**, as expected for the requested synthetic run.

Full synthetic evidence: [runtime canaries](task17-runtime-canaries.md), [JSON receipts](../../experiments/runtime-gate-v2/task17-synthetic-results.json).

Full command logs are retained locally under `.dev/task17/`. `git diff --check` and the byte-for-byte backend type copy comparison pass. The contract copy is `.dev/task17/backend-types.ts`, respecting the work-only-inside-this-worktree instruction.
