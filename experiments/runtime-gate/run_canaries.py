"""Receipts, strict canary classification and fail-closed admission decision."""
import datetime
import json
import re
import subprocess
from pathlib import Path

from launcher import (ROOT, RUNS, DENO, FLAGS, RSS_LIMIT, CPU_LIMIT, WALL_LIMIT, TIMEBASE,
                      OUT_LIMIT, ERR_LIMIT, DISK_LIMIT, POLL_SECONDS, run, sha, verified_runtime)


def command(argv):
    p = subprocess.run(argv, capture_output=True, text=True, timeout=15)
    return dict(argv=argv, exit_code=p.returncode, stdout=p.stdout, stderr=p.stderr)


def classify(name, result):
    """Do not count crashes, parser failures or unsupported flags as denials."""
    stdout, stderr = result["stdout"], result["stderr"]
    code, reason = result["exit_code"], result["kill_reason"]
    if result["descendants_remaining"] or not result["leader_reaped"]:
        return "FAIL (unreaped tree)"
    expected_kill = {"cpu": {"cpu", "cpu-os"}, "worker-cpu": {"cpu", "cpu-os"},
                     "wall": {"wall"},
                     "memory": {"rss"}, "worker-memory": {"rss"},
                     "stdout": {"stdout"}, "stderr": {"stderr"}}
    if name in expected_kill:
        return "KILLED" if reason in expected_kill[name] and code in (-9, -24) else "FAIL (wrong termination)"
    if "storage" in name and reason in ("scratch", "rss"):
        return "KILLED" if code == -9 else "FAIL (limit exceeded before termination)"
    if reason:
        return "FAIL (unexpected kill)"
    try:
        msg = json.loads(stdout)
    except ValueError:
        msg = {}
    if name == "ok":
        return "PASS" if code == 0 and msg == {"ok": True, "value": 42} else "FAIL (benign)"
    if code == 42 or msg.get("escaped") or "CONFIG_LOADED" in stdout:
        return "ESCAPED"
    if code == 0 and msg.get("denied") is True:
        return "DENIED"
    if name.startswith("storage-") and code == 0 and msg.get("unavailable") is True:
        return "DENIED (API absent)"
    if "storage" in name and re.search(r"localStorage is not defined|localStorage is not supported|localStorage is not available", stderr + stdout, re.I):
        return "DENIED (API absent)"
    if name in ("dynamic-remote", "worker-remote", "static-remote") and re.search(
            r"remote.*(?:disabled|not allowed)|(?:disabled|not allowed).*remote|no-remote", stderr + stdout, re.I):
        return "DENIED"
    if name == "config-import-map" and ("not a relative path" in stderr or "not a dependency" in stderr) and "config-must-be-ignored" in stderr:
        return "DENIED (config ignored)"
    if name in ("static-hosts", "symlink-hosts") and re.search(
            r"SyntaxError|parse|Unexpected|invalid|Expected.*at file:///etc/hosts", stderr, re.I):
        # /etc/hosts is not JSON. A parser failure is evidence the loader read
        # outside the package, NOT evidence of denied filesystem access.
        return "ESCAPED (outside bytes parsed)"
    if name.startswith(("static-", "symlink-")) and re.search(
            r"[Pp]ermission denied|[Oo]peration not permitted|NotCapable", stderr):
        return "DENIED"
    if name == "worker-local" and "Requires read access" in stderr and "worker-body.ts" in stderr:
        return "DENIED (worker bootstrap)"
    return "FAIL (unclassified)"


def excerpt(result):
    if result is None:
        return "not run: Seatbelt unavailable; see admission probe"
    text = result["stderr"] or result["stdout"]
    text = text.replace(str(ROOT), "$EXPERIMENT")
    return re.sub(r"\s+", " ", text).strip()[:200]


def compact(result):
    if result is None:
        return None
    return {**{k: v for k, v in result.items() if k not in ("stdout", "stderr")},
            "stdout_excerpt": result["stdout"][:500], "stderr_excerpt": excerpt(result)}


def main():
    verified_runtime()
    lock = json.loads((ROOT / "runtime.lock.json").read_text())
    help_result = command([str(DENO), "run", "--help"])
    for flag in FLAGS[1:-1]:
        if flag not in help_result["stdout"]:
            raise RuntimeError(f"Pinned deno run --help does not support {flag}")
    (ROOT / ".runtime/deno-run-help.txt").write_text(help_result["stdout"])
    RUNS.mkdir(exist_ok=True)
    fixtures = RUNS / "fixtures"
    (fixtures / "home/.ssh").mkdir(parents=True, exist_ok=True)
    (fixtures / "home/.ssh/known_hosts").write_text("SYNTHETIC_KNOWN_HOST\n")
    (fixtures / "private.ts").write_text('export const marker = "SYNTHETIC_OUTSIDE_PACKAGE";\n')
    (fixtures / "private.json").write_text('{"marker":"SYNTHETIC_OUTSIDE_JSON"}\n')
    replacements = {"__SYNTHETIC_HOME__": str(fixtures / "home"),
                    "__OUTSIDE_FILE_URL__": (fixtures / "private.ts").as_uri(),
                    "__OUTSIDE_JSON__": str(fixtures / "private.json"),
                    "__WORKER_SOURCE_JSON__": json.dumps((ROOT / "canaries/worker-body.ts").read_text()),
                    "__MEMORY_SOURCE_JSON__": json.dumps((ROOT / "canaries/memory.ts").read_text()),
                    "__CPU_SOURCE_JSON__": json.dumps((ROOT / "canaries/cpu.ts").read_text()),
                    "__OUTSIDE_WRITE__": str(fixtures / "forbidden-write.txt")}
    # Always run the exact profile, not an allow-default compatibility substitute.
    print("Probing exact Seatbelt profile with ok.ts...", flush=True)
    probe = run(ROOT / "canaries", "ok.ts", replacements=replacements)
    seatbelt_available = classify("ok", probe) == "PASS"
    if not seatbelt_available:
        print(f"Seatbelt unavailable: exit={probe['exit_code']} {excerpt(probe)}", flush=True)
        print("Explicit synthetic Deno-permission fallback; all OS-boundary rows UNKNOWN.", flush=True)
    specs = [(n, n + ".ts", None) for n in (
        "ok", "read-hosts", "read-home", "static-private", "static-hosts", "static-remote",
        "fetch", "connect", "env-home", "env-all", "command", "ffi", "dynamic-data",
        "dynamic-remote", "write", "storage-local", "storage-cache", "storage-indexeddb",
        "symlink-hosts", "symlink-private", "config", "config-import-map", "cpu", "wall", "memory",
        "stdout", "stderr", "worker-local", "worker-memory", "worker-cpu")]
    specs += [("worker-" + action, "worker.ts", action) for action in (
        "read", "fetch", "connect", "env", "command", "ffi", "data", "remote", "write", "storage")]
    rows = []
    for name, entry, action in specs:
        subs = {**replacements, "__WORKER_ACTION__": action or "read"}
        sb = (run(ROOT / "canaries", entry, replacements=subs) if seatbelt_available else None)
        # Run Deno layer separately even on a Seatbelt-capable host to expose
        # loader/API behavior masked by OS denials.
        deno = run(ROOT / "canaries", entry, seatbelt=False, replacements=subs)
        observed = classify(name, sb) if sb is not None else "UNKNOWN"
        layer_observed = classify(name, deno)
        row = dict(canary=name, expected="PASS: JSON value=42" if name == "ok" else "DENIED or KILLED",
                   observed=observed, deno_permission_layer=layer_observed,
                   seatbelt=compact(sb), deno=compact(deno))
        rows.append(row)
        print(f"{name}: OS={observed}; Deno={layer_observed}; exit={deno['exit_code']}; "
              f"kill={deno['kill_reason']}; RSS={deno['process_tree_peak_rss_bytes']}; "
              f"overshoot={deno['rss_overshoot_bytes']}; {excerpt(deno)}", flush=True)
    escapes = [r["canary"] for r in rows if r["observed"].startswith("ESCAPED")]
    layer_escapes = [r["canary"] for r in rows if r["deno_permission_layer"].startswith("ESCAPED")]
    failures = [r["canary"] for r in rows if r["observed"].startswith("FAIL")]
    unknowns = [r["canary"] for r in rows if r["observed"] == "UNKNOWN"]
    # There is no hard aggregate memory/disk/CPU quota in this experiment.
    # A finite overshoot sample is NOT a proof that growth is bounded.
    limits_admitted = False
    verdict = "FAIL"
    os_info = command(["/usr/bin/sw_vers"])
    virtual_limit = command(["/bin/bash", "-c", "ulimit -v 262144; result=$?; ulimit -v; exit $result"])
    cpu_limit = command(["/bin/bash", "-c", "ulimit -t 30; ulimit -t"])
    signature = command(["/usr/bin/codesign", "-dv", "--verbose=4", str(DENO)])
    entitlements = command(["/usr/bin/codesign", "-d", "--entitlements", "-", str(DENO)])
    binary_version = command([str(DENO), "--version"])
    metadata = dict(timestamp_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),
                    verdict=verdict, selected_engine="finite host-owned CSV mapping",
                    generated_typescript_enabled=False, os=os_info, runtime=lock,
                    version_output=binary_version, profile_sha256=sha(ROOT / "profile.sb"),
                    profile=(ROOT / "profile.sb").read_text(), deno_argv_template=[str(DENO), *FLAGS, "<immutable-package>/<entry.ts>"],
                    env_template={"DENO_DIR": "<run>/scratch", "HOME": "<run>/scratch", "NO_COLOR": "1"},
                    launch_signature="unsigned shell/Python launcher; no launcher entitlements",
                    deno_signature=signature, deno_entitlements=entitlements,
                    launcher_sha256={n: sha(ROOT / n) for n in ("launcher.py", "run-sandboxed.sh")},
                    mach_lookup_allowlist=[], seatbelt_available=seatbelt_available,
                    process_monitor="libproc proc_listpids (group + recursive children), proc_pidinfo BSD/task metadata",
                    cpu_timebase={"numer": TIMEBASE.numer, "denom": TIMEBASE.denom},
                    seatbelt_probe=compact(probe), escapes=escapes, deno_layer_escapes=layer_escapes,
                    failures=failures, unknowns=unknowns,
                    limits=dict(admitted=limits_admitted, rss_bytes=RSS_LIMIT, cpu_seconds=CPU_LIMIT,
                                wall_seconds=WALL_LIMIT, stdout_bytes=OUT_LIMIT, stderr_bytes=ERR_LIMIT,
                                scratch_bytes=DISK_LIMIT, poll_target_seconds=POLL_SECONDS,
                                virtual_limit_probe=virtual_limit, cpu_limit_probe=cpu_limit,
                                enforcement="RLIMIT_CPU=30 per process; core dumps disabled. RSS, aggregate CPU, wall and du disk use polling. Output captures hard capped; producer bytes may exceed caps before kill. No hard aggregate RSS/disk quota; admission fails."),
                    rows=rows)
    (ROOT / "results.json").write_text(json.dumps(metadata, indent=2) + "\n")
    write_report(metadata, help_result["stdout"])
    print("VERDICT: FAIL — select finite host-owned CSV mapping; generated TypeScript DISABLED.", flush=True)
    print("Deno-layer escaping canaries: " + ", ".join(layer_escapes), flush=True)
    print("OS-boundary UNKNOWN: " + str(len(unknowns)) + " rows; aggregate limits NOT ADMITTED.", flush=True)
    print("Evidence: experiments/runtime-gate/results.json; docs/build/runtime-lock.md", flush=True)
    return 1


def cell(value):
    return str(value).replace("|", "\\|").replace("\n", " ").replace("`", "'")


def write_report(m, help_text):
    lines = ["# Runtime admission experiment — M0 day 6", "",
             f"**Verdict: {m['verdict']}. Generated TypeScript remains DISABLED. Select the finite host-owned CSV mapping (PLAN §7.3).**", "",
             f"Measured at {m['timestamp_utc']}. This is synthetic developer evidence, not product admission.", "",
             "Seatbelt admission: " + ("benign launch succeeded." if m["seatbelt_available"] else
              f"**UNKNOWN**: the enclosing sandbox rejected the exact profile: exit {m['seatbelt_probe']['exit_code']}, `{cell(m['seatbelt_probe']['stderr_excerpt'])}`."),
             "All OS-boundary rows below are UNKNOWN when that probe fails. Deno-only results cannot establish OS confinement.", "",
             "Deno-only escaping canaries: **" + ", ".join(m["deno_layer_escapes"]) + "**.",
             "OS escapes: " + (", ".join(m["escapes"]) or "none established; UNKNOWN is not a denial") + ".",
             "Aggregate resource admission: **FAIL**. RSS, aggregate CPU and scratch have polling only, without a hard aggregate OS bound. No rerun can return PASS until an independently verified hard bound replaces those monitors and every canary succeeds under Seatbelt.", "",
             "## Pin and host", "", "```text", m["version_output"]["stdout"].strip(),
             m["os"]["stdout"].strip(), "```", "",
             f"Archive SHA-256: `{m['runtime']['archive_sha256']}`.",
             f"Binary SHA-256: `{m['runtime']['binary_sha256']}`.",
             f"[Official release]({m['runtime']['archive_url']}); [archive checksum]({m['runtime']['archive_checksum_url']}); [binary checksum]({m['runtime']['binary_checksum_url']}). Both digests checked before execution; cached mismatches fail closed.", "",
             "## Exact launch", "", "The template substitutes absolute canonical run paths; each concrete argv, empty-environment allowlist, package manifest and receipt is in `results.json`. No inherited PATH, TMPDIR, API keys or DENO_* variables.", "", "```json",
             json.dumps({"argv": ["/usr/bin/sandbox-exec", "-D", "DENO=<pinned-binary>", "-D", "PACKAGE=<immutable-package>", "-D", "SCRATCH=<run>/scratch", "-f", str(ROOT / "profile.sb"), *m["deno_argv_template"]], "env": m["env_template"]}, indent=2), "```", "",
             "All switches were verified against the pinned binary's `deno run --help` (saved in `.runtime/deno-run-help.txt`). There are no --allow-* switches. --no-lock is supported and used: no dependencies are admitted or fetched, so there is no dependency lock discovery. Package byte manifests supplement the runtime lock. The cwd is the frozen package; input is synthetic JSON on stdin, the benign output is exactly one JSON message on stdout. The CLI rejects invalid JSON responses; the adversarial harness records malformed output/floods for evidence.", "",
             "## Profile", "", f"Full profile SHA-256: `{m['profile_sha256']}`.", "", "```scheme", m["profile"].strip(), "```", "",
             "Mach lookup allowlist: **empty**. No empirical claim of compatibility or minimality is possible in this enclosing sandbox. System read candidates are /usr/lib, /System/Library and /private/var/db/dyld; no home or general /private/var allowance. Pipes are inherited. Process exec is restricted to the pinned binary; process-fork and all network operations are denied. No file writes except the per-run scratch are allowed.", "",
             "The host copies synthetic canaries into a fresh package, hashes regular files, preserves malicious symlinks deliberately for loader probes, then sets files 0444 and directories 0555 before launch. This is experimental byte freezing, not a production immutable store: the same host UID can chmod it. Production admission still requires canonical graph validation, symlink/dynamic/data/blob rejection, immutable bytes and validation under the same working OS boundary. No scanner or chmod result is counted as a successful sandbox denial.", "",
             "## Canary receipts", "", "Expected malicious result: DENIED or KILLED. Deno layer is an explicit fallback experiment using the same flags/env/monitors, without sandbox-exec. An outside JSON parser error means bytes were read, and is classified as an escape; a synthetic valid outside module/JSON marker confirms the loader behavior. Missing APIs are marked explicitly. Generic crashes and unexpected errors never count as denials.", "",
             "| Canary | Expected | Observed OS boundary | Deno layer | Evidence (fallback when OS unknown; excerpt ≤200 chars) |",
             "|---|---|---|---|---|"]
    for row in m["rows"]:
        r = row["seatbelt"] or row["deno"]
        evidence = f"exit={r['exit_code']}; kill={r['kill_reason']}; {r['stderr_excerpt']}"
        lines.append("| " + " | ".join(cell(v) for v in (row["canary"], row["expected"], row["observed"], row["deno_permission_layer"], evidence)) + " |")
    lines += ["", "## Resource evidence", "",
              "RLIMIT_CPU is set in the child before exec (soft=hard=30 s, per process), with core dumps disabled. Aggregate CPU sums observed process CPU including observed descendants; exited descendants between polls can be missed. The watchdog polls libproc process-tree RSS/CPU (process group plus recursive children) and du -sk allocated scratch size with a 100 ms target, enforces 120 s wall, tracks descendants and process groups, kills the group and known descendants, waits for the leader and checks for survivors. Workers share a process and count toward its RSS/CPU. Local-file Worker bootstraps are separately probed; effect/resource probes use data URL worker bootstraps so a missing loader read grant cannot mask their behavior. CPU ticks use mach_timebase_info; the conversion factor is recorded in results.json. Spawned OS descendants are denied by Deno/Seatbelt; an OS descendant escaping both layers is not proven contained by this monitor.", "",
              f"`ulimit -v 262144`: exit {m['limits']['virtual_limit_probe']['exit_code']}; `{cell(m['limits']['virtual_limit_probe']['stderr'].strip())}`; reported limit `{m['limits']['virtual_limit_probe']['stdout'].strip()}`. No usable virtual-memory limit on this host. `ulimit -t 30`: exit {m['limits']['cpu_limit_probe']['exit_code']}, output `{m['limits']['cpu_limit_probe']['stdout'].strip()}`. Actual child uses Python setrlimit, the same OS primitive.", "",
              "Stdout captures retain at most 20,000,000 bytes; stderr at most 1,048,576 bytes. Crossing either cap kills the process group. The producer can emit extra pipe bytes before the kill, which are counted below and discarded. No unbounded log files are written. du measures allocated blocks, not a filesystem quota; scratch can grow between polls. All polling limits can overshoot without a proven finite bound: measured maxima below are samples, not guarantees. The sampled aggregate RSS peak can miss the final allocation burst. wait4 also records the kernel leader peak RSS (bytes on macOS) and CPU; the reported RSS overshoot is the larger of that kernel leader peak and the sampled tree peak minus the cap, a lower bound on actual aggregate overshoot. Scratch includes a final du sample after kill/reap.", "",
              "| Canary / layer | Kill | Sampled tree / kernel leader peak RSS bytes | RSS overshoot lower bound bytes | Peak scratch bytes | Scratch overshoot bytes | Max poll gap s | Sampled tree / kernel leader CPU s | Wall s | Stdout seen / retained | Stderr seen / retained | Survivors |",
              "|---|---|---:|---:|---:|---:|---:|---:|---:|---|---|---|"]
    for row in m["rows"]:
        if row["canary"] not in ("cpu", "worker-cpu", "wall", "memory", "worker-memory", "stdout", "stderr", "storage-local", "storage-cache", "storage-indexeddb", "worker-storage"):
            continue
        for layer in ("seatbelt", "deno"):
            r = row[layer]
            if r is None:
                continue
            values = [row["canary"] + " / " + layer, r["kill_reason"], f"{r['process_tree_peak_rss_bytes']} / {r['leader_kernel_peak_rss_bytes']}", r["rss_overshoot_lower_bound_bytes"], r["scratch_peak_allocated_bytes"], r["scratch_overshoot_bytes"], r["max_poll_gap_seconds"], f"{r['tree_cpu_seconds']} / {r['leader_kernel_cpu_seconds']}", r["wall_seconds"], f"{r['stdout_bytes_seen']} / {r['stdout_bytes_retained']}", f"{r['stderr_bytes_seen']} / {r['stderr_bytes_retained']}", r["descendants_remaining"]]
            lines.append("| " + " | ".join(cell(v) for v in values) + " |")
    lines += ["", "## Signing and limitations", "",
              "Launcher: unsigned shell/Python scripts, no launcher entitlements. This fails the plan's signed-launcher prerequisite. The downloaded Deno signature and entitlements are recorded below; no app/helper was signed or modified. Not yet tested inside a signed app bundle. Windows is not covered.", "",
              "Apple's installed sandbox-exec manual warns: **execute within a sandbox (DEPRECATED)** and **Developers who wish to sandbox an app should instead adopt the App Sandbox feature**. This host emitted no separate runtime deprecation warning; the observed runtime warning/error was the sandbox_apply rejection above.", "",
              "```text", m["deno_signature"]["stderr"].strip(), m["deno_entitlements"]["stdout"].strip(), m["deno_entitlements"]["stderr"].strip(), "```", "",
              "No real home/SSH fixtures or credentials were accessed. The home-read canary uses a host-created synthetic known_hosts outside the package. /etc/hosts probes use only that public system fixture. The fallback is never used automatically by run-sandboxed.sh. It is confined to this explicitly authorized synthetic experiment.", "",
              "Reproduce: `bash experiments/runtime-gate/fetch-deno.sh && bash experiments/runtime-gate/run-canaries.sh`. Fetch exits 0 on verified bytes. The harness intentionally exits 1 for a failed admission gate, after writing both reports. Rerun outside the enclosing sandbox to resolve UNKNOWN rows; do not interpret this report as permission to enable generated execution.", ""]
    (ROOT.parents[1] / "docs/build/runtime-lock.md").write_text("\n".join(lines))


if __name__ == "__main__":
    raise SystemExit(main())
