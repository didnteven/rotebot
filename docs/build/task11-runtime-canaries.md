# QuickJS runtime admission v2

**Verdict: UNKNOWN. Product policy is recorded in ADR 0003; this synthetic run does not establish OS or release admission.**

Measured 2026-10-27T07:01:24.017799+00:00. Synthetic developer evidence; OS rows are UNKNOWN until an outside rerun with `bash experiments/runtime-gate-v2/run-canaries.sh --os`.

The production launcher fails closed if any setrlimit or sandbox setup fails. Finite RLIMIT_AS availability on this host: **False**. The explicit synthetic path omits Seatbelt and RLIMIT_AS; it is never an automatic fallback.

## Versions and hashes

Runner 0.1.0; rquickjs **0.14.0**, embedding QuickJS-NG **0.16.2**. Cargo.lock pins all crate archives by checksum. [rquickjs source/API](https://docs.rs/rquickjs/0.14.0/rquickjs/).

```json
{
  "pins": {
    "rquickjs": {
      "version": "0.14.0",
      "checksum": "a0a22bf72f515cc4189dd7c63d016f6de92a11eee98ddf7dd64794198f6eb34e"
    },
    "rquickjs-core": {
      "version": "0.14.0",
      "checksum": "6657979a24d543e5fc66f32cef2f47ca82ce8eb55cec5f7d59a12b7b570bbf24"
    },
    "rquickjs-sys": {
      "version": "0.14.0",
      "checksum": "cee271d0eeba64f0915b846cb7ae02e16faf3dfdffdca91731101d9d30fe3423"
    }
  },
  "runner_sha256": "c439555499d74c91e517002a4863be505889ff7e16d16dc8acbe94535bf17984",
  "launcher_sha256": "3213d2aa5e3805193d5a7d38294b473135499f309cc6524d61e508c68a9729ec",
  "profile_sha256": "9d9d615e1d6772666f0cb96a197f7a1a9391ebe66719f8d9b73f60cc7bcb1e3a",
  "cargo_lock_sha256": "7dfaa0961a06bfe4cb638ab64c1dc2400a94d77f44f3d0426a138dc9639393e3",
  "quickjs_header_sha256": "979127138da79cad5ddc7effa1afb13e8b02f744da2aa36844d4cb9d8cb92de9",
  "harness_sha256": "ec9cb681c33ff8fd7bbea9a11516b61530a379556285f9fa119bc0b3fbcd1ffb"
}
```

## Launch

Exact host OS: `macOS-26.5-arm64-arm-64bit-Mach-O`. OS version:
```text
ProductName:		macOS
ProductVersion:		26.5
BuildVersion:		25F71
```

All concrete argv, empty env, source hashes, statuses and kernel usage are in `experiments/runtime-gate-v2/task11-synthetic-results.json`. OS argv:

```json
[
  "/usr/bin/sandbox-exec",
  "-D",
  "RUNNER=/Volumes/250GB SAMSUNG 970 EVO PLUS /dev-tools/macroscope-cache/targets/11-general-tasks/debug/macroscope-runner",
  "-f",
  "/Users/todd/StudioProjects/macroscope-wt/general-tasks/experiments/runtime-gate-v2/profile.sb",
  "/Volumes/250GB SAMSUNG 970 EVO PLUS /dev-tools/macroscope-cache/targets/11-general-tasks/debug/macroscope-runner"
]
```

Environment: `{}`. Cwd: `/`. Inherited descriptors: stdin/stdout/stderr pipes only (CLOEXEC on other host descriptors). Dedicated process group; parent bounds stdout to 20,000,000 bytes and stderr to 65,536 bytes, kills/reaps at wall budget + 1 s. No descendants, module graph, cache or writable scratch are admitted.

pre_exec soft=hard limits (all errors abort launch):

```json
{
  "RLIMIT_AS": 536870912,
  "RLIMIT_CPU": 11,
  "RLIMIT_FSIZE": 0,
  "RLIMIT_NOFILE": 32,
  "RLIMIT_NPROC": 0,
  "RLIMIT_CORE": 0
}
```

QuickJS limits: 64 MiB allocator, 512 KiB stack, 10 s process CPU, 12 s wall; requests can only lower ceilings. Interrupt checks wall and process CPU; parent deadline also covers parser/native code. Stdin is capped at 20,000,000 bytes. Exactly one output, raw text ≤20,000,000 bytes; serialized response including escaping/logs ≤20,000,000 bytes; logs ≤1 MiB including per-entry overhead. SDK CSV host buffers and conversions are bounded by input/output/shape ceilings; the QuickJS heap ceiling alone is not a claim that all host allocations count as JS heap.

## Seatbelt profile

Candidate, not empirically minimized or compatibility-verified here; mach-lookup allowlist is empty. No user-data path, general metadata grant, write, network or process-fork grant.

```scheme
;; Deny-by-default Seatbelt profile for macroscope-runner (QuickJS, no module loader).
;; Minimal set found empirically on macOS 26.5 outside any enclosing sandbox (bisected):
;; the runner aborts without sysctl reads and without reading the root directory entry.
;; Nothing else: no network, no mach-lookup, no writes, no user-data reads, no metadata.
(version 1)
(deny default)
(deny network*)
(deny process-fork)
(allow process-exec (literal (param "RUNNER")))
(allow file-read* (literal (param "RUNNER")))
(allow file-read* (subpath "/usr/lib")
                  (subpath "/System/Library")
                  (subpath "/private/var/db/dyld")
                  (subpath "/System/Volumes/Preboot/Cryptexes/OS/System/Library"))
(allow sysctl-read)
(allow file-read* (literal "/"))
```

## Canaries

A generic crash/error never counts as a denial. API rows require an explicit absent-API marker; limit/import rows require the exact error kind. Benign rows require exact bytes. Symlink imports/output traversal are rejected as source/data without opening the target; no user home, credentials or real symlink targets are read. Config discovery is inapplicable: only source bytes from stdin execute, with no filesystem loader. Worker effect/CPU/memory probes all stop at the absent Worker API.

| Canary | Expected | Synthetic worker | OS boundary | Evidence |
|---|---|---|---|---|
| command | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: Deno absent"], "outputs": []}; peak RSS=3178496 |
| config-import-map | error:syntax | DENIED | UNKNOWN | {"ok": false, "error": {"kind": "syntax", "message": "imports are forbidden"}}; peak RSS=2113536 |
| config | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: Deno absent"], "outputs": []}; peak RSS=3178496 |
| connect | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: Deno absent"], "outputs": []}; peak RSS=3194880 |
| cpu | error:cpu | DENIED | UNKNOWN | {"ok": false, "error": {"kind": "cpu", "message": "execution budget exceeded"}}; peak RSS=3211264 |
| dynamic-blob | error:syntax | DENIED | UNKNOWN | {"ok": false, "error": {"kind": "syntax", "message": "imports are forbidden"}}; peak RSS=2113536 |
| dynamic-data | error:syntax | DENIED | UNKNOWN | {"ok": false, "error": {"kind": "syntax", "message": "imports are forbidden"}}; peak RSS=2113536 |
| dynamic-remote | error:syntax | DENIED | UNKNOWN | {"ok": false, "error": {"kind": "syntax", "message": "imports are forbidden"}}; peak RSS=2113536 |
| env-all | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: process absent"], "outputs": []}; peak RSS=3178496 |
| env-home | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: Deno absent"], "outputs": []}; peak RSS=3178496 |
| eval | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: eval absent"], "outputs": []}; peak RSS=3178496 |
| fetch | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: fetch absent"], "outputs": []}; peak RSS=3178496 |
| ffi | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: Deno absent"], "outputs": []}; peak RSS=3178496 |
| function-constructor | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: constructors absent"], "outputs": []}; peak RSS=3194880 |
| host-huge-date | error:forbidden | DENIED | UNKNOWN | {"ok": false, "error": {"kind": "forbidden", "message": "date argument too large"}}; peak RSS=5308416 |
| host-huge-money | error:forbidden | DENIED | UNKNOWN | {"ok": false, "error": {"kind": "forbidden", "message": "money argument too large"}}; peak RSS=5308416 |
| host-unknown-handle | error:forbidden | DENIED | UNKNOWN | {"ok": false, "error": {"kind": "forbidden", "message": "unknown input handle"}}; peak RSS=3227648 |
| hostile-exception | error:runtime | DENIED | UNKNOWN | {"ok": false, "error": {"kind": "runtime", "message": "JavaScript execution failed: use synchronous SDK code only; eval, Function, Worker, Date, Math.random and OS APIs are unavailable"}}; peak RSS=3211264 |
| memory-objects | error:memory | DENIED | UNKNOWN | {"ok": false, "error": {"kind": "memory", "message": "JavaScript execution failed: use synchronous SDK code only; eval, Function, Worker, Date, Math.random and OS APIs are unavailable"}}; peak RSS=72482816 |
| memory | error:memory | DENIED | UNKNOWN | {"ok": false, "error": {"kind": "memory", "message": "JavaScript execution failed: use synchronous SDK code only; eval, Function, Worker, Date, Math.random and OS APIs are unavailable"}}; peak RSS=69697536 |
| ok | exact fixture | PASS | UNKNOWN | {"ok": true, "log": [], "outputs": [{"name": "ok.csv", "bytes": 9}]}; peak RSS=3211264 |
| output-absolute | error:forbidden | DENIED | UNKNOWN | {"ok": false, "error": {"kind": "forbidden", "message": "output requires a .csv, .tsv, .txt, .md or .json basename"}}; peak RSS=3260416 |
| output-flood | error:output_limit | DENIED | UNKNOWN | {"ok": false, "error": {"kind": "output_limit", "message": "output byte limit exceeded"}}; peak RSS=43319296 |
| output-nul | error:forbidden | DENIED | UNKNOWN | {"ok": false, "error": {"kind": "forbidden", "message": "output requires a .csv, .tsv, .txt, .md or .json basename"}}; peak RSS=3260416 |
| output-repeat | error:output_limit | DENIED | UNKNOWN | {"ok": false, "error": {"kind": "output_limit", "message": "only one output is allowed"}}; peak RSS=3260416 |
| output-symlink | error:forbidden | DENIED | UNKNOWN | {"ok": false, "error": {"kind": "forbidden", "message": "output requires a .csv, .tsv, .txt, .md or .json basename"}}; peak RSS=3260416 |
| output-traversal | error:forbidden | DENIED | UNKNOWN | {"ok": false, "error": {"kind": "forbidden", "message": "output requires a .csv, .tsv, .txt, .md or .json basename"}}; peak RSS=3260416 |
| prototype-pollution | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: frozen SDK"], "outputs": []}; peak RSS=3227648 |
| read-home | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: Deno absent"], "outputs": []}; peak RSS=3178496 |
| read-hosts | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: Deno absent"], "outputs": []}; peak RSS=3178496 |
| static-hosts | error:syntax | DENIED | UNKNOWN | {"ok": false, "error": {"kind": "syntax", "message": "imports are forbidden"}}; peak RSS=2113536 |
| static-private | error:syntax | DENIED | UNKNOWN | {"ok": false, "error": {"kind": "syntax", "message": "imports are forbidden"}}; peak RSS=2113536 |
| static-remote | error:syntax | DENIED | UNKNOWN | {"ok": false, "error": {"kind": "syntax", "message": "imports are forbidden"}}; peak RSS=2113536 |
| stderr | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: console absent"], "outputs": []}; peak RSS=3194880 |
| stdout | error:output_limit | DENIED | UNKNOWN | {"ok": false, "error": {"kind": "output_limit", "message": "log byte limit exceeded"}}; peak RSS=4325376 |
| storage-cache | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: caches absent"], "outputs": []}; peak RSS=3178496 |
| storage-indexeddb | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: indexedDB absent"], "outputs": []}; peak RSS=3162112 |
| storage-local | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: localStorage absent"], "outputs": []}; peak RSS=3194880 |
| symlink-hosts | error:syntax | DENIED | UNKNOWN | {"ok": false, "error": {"kind": "syntax", "message": "imports are forbidden"}}; peak RSS=2113536 |
| symlink-private | error:syntax | DENIED | UNKNOWN | {"ok": false, "error": {"kind": "syntax", "message": "imports are forbidden"}}; peak RSS=2113536 |
| wall | error:wall | DENIED | UNKNOWN | {"ok": false, "error": {"kind": "wall", "message": "execution budget exceeded"}}; peak RSS=3211264 |
| worker-command | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: Worker absent"], "outputs": []}; peak RSS=3178496 |
| worker-connect | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: Worker absent"], "outputs": []}; peak RSS=3178496 |
| worker-cpu | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: Worker absent"], "outputs": []}; peak RSS=3162112 |
| worker-data | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: Worker absent"], "outputs": []}; peak RSS=3178496 |
| worker-env | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: Worker absent"], "outputs": []}; peak RSS=3178496 |
| worker-fetch | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: Worker absent"], "outputs": []}; peak RSS=3194880 |
| worker-ffi | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: Worker absent"], "outputs": []}; peak RSS=3178496 |
| worker-local | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: Worker absent"], "outputs": []}; peak RSS=3178496 |
| worker-memory | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: Worker absent"], "outputs": []}; peak RSS=3178496 |
| worker-read | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: Worker absent"], "outputs": []}; peak RSS=3194880 |
| worker-remote | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: Worker absent"], "outputs": []}; peak RSS=3162112 |
| worker-storage | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: Worker absent"], "outputs": []}; peak RSS=3178496 |
| worker-write | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: Worker absent"], "outputs": []}; peak RSS=3178496 |
| write | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: Deno absent"], "outputs": []}; peak RSS=3178496 |
| orders | exact fixture | PASS | UNKNOWN | {"ok": true, "log": ["rows: 18"], "outputs": [{"name": "orders_ready.csv", "bytes": 481}]}; peak RSS=3424256 |
| stdin-flood | error:forbidden | DENIED | UNKNOWN | {"ok": false, "error": {"kind": "forbidden", "message": "stdin request exceeds 20 MB or cannot be read"}} |

## Memory peak evidence

Kernel `getrusage(RUSAGE_CHILDREN).ru_maxrss`, bytes on macOS; no RSS polling. Each launcher has exactly one child. Memory canaries touch allocations so demand-zero pages do not hide resident growth. Unit tests independently use wait4 rusage. Admission threshold for these probes: 64 MiB + 16 MiB fixed slack.

| Canary / layer | Peak RSS bytes | Above 64 MiB | Slack bytes |
|---|---:|---:|---:|
| memory-objects / synthetic | 72482816 | 5373952 | 16777216 |
| memory / synthetic | 69697536 | 2588672 | 16777216 |

## Signatures, remaining gate requirements and verdict

```json
[
  {
    "argv": [
      "/usr/bin/codesign",
      "-dvv",
      "--entitlements",
      ":-",
      "/Volumes/250GB SAMSUNG 970 EVO PLUS /dev-tools/macroscope-cache/targets/11-general-tasks/debug/macroscope-runner"
    ],
    "exit": 0,
    "stdout": "",
    "stderr": "Executable=/Volumes/250GB SAMSUNG 970 EVO PLUS /dev-tools/macroscope-cache/targets/11-general-tasks/debug/macroscope-runner\nIdentifier=macroscope_runner-f1357465c88ea4c7\nFormat=Mach-O thin (arm64)\nCodeDirectory v=20400 size=28027 flags=0x20002(adhoc,linker-signed) hashes=872+0 location=embedded\nSignature=adhoc\nInfo.plist=not bound\nTeamIdentifier=not set\nSealed Resources=none\nInternal requirements=none\nwarning: Specifying ':' in the path is deprecated and will not work in a future release\n"
  },
  {
    "argv": [
      "/usr/bin/codesign",
      "-dvv",
      "--entitlements",
      ":-",
      "/Volumes/250GB SAMSUNG 970 EVO PLUS /dev-tools/macroscope-cache/targets/11-general-tasks/debug/macroscope-runner-launch"
    ],
    "exit": 0,
    "stdout": "",
    "stderr": "Executable=/Volumes/250GB SAMSUNG 970 EVO PLUS /dev-tools/macroscope-cache/targets/11-general-tasks/debug/macroscope-runner-launch\nIdentifier=macroscope_runner_launch-b592dfc59b9a4ead\nFormat=Mach-O thin (arm64)\nCodeDirectory v=20400 size=14210 flags=0x20002(adhoc,linker-signed) hashes=440+0 location=embedded\nSignature=adhoc\nInfo.plist=not bound\nTeamIdentifier=not set\nSealed Resources=none\nInternal requirements=none\nwarning: Specifying ':' in the path is deprecated and will not work in a future release\n"
  }
]
```

PASS requires every benign fixture and canary under a working OS boundary, all resource ceilings supported, and plan §7.3/P8 signed immutable launcher/app-bundle evidence. This developer spike has no trusted release signature or signed-bundle test. Even a successful `--os` canary run cannot assert full release admission without those receipts. UNKNOWN is a failed admission, not permission to enable generated execution. Windows is unverified. ADR 0003 enables the product program path at the previously admitted developer boundary; this run supplies synthetic regression evidence only.

Source admission conservatively rejects the byte sequence `import` (including comments/strings); no module loader is installed. eval/Function globals and function/async/generator constructor links are removed. Promise intrinsics are required for safe async teardown, but the Promise global is absent, returned promises are refused, and no job queue is drained. SDK objects are recursively frozen and have null prototypes. Rust date/money/formula handling is shared with the finite executor.
