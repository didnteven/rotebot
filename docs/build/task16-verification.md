# Task 16 verification — m4/schedules

Verified on macOS arm64 with Node v24.10.0 and npm 11.6.0. The preset `CARGO_TARGET_DIR` was preserved. Source changes remain in this worktree; no commits, pushes or branch changes were made.

| Command | Result | Exact counts |
|---|---|---|
| `cargo build -p macroscope-runner --bins` | PASS (exit 0) | Both runner binaries built |
| `bash scripts/check.sh` — run 1 | PASS (exit 0) | Workspace: 283 passed, 0 failed, 4 existing ignored. Synthetic app-core: 119 passed, 0 failed, 1 existing ignored. UI: 16 files, 120 passed, 0 failed. |
| `bash scripts/check.sh` — run 2 | PASS (exit 0) | Workspace: 283 passed, 0 failed, 4 existing ignored. Synthetic app-core: 119 passed, 0 failed, 1 existing ignored. UI: 16 files, 120 passed, 0 failed. |
| `bash experiments/runtime-gate-v2/run-canaries.sh` | PASS (exit 0) | 60/60 synthetic cases matched; 0 failures |
| `cd apps/desktop && npx tauri build --debug --no-bundle` | PASS (exit 0) | Debug desktop executable built; no bundle requested |

Each check executed **402 passing Rust tests** and **120 passing UI tests**. The five ignored executions represent four pre-existing opt-in tests: the Swift observer test appears in both Rust invocations; live MCP discovery and the two live Muse conformance tests appear in the workspace run. No new test has an ignore, skip or early return; no tests were weakened or deleted. The live opt-ins were left untouched.

Synthetic canaries supply no new OS admission evidence (`UNKNOWN`); the existing OS boundary evidence was preserved. The generated synthetic evidence and raw command logs are retained under `.dev/task16/`. The previous task’s synthetic report files were restored after saving this run’s evidence.

All **67** app commands agree between `handlers()` and build `COMMANDS`, and each has a main-window capability permission. The backend type copy at `.dev/task16/backend-types.ts` is byte-identical to the final source contract. `git diff --check` passes.

## Exact output tails

### Runner build

```text
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.30s
```

### Check run 1

```text

 Test Files  16 passed (16)
      Tests  120 passed (120)
   Start at  01:19:02
   Duration  12.80s (tests 78%, environment 17%, setup 3%, import 1%, transform 1%)

    Isolate  16 workers spawned · ~469ms startup each (spawn + environment, per file)
             at least ~1.41s faster with isolate: false — reuses workers across files instead of one per file


> @macroscope/desktop@0.1.0 build
> vite build

vite v8.3.5 building client environment for production...
transforming...
✓ 33 modules transformed.
rendering chunks...
computing gzip size...
dist/index.html                   0.44 kB │ gzip:  0.28 kB
dist/assets/index-tDK369K_.css    6.66 kB │ gzip:  2.15 kB
dist/assets/index-Wg6y-m4W.js   332.29 kB │ gzip: 99.34 kB

✓ built in 108ms
```

### Check run 2

```text

 Test Files  16 passed (16)
      Tests  120 passed (120)
   Start at  01:19:44
   Duration  12.78s (tests 78%, environment 16%, setup 3%, import 1%, transform 1%)

    Isolate  16 workers spawned · ~458ms startup each (spawn + environment, per file)
             at least ~1.37s faster with isolate: false — reuses workers across files instead of one per file


> @macroscope/desktop@0.1.0 build
> vite build

vite v8.3.5 building client environment for production...
transforming...
✓ 33 modules transformed.
rendering chunks...
computing gzip size...
dist/index.html                   0.44 kB │ gzip:  0.28 kB
dist/assets/index-tDK369K_.css    6.66 kB │ gzip:  2.15 kB
dist/assets/index-Wg6y-m4W.js   332.29 kB │ gzip: 99.34 kB

✓ built in 110ms
```

### Synthetic canaries

```text
worker-memory: synthetic=DENIED, OS=UNKNOWN
worker-read: synthetic=DENIED, OS=UNKNOWN
worker-remote: synthetic=DENIED, OS=UNKNOWN
worker-storage: synthetic=DENIED, OS=UNKNOWN
worker-write: synthetic=DENIED, OS=UNKNOWN
write: synthetic=DENIED, OS=UNKNOWN
orders: synthetic=PASS, OS=UNKNOWN
stdin-flood: synthetic=DENIED, OS=UNKNOWN
Synthetic suite: PASS; admission verdict: UNKNOWN; evidence: docs/build/task13-runtime-canaries.md
```

### Tauri debug build

```text
   Compiling tao v0.37.2
   Compiling window-vibrancy v0.8.1
   Compiling rfd v0.16.1
   Compiling tauri-runtime v2.12.2
   Compiling tray-icon v0.25.1
   Compiling macroscope-model-gateway v0.1.0 (/Volumes/250GB SAMSUNG 970 EVO PLUS /dev-tools/macroscope-wt/schedules/crates/model-gateway)
   Compiling macroscope-connections v0.1.0 (/Volumes/250GB SAMSUNG 970 EVO PLUS /dev-tools/macroscope-wt/schedules/crates/connections)
   Compiling macroscope-builder v0.1.0 (/Volumes/250GB SAMSUNG 970 EVO PLUS /dev-tools/macroscope-wt/schedules/crates/builder)
   Compiling tauri-runtime-wry v2.12.3
   Compiling libsqlite3-sys v0.39.1
   Compiling rusqlite v0.41.0
   Compiling macroscope-storage v0.1.0 (/Volumes/250GB SAMSUNG 970 EVO PLUS /dev-tools/macroscope-wt/schedules/crates/storage)
   Compiling macroscope-app-core v0.1.0 (/Volumes/250GB SAMSUNG 970 EVO PLUS /dev-tools/macroscope-wt/schedules/crates/app-core)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1m 05s
       Built application at: /Volumes/250GB SAMSUNG 970 EVO PLUS /dev-tools/macroscope-cache/targets/wt-schedules/debug/macroscope-desktop
```
