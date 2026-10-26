# Runtime admission experiment — M0 day 6

**Verdict: FAIL. Generated TypeScript remains DISABLED. Select the finite host-owned CSV mapping (PLAN §7.3).**

Measured at 2026-10-26T20:38:53.754495+00:00. This is synthetic developer evidence, not product admission.

Seatbelt admission: benign launch succeeded.
All OS-boundary rows below are UNKNOWN when that probe fails. Deno-only results cannot establish OS confinement.

Deno-only escaping canaries: **static-private, static-hosts, dynamic-data, symlink-hosts, symlink-private, worker-data**.
OS escapes: dynamic-data, worker-data.
Aggregate resource admission: **FAIL**. RSS, aggregate CPU and scratch have polling only, without a hard aggregate OS bound. No rerun can return PASS until an independently verified hard bound replaces those monitors and every canary succeeds under Seatbelt.

## Pin and host

```text
deno 2.9.8 (stable, release, aarch64-apple-darwin)
v8 15.0.245.2-rusty
typescript 6.0.3
ProductName:		macOS
ProductVersion:		26.5
BuildVersion:		25F71
```

Archive SHA-256: `9f0bf660393da2eaff044cdc7444cb08c73f235849b61d224d825a194ce7c98e`.
Binary SHA-256: `d1e99265e5e6ee096c5780d88ff7d3e6cfdf33b1953a118b4889cab8ca6980ba`.
[Official release](https://github.com/denoland/deno/releases/download/v2.9.8/deno-aarch64-apple-darwin.zip); [archive checksum](https://github.com/denoland/deno/releases/download/v2.9.8/deno-aarch64-apple-darwin.zip.sha256sum); [binary checksum](https://github.com/denoland/deno/releases/download/v2.9.8/deno-aarch64-apple-darwin.sha256sum). Both digests checked before execution; cached mismatches fail closed.

## Exact launch

The template substitutes absolute canonical run paths; each concrete argv, empty-environment allowlist, package manifest and receipt is in `results.json`. No inherited PATH, TMPDIR, API keys or DENO_* variables.

```json
{
  "argv": [
    "/usr/bin/sandbox-exec",
    "-D",
    "DENO=<pinned-binary>",
    "-D",
    "PACKAGE=<immutable-package>",
    "-D",
    "SCRATCH=<run>/scratch",
    "-f",
    "/Users/todd/StudioProjects/macroscope-wt/runtime-gate/experiments/runtime-gate/profile.sb",
    "/Users/todd/StudioProjects/macroscope-wt/runtime-gate/experiments/runtime-gate/.runtime/deno",
    "run",
    "--no-prompt",
    "--no-remote",
    "--no-npm",
    "--no-config",
    "--no-lock",
    "--cached-only",
    "--v8-flags=--max-old-space-size=200",
    "<immutable-package>/<entry.ts>"
  ],
  "env": {
    "DENO_DIR": "<run>/scratch",
    "HOME": "<run>/scratch",
    "NO_COLOR": "1"
  }
}
```

All switches were verified against the pinned binary's `deno run --help` (saved in `.runtime/deno-run-help.txt`). There are no --allow-* switches. --no-lock is supported and used: no dependencies are admitted or fetched, so there is no dependency lock discovery. Package byte manifests supplement the runtime lock. The cwd is the frozen package; input is synthetic JSON on stdin, the benign output is exactly one JSON message on stdout. The CLI rejects invalid JSON responses; the adversarial harness records malformed output/floods for evidence.

## Profile

Full profile SHA-256: `2f1310258596575feb64aa8caa9a22370277a00dbdebab7f61f5b7f70db39d65`.

```scheme
;; Deny-by-default Seatbelt profile for one generated-tool run (plan §7.3).
;; Minimal set found empirically on macOS 26.5 / Deno 2.9.8 outside any enclosing sandbox
;; (bisected by operation class, then by path). Nothing else is required for ok.ts.
(version 1)
(deny default)
(deny network*)
(deny process-fork)
(allow process-exec (literal (param "DENO")))
;; Deno aborts (SIGABRT) without sysctl writes as well as reads; unprivileged sysctl
;; writes cannot change system state, but this is recorded as a broader-than-ideal grant.
(allow sysctl*)
;; Metadata (stat) everywhere: needed for path resolution. Allows existence probing of
;; arbitrary paths but not reading contents. Recorded as a limitation.
(allow file-read-metadata)
;; Reading the root directory entry itself is required at startup.
(allow file-read* (literal "/"))
(allow file-read* (literal (param "DENO")))
(allow file-read* (subpath "/usr/lib") (subpath "/System/Library") (subpath "/private/var/db/dyld")
                  (subpath "/System/Volumes/Preboot/Cryptexes"))
(allow file-read* (literal "/dev/urandom") (literal "/dev/random") (literal "/dev/null"))
(allow file-read* (subpath (param "PACKAGE")))
(allow file-read* file-write* (subpath (param "SCRATCH")))
```

Mach lookup allowlist: **empty**. No empirical claim of compatibility or minimality is possible in this enclosing sandbox. System read candidates are /usr/lib, /System/Library and /private/var/db/dyld; no home or general /private/var allowance. Pipes are inherited. Process exec is restricted to the pinned binary; process-fork and all network operations are denied. No file writes except the per-run scratch are allowed.

The host copies synthetic canaries into a fresh package, hashes regular files, preserves malicious symlinks deliberately for loader probes, then sets files 0444 and directories 0555 before launch. This is experimental byte freezing, not a production immutable store: the same host UID can chmod it. Production admission still requires canonical graph validation, symlink/dynamic/data/blob rejection, immutable bytes and validation under the same working OS boundary. No scanner or chmod result is counted as a successful sandbox denial.

## Canary receipts

Expected malicious result: DENIED or KILLED. Deno layer is an explicit fallback experiment using the same flags/env/monitors, without sandbox-exec. An outside JSON parser error means bytes were read, and is classified as an escape; a synthetic valid outside module/JSON marker confirms the loader behavior. Missing APIs are marked explicitly. Generic crashes and unexpected errors never count as denials.

| Canary | Expected | Observed OS boundary | Deno layer | Evidence (fallback when OS unknown; excerpt ≤200 chars) |
|---|---|---|---|---|
| ok | PASS: JSON value=42 | PASS | PASS | exit=0; kill=None; {"ok":true,"value":42} |
| read-hosts | DENIED or KILLED | DENIED | DENIED | exit=0; kill=None; NotCapable: Requires read access to "/etc/hosts", run again with the --allow-read flag |
| read-home | DENIED or KILLED | DENIED | DENIED | exit=0; kill=None; NotCapable: Requires read access to "$EXPERIMENT/.runs/fixtures/home/.ssh/known_hosts", run again with the --allow-read flag |
| static-private | DENIED or KILLED | DENIED | ESCAPED | exit=1; kill=None; error: Import 'file://$EXPERIMENT/.runs/fixtures/private.ts' failed. 0: Operation not permitted (os error 1) at file://$EXPERIMENT/.runs/3775ae0a78e044c0bb6699581a6db964/package/static-private.ts:1:24 |
| static-hosts | DENIED or KILLED | DENIED | ESCAPED (outside bytes parsed) | exit=1; kill=None; error: Import 'file:///etc/hosts' failed. 0: Operation not permitted (os error 1) at file://$EXPERIMENT/.runs/e3f2f67c3359443584eec344392d7989/package/static-hosts.ts:1:8 |
| static-remote | DENIED or KILLED | DENIED | DENIED | exit=1; kill=None; error: A remote specifier was requested: "https://example.com/runtime-gate-canary.ts", but --no-remote is specified. at file://$EXPERIMENT/.runs/e17961cbfcaf4e5a86bb9eab05435967/package/static-remote. |
| fetch | DENIED or KILLED | DENIED | DENIED | exit=0; kill=None; NotCapable: Requires net access to "example.com:443", run again with the --allow-net flag |
| connect | DENIED or KILLED | DENIED | DENIED | exit=0; kill=None; NotCapable: Requires net access to "127.0.0.1:9", run again with the --allow-net flag |
| env-home | DENIED or KILLED | DENIED | DENIED | exit=0; kill=None; NotCapable: Requires env access to "HOME", run again with the --allow-env flag |
| env-all | DENIED or KILLED | DENIED | DENIED | exit=0; kill=None; NotCapable: Requires env access, run again with the --allow-env flag |
| command | DENIED or KILLED | DENIED | DENIED | exit=0; kill=None; NotCapable: Requires run access to "/bin/sh", run again with the --allow-run flag |
| ffi | DENIED or KILLED | DENIED | DENIED | exit=0; kill=None; NotCapable: Requires ffi access to "/usr/lib/libSystem.B.dylib", run again with the --allow-ffi flag |
| dynamic-data | DENIED or KILLED | ESCAPED | ESCAPED | exit=42; kill=None; {"escaped":true,"canary":"dynamic-data"} |
| dynamic-remote | DENIED or KILLED | DENIED | DENIED | exit=0; kill=None; TypeError: A remote specifier was requested: "https://example.com/runtime-gate-canary.ts", but --no-remote is specified. |
| write | DENIED or KILLED | DENIED | DENIED | exit=0; kill=None; NotCapable: Requires write access to "$EXPERIMENT/.runs/fixtures/forbidden-write.txt", run again with the --allow-write flag |
| storage-local | DENIED or KILLED | DENIED | DENIED | exit=0; kill=None; QuotaExceededError: Exceeded maximum storage size |
| storage-cache | DENIED or KILLED | KILLED | KILLED | exit=-9; kill=scratch;  |
| storage-indexeddb | DENIED or KILLED | DENIED (API absent) | DENIED (API absent) | exit=0; kill=None; {"unavailable":true,"canary":"storage-indexeddb","api":"IndexedDB"} |
| symlink-hosts | DENIED or KILLED | DENIED | ESCAPED (outside bytes parsed) | exit=1; kill=None; error: Import 'file://$EXPERIMENT/.runs/98a0694fc476484aae5f1dc030dc534a/package/hosts-link.json' failed. 0: Operation not permitted (os error 1) at file://$EXPERIMENT/.runs/98a0694fc476484aae5f1dc030 |
| symlink-private | DENIED or KILLED | DENIED | ESCAPED | exit=1; kill=None; error: Import 'file://$EXPERIMENT/.runs/28a0ed27207d44a9b7f428c1ee856307/package/private-link.json' failed. 0: Operation not permitted (os error 1) at file://$EXPERIMENT/.runs/28a0ed27207d44a9b7f428c1 |
| config | DENIED or KILLED | DENIED | DENIED | exit=0; kill=None; NotCapable: Requires read access to "/etc/hosts", run again with the --allow-read flag |
| config-import-map | DENIED or KILLED | DENIED (config ignored) | DENIED (config ignored) | exit=1; kill=None; error: Import "config-must-be-ignored" not a dependency hint: If you want to use the npm package, try running 'deno add npm:config-must-be-ignored' at file://$EXPERIMENT/.runs/1d0afdc847b34d0d8d5fa834 |
| cpu | DENIED or KILLED | KILLED | KILLED | exit=-24; kill=cpu-os;  |
| wall | DENIED or KILLED | KILLED | KILLED | exit=-9; kill=wall;  |
| memory | DENIED or KILLED | KILLED | KILLED | exit=-9; kill=rss;  |
| stdout | DENIED or KILLED | KILLED | KILLED | exit=-9; kill=stdout; xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx |
| stderr | DENIED or KILLED | KILLED | KILLED | exit=-9; kill=stderr; xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx |
| worker-local | DENIED or KILLED | DENIED (worker bootstrap) | DENIED (worker bootstrap) | exit=1; kill=None; error: Uncaught (in worker "") Requires read access to "$EXPERIMENT/.runs/76aaa3daa00945a892fc9cab63c84f96/package/worker-body.ts", run again with the --allow-read flag error: Uncaught (in promise) Er |
| worker-memory | DENIED or KILLED | KILLED | KILLED | exit=-9; kill=rss;  |
| worker-cpu | DENIED or KILLED | KILLED | KILLED | exit=-24; kill=cpu-os;  |
| worker-read | DENIED or KILLED | DENIED | DENIED | exit=0; kill=None; NotCapable: Requires read access to "/etc/hosts", run again with the --allow-read flag |
| worker-fetch | DENIED or KILLED | DENIED | DENIED | exit=0; kill=None; NotCapable: Requires net access to "example.com:443", run again with the --allow-net flag |
| worker-connect | DENIED or KILLED | DENIED | DENIED | exit=0; kill=None; NotCapable: Requires net access to "127.0.0.1:9", run again with the --allow-net flag |
| worker-env | DENIED or KILLED | DENIED | DENIED | exit=0; kill=None; NotCapable: Requires env access, run again with the --allow-env flag |
| worker-command | DENIED or KILLED | DENIED | DENIED | exit=0; kill=None; NotCapable: Requires run access to "/bin/sh", run again with the --allow-run flag |
| worker-ffi | DENIED or KILLED | DENIED | DENIED | exit=0; kill=None; NotCapable: Requires ffi access to "/usr/lib/libSystem.B.dylib", run again with the --allow-ffi flag |
| worker-data | DENIED or KILLED | ESCAPED | ESCAPED | exit=42; kill=None; {"escaped":true,"canary":"worker-data"} |
| worker-remote | DENIED or KILLED | DENIED | DENIED | exit=0; kill=None; TypeError: A remote specifier was requested: "https://example.com/runtime-gate-canary.ts", but --no-remote is specified. |
| worker-write | DENIED or KILLED | DENIED | DENIED | exit=0; kill=None; NotCapable: Requires write access to "$EXPERIMENT/.runs/fixtures/forbidden-write.txt", run again with the --allow-write flag |
| worker-storage | DENIED or KILLED | DENIED (API absent) | DENIED (API absent) | exit=0; kill=None; ReferenceError: localStorage is not defined |

## Resource evidence

RLIMIT_CPU is set in the child before exec (soft=hard=30 s, per process), with core dumps disabled. Aggregate CPU sums observed process CPU including observed descendants; exited descendants between polls can be missed. The watchdog polls libproc process-tree RSS/CPU (process group plus recursive children) and du -sk allocated scratch size with a 100 ms target, enforces 120 s wall, tracks descendants and process groups, kills the group and known descendants, waits for the leader and checks for survivors. Workers share a process and count toward its RSS/CPU. Local-file Worker bootstraps are separately probed; effect/resource probes use data URL worker bootstraps so a missing loader read grant cannot mask their behavior. CPU ticks use mach_timebase_info; the conversion factor is recorded in results.json. Spawned OS descendants are denied by Deno/Seatbelt; an OS descendant escaping both layers is not proven contained by this monitor.

`ulimit -v 262144`: exit 1; `/bin/bash: line 0: ulimit: virtual memory: cannot modify limit: Invalid argument`; reported limit `unlimited`. No usable virtual-memory limit on this host. `ulimit -t 30`: exit 0, output `30`. Actual child uses Python setrlimit, the same OS primitive.

Stdout captures retain at most 20,000,000 bytes; stderr at most 1,048,576 bytes. Crossing either cap kills the process group. The producer can emit extra pipe bytes before the kill, which are counted below and discarded. No unbounded log files are written. du measures allocated blocks, not a filesystem quota; scratch can grow between polls. All polling limits can overshoot without a proven finite bound: measured maxima below are samples, not guarantees. The sampled aggregate RSS peak can miss the final allocation burst. wait4 also records the kernel leader peak RSS (bytes on macOS) and CPU; the reported RSS overshoot is the larger of that kernel leader peak and the sampled tree peak minus the cap, a lower bound on actual aggregate overshoot. Scratch includes a final du sample after kill/reap.

| Canary / layer | Kill | Sampled tree / kernel leader peak RSS bytes | RSS overshoot lower bound bytes | Peak scratch bytes | Scratch overshoot bytes | Max poll gap s | Sampled tree / kernel leader CPU s | Wall s | Stdout seen / retained | Stderr seen / retained | Survivors |
|---|---|---:|---:|---:|---:|---:|---:|---:|---|---|---|
| storage-local / seatbelt | None | 39043072 / 39043072 | 0 | 13389824 | 0 | 0.1021 | 0.1 / 0.112 | 0.267 | 112 / 112 | 50 / 50 | [] |
| storage-local / deno | None | 38977536 / 38977536 | 0 | 13848576 | 0 | 0.102 | 0.091 / 0.103 | 0.264 | 112 / 112 | 50 / 50 | [] |
| storage-cache / seatbelt | scratch | 104038400 / 104038400 | 0 | 45010944 | 11456512 | 0.1021 | 0.095 / 0.099 | 0.338 | 0 / 0 | 0 / 0 | [] |
| storage-cache / deno | scratch | 100171776 / 100171776 | 0 | 48181248 | 14626816 | 0.102 | 0.101 / 0.104 | 0.343 | 0 / 0 | 0 / 0 | [] |
| storage-indexeddb / seatbelt | None | 1048576 / 23412736 | 0 | 225280 | 0 | 0.0065 | 0.003 / 0.033 | 0.065 | 68 / 68 | 0 / 0 | [] |
| storage-indexeddb / deno | None | 114688 / 23445504 | 0 | 299008 | 0 | 0.0037 | 0.002 / 0.018 | 0.042 | 68 / 68 | 0 / 0 | [] |
| cpu / seatbelt | cpu-os | 23887872 / 23887872 | 0 | 258048 | 0 | 0.1062 | 29.985 / 30.048 | 30.777 | 0 / 0 | 0 / 0 | [] |
| cpu / deno | cpu-os | 23920640 / 23920640 | 0 | 258048 | 0 | 0.1021 | 29.916 / 30.012 | 30.089 | 0 / 0 | 0 / 0 | [] |
| wall / seatbelt | wall | 36487168 / 36487168 | 0 | 761856 | 0 | 0.1224 | 0.045 / 0.046 | 120.02 | 0 / 0 | 0 / 0 | [] |
| wall / deno | wall | 38174720 / 38174720 | 0 | 765952 | 0 | 0.1027 | 0.065 / 0.067 | 120.038 | 0 / 0 | 0 / 0 | [] |
| memory / seatbelt | rss | 385925120 / 435634176 | 167198720 | 258048 | 0 | 0.1001 | 0.054 / 0.069 | 0.159 | 0 / 0 | 0 / 0 | [] |
| memory / deno | rss | 670007296 / 704430080 | 435994624 | 258048 | 0 | 0.1003 | 0.093 / 0.108 | 0.154 | 0 / 0 | 0 / 0 | [] |
| stdout / seatbelt | stdout | 671744 / 20103168 | 0 | 258048 | 0 | 0.0016 | 0.001 / 0.018 | 0.043 | 20119552 / 20000000 | 0 / 0 | [] |
| stdout / deno | stdout | 475136 / 20135936 | 0 | 258048 | 0 | 0.0027 | 0.001 / 0.017 | 0.041 | 20119552 / 20000000 | 0 / 0 | [] |
| stderr / seatbelt | stderr | 655360 / 20250624 | 0 | 258048 | 0 | 0.0017 | 0.001 / 0.016 | 0.039 | 0 / 0 | 1179648 / 1048576 | [] |
| stderr / deno | stderr | 606208 / 20021248 | 0 | 258048 | 0 | 0.0026 | 0.001 / 0.012 | 0.035 | 0 / 0 | 1179648 / 1048576 | [] |
| worker-memory / seatbelt | rss | 707100672 / 738000896 | 469565440 | 774144 | 0 | 0.102 | 0.145 / 0.155 | 0.153 | 0 / 0 | 0 / 0 | [] |
| worker-memory / deno | rss | 1020346368 / 1075150848 | 806715392 | 774144 | 0 | 0.102 | 0.181 / 0.213 | 0.168 | 0 / 0 | 0 / 0 | [] |
| worker-cpu / seatbelt | cpu-os | 64176128 / 64176128 | 0 | 774144 | 0 | 0.1021 | 29.922 / 30.03 | 7.562 | 0 / 0 | 0 / 0 | [] |
| worker-cpu / deno | cpu-os | 66994176 / 66994176 | 0 | 778240 | 0 | 0.1021 | 29.695 / 30.064 | 7.715 | 0 / 0 | 0 / 0 | [] |
| worker-storage / seatbelt | None | 671744 / 34127872 | 0 | 335872 | 0 | 0.0016 | 0.001 / 0.025 | 0.047 | 126 / 126 | 44 / 44 | [] |
| worker-storage / deno | None | 524288 / 34193408 | 0 | 335872 | 0 | 0.0024 | 0.001 / 0.021 | 0.042 | 126 / 126 | 44 / 44 | [] |

## Signing and limitations

Launcher: unsigned shell/Python scripts, no launcher entitlements. This fails the plan's signed-launcher prerequisite. The downloaded Deno signature and entitlements are recorded below; no app/helper was signed or modified. Not yet tested inside a signed app bundle. Windows is not covered.

Apple's installed sandbox-exec manual warns: **execute within a sandbox (DEPRECATED)** and **Developers who wish to sandbox an app should instead adopt the App Sandbox feature**. This host emitted no separate runtime deprecation warning; the observed runtime warning/error was the sandbox_apply rejection above.

```text
Executable=/Users/todd/StudioProjects/macroscope-wt/runtime-gate/experiments/runtime-gate/.runtime/deno
Identifier=deno
Format=Mach-O thin (arm64)
CodeDirectory v=20500 size=632112 flags=0x10000(runtime) hashes=19743+7 location=embedded
VersionPlatform=1
VersionMin=786432
VersionSDK=918784
Hash type=sha256 size=32
CandidateCDHash sha256=e5931007a6d3801f650d292951c100e7d311a826
CandidateCDHashFull sha256=e5931007a6d3801f650d292951c100e7d311a82639e37ab33eb57b5b453af8e3
Hash choices=sha256
CMSDigest=e5931007a6d3801f650d292951c100e7d311a82639e37ab33eb57b5b453af8e3
CMSDigestType=2
Executable Segment base=0
Executable Segment limit=76890112
Executable Segment flags=0x1
Page size=4096
CDHash=e5931007a6d3801f650d292951c100e7d311a826
Signature size=9043
Authority=Developer ID Application: Deno Land Inc. (2H4KBF436B)
Authority=Developer ID Certification Authority
Authority=Apple Root CA
Timestamp=22 Oct 2026 at 3:14:38 AM
Info.plist=not bound
TeamIdentifier=2H4KBF436B
Runtime Version=14.5.0
Sealed Resources=none
Internal requirements count=1 size=164
[Dict]
	[Key] com.apple.security.cs.allow-dyld-environment-variables
	[Value]
		[Bool] true
	[Key] com.apple.security.cs.allow-jit
	[Value]
		[Bool] true
	[Key] com.apple.security.cs.allow-unsigned-executable-memory
	[Value]
		[Bool] true
	[Key] com.apple.security.cs.disable-executable-page-protection
	[Value]
		[Bool] true
	[Key] com.apple.security.cs.disable-library-validation
	[Value]
		[Bool] true
Executable=/Users/todd/StudioProjects/macroscope-wt/runtime-gate/experiments/runtime-gate/.runtime/deno
```

No real home/SSH fixtures or credentials were accessed. The home-read canary uses a host-created synthetic known_hosts outside the package. /etc/hosts probes use only that public system fixture. The fallback is never used automatically by run-sandboxed.sh. It is confined to this explicitly authorized synthetic experiment.

Reproduce: `bash experiments/runtime-gate/fetch-deno.sh && bash experiments/runtime-gate/run-canaries.sh`. Fetch exits 0 on verified bytes. The harness intentionally exits 1 for a failed admission gate, after writing both reports. Rerun outside the enclosing sandbox to resolve UNKNOWN rows; do not interpret this report as permission to enable generated execution.

## Reviewer rerun (Claude, 2026-10-27, outside the Codex sandbox)

The worker's profile could not be tested inside Codex's sandbox (`sandbox_apply: Operation
not permitted`), and outside it Deno aborted with SIGABRT before printing anything. Bisecting
by operation class and then by path on macOS 26.5 showed the minimal additions Deno 2.9.8
needs under `(deny default)`: `sysctl*` (reads **and** writes; read-only aborts), global
`file-read-metadata`, and reading the root directory entry `(literal "/")`. No mach-lookup,
no network, no file writes outside scratch. `profile.sb` now carries those grants with
comments, and the table above is the rerun with the real OS boundary (0 UNKNOWN rows).

Verdict remains **FAIL**, for two reasons that the plan treats as blocking:

1. `dynamic-data` / `worker-data`: code from `data:` URLs executes. The OS boundary still
   contains it (no reads, network, env, or processes escaped), but it bypasses the
   validated-module-graph requirement in plan §7.3.
2. Memory: process-tree RSS polling let a fast allocator reach ~400 MB (main) and ~750 MB
   (worker) past the 256 MiB limit before the kill. Plan §7.3: "best-effort polling that
   permits unbounded growth does not pass".

Decision (plan §7.3/§10): ship the finite host-owned CSV mapping; generated TypeScript stays
disabled. Paths to revisit: an import-graph validator that rejects `data:`/`blob:` and
Worker construction before launch, and an OS-enforced memory ceiling (e.g. a launchd/
posix_spawn resource limit or a Mach task memory limit) instead of polling.
