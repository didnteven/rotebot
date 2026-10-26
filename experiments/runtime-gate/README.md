# Generated-runtime admission experiment

Synthetic macOS arm64 evidence only. Generated TypeScript stays disabled; this
is not a shipping runner or a substitute for PLAN §7.3/P8 admission.

From the repository root, with Python 3, curl and the system macOS tools:

```sh
bash experiments/runtime-gate/fetch-deno.sh && bash experiments/runtime-gate/run-canaries.sh
```

Fetch verifies the pinned official ZIP and extracted executable against the
published SHA-256 files and the checked-in lock. It refuses a corrupted cache.
No install, authentication, signing, cargo or npm commands are needed.

The canary harness writes `results.json` and `../../docs/build/runtime-lock.md`.
It exits 1 when admission fails, including UNKNOWN Seatbelt rows. If the exact
profile cannot launch the benign control, it records the rejection and runs
explicit synthetic Deno-only probes. That fallback never grants permissions.
`run-sandboxed.sh <package-dir> <entry.ts>` always requires Seatbelt and never
downgrades. Packages must be below this experiment directory. Example:

```sh
printf '{"value":21}\n' | bash experiments/runtime-gate/run-sandboxed.sh experiments/runtime-gate/canaries ok.ts
```

Each run copies/hashes the synthetic package, substitutes fixture paths and
worker-source templates, freezes mode bits, sets an empty three-variable env,
and activates the profile before loading the entry. Malicious symlinks are
preserved intentionally so the loader is tested. The host never imports TS.
The raw source templates are intended for the harness, not independent runs.

The external watchdog uses libproc to enumerate the process group and recursive
children, polling RSS/CPU and `du` scratch usage at a 100 ms target. Mach CPU
ticks are converted using the host's timebase. `wait4` supplies kernel leader
peak RSS/CPU after kill/reap. Output capture is hard capped; memory/disk are
polling only and can overshoot. Therefore this implementation cannot admit
aggregate limits, even on a host where every functional denial passes.

`.runtime/` holds the verified binary/ZIP/help. `.runs/` holds disposable copied
packages, scratch, bounded raw captures and detailed per-run receipts. Both are
gitignored. Repeated experiments consume host disk across runs: retention is
manual and outside the per-run budget. Review them before removing them; frozen
packages need their owner-write bits restored for removal. `results.json` holds
compact excerpts (≤200 stderr characters), concrete argv/env, source manifests,
profile/signature evidence and all measured limits; ignored receipts are not
required to interpret that report.

The shell/Python launcher is unsigned. The proposed profile is unvalidated until
rerun outside the enclosing sandbox; it grants no Mach services. No signed app
bundle or Windows boundary has been tested. Apple's installed manual marks
`sandbox-exec` DEPRECATED. Select the finite host-owned CSV mapping on failure;
this task does not alter product feature flags or any other repository path.
