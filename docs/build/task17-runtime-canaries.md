# QuickJS runtime admission v2

**Admission verdict: UNKNOWN. Synthetic suite: PASS.**

Measured 2026-10-27T13:01:25.881534+00:00. Synthetic developer evidence; OS rows are UNKNOWN until an outside rerun with `bash experiments/runtime-gate-v2/run-canaries.sh --os`.

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
  "runner_sha256": "393d2174bf9cbe4a691b3427d518ea86ea5edb7c5dfeabd205b5c636b70c7425",
  "launcher_sha256": "d7a75fc3795a9b09bca8d5defbfb22b8a1c257e9dba7b2a3990d71c956a0aa0e",
  "profile_sha256": "9d9d615e1d6772666f0cb96a197f7a1a9391ebe66719f8d9b73f60cc7bcb1e3a",
  "cargo_lock_sha256": "de73f6244c067c5646abd70df2a5809454bc0e4dfb00cf166930fcd1f35fff90",
  "quickjs_header_sha256": "979127138da79cad5ddc7effa1afb13e8b02f744da2aa36844d4cb9d8cb92de9",
  "harness_sha256": "8558640e8c803c7ee4dc86fb1cc9f4c840dcdf1dd113a3dcdaacf70e0a5b4e49"
}
```

## Launch

Exact host OS: `macOS-26.5-arm64-arm-64bit-Mach-O`. OS version:
```text
ProductName:		macOS
ProductVersion:		26.5
BuildVersion:		25F71
```

All concrete argv, empty env, source hashes, statuses and kernel usage are in `experiments/runtime-gate-v2/task17-synthetic-results.json`. OS argv:

```json
[
  "/usr/bin/sandbox-exec",
  "-D",
  "RUNNER=/Volumes/250GB SAMSUNG 970 EVO PLUS /dev-tools/macroscope-cache/targets/wt-model-step/debug/macroscope-runner",
  "-f",
  "/Volumes/250GB SAMSUNG 970 EVO PLUS /dev-tools/macroscope-wt/model-step/experiments/runtime-gate-v2/profile.sb",
  "/Volumes/250GB SAMSUNG 970 EVO PLUS /dev-tools/macroscope-cache/targets/wt-model-step/debug/macroscope-runner"
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

QuickJS limits: 64 MiB allocator, 512 KiB stack, 10 s process CPU, 12 s wall; requests can only lower ceilings. Interrupt checks wall and process CPU; parent deadline also covers parser/native code. Stdin is capped at 20,000,000 bytes. At most one output plus five planned connection calls (JSON arguments ≤64 KiB each), raw output text ≤20,000,000 bytes; serialized response including escaping/logs ≤20,000,000 bytes; logs ≤1 MiB including per-entry overhead. SDK CSV host buffers and conversions are bounded by input/output/shape ceilings; the QuickJS heap ceiling alone is not a claim that all host allocations count as JS heap.

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
| command | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: Deno absent"], "planned_calls": [], "outputs": []}; peak RSS=4702208 |
| config-import-map | error:syntax | DENIED | UNKNOWN | {"ok": false, "error": {"kind": "syntax", "message": "imports are forbidden"}}; peak RSS=3047424 |
| config | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: Deno absent"], "planned_calls": [], "outputs": []}; peak RSS=4702208 |
| connect | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: Deno absent"], "planned_calls": [], "outputs": []}; peak RSS=4718592 |
| connection-args-limit | error:forbidden | DENIED | UNKNOWN | {"ok": false, "error": {"kind": "forbidden", "message": "invalid connection request or args exceed 64 KiB"}}; peak RSS=5029888 |
| connection-call-limit | error:forbidden | DENIED | UNKNOWN | {"ok": false, "error": {"kind": "forbidden", "message": "at most 5 connection calls per run"}}; peak RSS=4751360 |
| connection-planned | exact fixture | PASS | UNKNOWN | {"ok": true, "log": [], "planned_calls": [{"connectorId": "gmail", "toolName": "create_draft", "args": {"body": "review me"}}], "outputs": []}; peak RSS=4784128 |
| cpu | error:cpu | DENIED | UNKNOWN | {"ok": false, "error": {"kind": "cpu", "message": "execution budget exceeded"}}; peak RSS=4718592 |
| dynamic-blob | error:syntax | DENIED | UNKNOWN | {"ok": false, "error": {"kind": "syntax", "message": "imports are forbidden"}}; peak RSS=3047424 |
| dynamic-data | error:syntax | DENIED | UNKNOWN | {"ok": false, "error": {"kind": "syntax", "message": "imports are forbidden"}}; peak RSS=3047424 |
| dynamic-remote | error:syntax | DENIED | UNKNOWN | {"ok": false, "error": {"kind": "syntax", "message": "imports are forbidden"}}; peak RSS=3047424 |
| env-all | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: process absent"], "planned_calls": [], "outputs": []}; peak RSS=4718592 |
| env-home | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: Deno absent"], "planned_calls": [], "outputs": []}; peak RSS=4718592 |
| eval | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: eval absent"], "planned_calls": [], "outputs": []}; peak RSS=4702208 |
| fetch | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: fetch absent"], "planned_calls": [], "outputs": []}; peak RSS=4702208 |
| ffi | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: Deno absent"], "planned_calls": [], "outputs": []}; peak RSS=4702208 |
| function-constructor | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: constructors absent"], "planned_calls": [], "outputs": []}; peak RSS=4702208 |
| host-huge-date | error:forbidden | DENIED | UNKNOWN | {"ok": false, "error": {"kind": "forbidden", "message": "date argument too large"}}; peak RSS=6799360 |
| host-huge-money | error:forbidden | DENIED | UNKNOWN | {"ok": false, "error": {"kind": "forbidden", "message": "money argument too large"}}; peak RSS=6782976 |
| host-unknown-handle | error:forbidden | DENIED | UNKNOWN | {"ok": false, "error": {"kind": "forbidden", "message": "unknown input handle"}}; peak RSS=4718592 |
| hostile-exception | error:runtime | DENIED | UNKNOWN | {"ok": false, "error": {"kind": "runtime", "message": "JavaScript execution failed: use synchronous SDK code only; eval, Function, Worker, Date, Math.random and OS APIs are unavailable"}}; peak RSS=4734976 |
| memory-objects | error:memory | DENIED | UNKNOWN | {"ok": false, "error": {"kind": "memory", "message": "JavaScript execution failed: use synchronous SDK code only; eval, Function, Worker, Date, Math.random and OS APIs are unavailable"}}; peak RSS=73924608 |
| memory | error:memory | DENIED | UNKNOWN | {"ok": false, "error": {"kind": "memory", "message": "JavaScript execution failed: use synchronous SDK code only; eval, Function, Worker, Date, Math.random and OS APIs are unavailable"}}; peak RSS=71155712 |
| model-ask-limit | error:model_limit | DENIED | UNKNOWN | {"ok": false, "error": {"kind": "model_limit", "message": "At most 3 model requests per run; repeated unanswered key"}}; peak RSS=10305536 |
| model-ask-no-network | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: no fetch or socket API exists"], "planned_calls": [], "outputs": []}; peak RSS=4718592 |
| ok | exact fixture | PASS | UNKNOWN | {"ok": true, "log": [], "planned_calls": [], "outputs": [{"name": "ok.csv", "bytes": 9}]}; peak RSS=4734976 |
| output-absolute | error:forbidden | DENIED | UNKNOWN | {"ok": false, "error": {"kind": "forbidden", "message": "output requires a .csv, .tsv, .txt, .md or .json basename"}}; peak RSS=4718592 |
| output-flood | error:output_limit | DENIED | UNKNOWN | {"ok": false, "error": {"kind": "output_limit", "message": "output byte limit exceeded"}}; peak RSS=44810240 |
| output-nul | error:forbidden | DENIED | UNKNOWN | {"ok": false, "error": {"kind": "forbidden", "message": "output requires a .csv, .tsv, .txt, .md or .json basename"}}; peak RSS=4718592 |
| output-repeat | error:output_limit | DENIED | UNKNOWN | {"ok": false, "error": {"kind": "output_limit", "message": "only one output is allowed"}}; peak RSS=4751360 |
| output-symlink | error:forbidden | DENIED | UNKNOWN | {"ok": false, "error": {"kind": "forbidden", "message": "output requires a .csv, .tsv, .txt, .md or .json basename"}}; peak RSS=4718592 |
| output-traversal | error:forbidden | DENIED | UNKNOWN | {"ok": false, "error": {"kind": "forbidden", "message": "output requires a .csv, .tsv, .txt, .md or .json basename"}}; peak RSS=4718592 |
| prototype-pollution | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: frozen SDK"], "planned_calls": [], "outputs": []}; peak RSS=4702208 |
| read-home | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: Deno absent"], "planned_calls": [], "outputs": []}; peak RSS=4718592 |
| read-hosts | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: Deno absent"], "planned_calls": [], "outputs": []}; peak RSS=4702208 |
| static-hosts | error:syntax | DENIED | UNKNOWN | {"ok": false, "error": {"kind": "syntax", "message": "imports are forbidden"}}; peak RSS=3047424 |
| static-private | error:syntax | DENIED | UNKNOWN | {"ok": false, "error": {"kind": "syntax", "message": "imports are forbidden"}}; peak RSS=3047424 |
| static-remote | error:syntax | DENIED | UNKNOWN | {"ok": false, "error": {"kind": "syntax", "message": "imports are forbidden"}}; peak RSS=3047424 |
| stderr | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: console absent"], "planned_calls": [], "outputs": []}; peak RSS=4702208 |
| stdout | error:output_limit | DENIED | UNKNOWN | {"ok": false, "error": {"kind": "output_limit", "message": "log byte limit exceeded"}}; peak RSS=5767168 |
| storage-cache | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: caches absent"], "planned_calls": [], "outputs": []}; peak RSS=4702208 |
| storage-indexeddb | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: indexedDB absent"], "planned_calls": [], "outputs": []}; peak RSS=4702208 |
| storage-local | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: localStorage absent"], "planned_calls": [], "outputs": []}; peak RSS=4702208 |
| symlink-hosts | error:syntax | DENIED | UNKNOWN | {"ok": false, "error": {"kind": "syntax", "message": "imports are forbidden"}}; peak RSS=3047424 |
| symlink-private | error:syntax | DENIED | UNKNOWN | {"ok": false, "error": {"kind": "syntax", "message": "imports are forbidden"}}; peak RSS=3047424 |
| wall | error:wall | DENIED | UNKNOWN | {"ok": false, "error": {"kind": "wall", "message": "execution budget exceeded"}}; peak RSS=4734976 |
| worker-command | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: Worker absent"], "planned_calls": [], "outputs": []}; peak RSS=4702208 |
| worker-connect | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: Worker absent"], "planned_calls": [], "outputs": []}; peak RSS=4702208 |
| worker-cpu | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: Worker absent"], "planned_calls": [], "outputs": []}; peak RSS=4718592 |
| worker-data | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: Worker absent"], "planned_calls": [], "outputs": []}; peak RSS=4718592 |
| worker-env | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: Worker absent"], "planned_calls": [], "outputs": []}; peak RSS=4718592 |
| worker-fetch | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: Worker absent"], "planned_calls": [], "outputs": []}; peak RSS=4718592 |
| worker-ffi | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: Worker absent"], "planned_calls": [], "outputs": []}; peak RSS=4734976 |
| worker-local | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: Worker absent"], "planned_calls": [], "outputs": []}; peak RSS=4702208 |
| worker-memory | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: Worker absent"], "planned_calls": [], "outputs": []}; peak RSS=4702208 |
| worker-read | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: Worker absent"], "planned_calls": [], "outputs": []}; peak RSS=4702208 |
| worker-remote | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: Worker absent"], "planned_calls": [], "outputs": []}; peak RSS=4734976 |
| worker-storage | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: Worker absent"], "planned_calls": [], "outputs": []}; peak RSS=4734976 |
| worker-write | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: Worker absent"], "planned_calls": [], "outputs": []}; peak RSS=4702208 |
| write | explicit denial marker | DENIED | UNKNOWN | {"ok": true, "log": ["DENIED: Deno absent"], "planned_calls": [], "outputs": []}; peak RSS=4702208 |
| orders | exact fixture | PASS | UNKNOWN | {"ok": true, "log": ["rows: 18"], "planned_calls": [], "outputs": [{"name": "orders_ready.csv", "bytes": 481}]}; peak RSS=5521408 |
| stdin-flood | error:forbidden | DENIED | UNKNOWN | {"ok": false, "error": {"kind": "forbidden", "message": "stdin request exceeds 20 MB or cannot be read"}} |

## Memory peak evidence

Kernel `getrusage(RUSAGE_CHILDREN).ru_maxrss`, bytes on macOS; no RSS polling. Each launcher has exactly one child. Memory canaries touch allocations so demand-zero pages do not hide resident growth. Unit tests independently use wait4 rusage. Admission threshold for these probes: 64 MiB + 16 MiB fixed slack.

| Canary / layer | Peak RSS bytes | Above 64 MiB | Slack bytes |
|---|---:|---:|---:|
| memory-objects / synthetic | 73924608 | 6815744 | 16777216 |
| memory / synthetic | 71155712 | 4046848 | 16777216 |

## Signatures, remaining gate requirements and verdict

```json
[
  {
    "argv": [
      "/usr/bin/codesign",
      "-dvv",
      "--entitlements",
      ":-",
      "/Volumes/250GB SAMSUNG 970 EVO PLUS /dev-tools/macroscope-cache/targets/wt-model-step/debug/macroscope-runner"
    ],
    "exit": 0,
    "stdout": "",
    "stderr": "Executable=/Volumes/250GB SAMSUNG 970 EVO PLUS /dev-tools/macroscope-cache/targets/wt-model-step/debug/macroscope-runner\nIdentifier=macroscope_runner-e5fcb2d420d6a35d\nFormat=Mach-O thin (arm64)\nCodeDirectory v=20400 size=162427 flags=0x20002(adhoc,linker-signed) hashes=5072+0 location=embedded\nSignature=adhoc\nInfo.plist=not bound\nTeamIdentifier=not set\nSealed Resources=none\nInternal requirements=none\nwarning: Specifying ':' in the path is deprecated and will not work in a future release\n"
  },
  {
    "argv": [
      "/usr/bin/codesign",
      "-dvv",
      "--entitlements",
      ":-",
      "/Volumes/250GB SAMSUNG 970 EVO PLUS /dev-tools/macroscope-cache/targets/wt-model-step/debug/macroscope-runner-launch"
    ],
    "exit": 0,
    "stdout": "",
    "stderr": "Executable=/Volumes/250GB SAMSUNG 970 EVO PLUS /dev-tools/macroscope-cache/targets/wt-model-step/debug/macroscope-runner-launch\nIdentifier=macroscope_runner_launch-4c98731b2016c928\nFormat=Mach-O thin (arm64)\nCodeDirectory v=20400 size=17954 flags=0x20002(adhoc,linker-signed) hashes=557+0 location=embedded\nSignature=adhoc\nInfo.plist=not bound\nTeamIdentifier=not set\nSealed Resources=none\nInternal requirements=none\nwarning: Specifying ':' in the path is deprecated and will not work in a future release\n"
  }
]
```

PASS requires every benign fixture and canary under a working OS boundary, all resource ceilings supported, and plan §7.3/P8 signed immutable launcher/app-bundle evidence. This developer spike has no trusted release signature or signed-bundle test. Even a successful `--os` canary run cannot assert full release admission without those receipts. UNKNOWN supplies no new OS admission evidence. ADR 0003 records the existing developer-boundary admission; synthetic reruns do not replace it. Windows is unverified.

Source admission conservatively rejects the byte sequence `import` (including comments/strings); no module loader is installed. eval/Function globals and function/async/generator constructor links are removed. Promise intrinsics are required for safe async teardown, but the Promise global is absent, returned promises are refused, and no job queue is drained. SDK objects are recursively frozen and have null prototypes. Rust date/money/formula handling is shared with the finite executor.
