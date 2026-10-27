#!/usr/bin/env python3
"""Synthetic evidence. Default never calls sandbox-exec; --os is an outside rerun."""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import platform
import resource
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[2]
EXP = ROOT / "experiments/runtime-gate-v2"
CAP = 64 * 1024 * 1024
SLACK = 16 * 1024 * 1024


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def command_output(argv):
    proc = subprocess.run(argv, capture_output=True, text=True, timeout=30, cwd=ROOT)
    return {"argv": argv, "exit": proc.returncode, "stdout": proc.stdout, "stderr": proc.stderr}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--os", action="store_true", help="reviewer: run outside the enclosing sandbox")
    args = parser.parse_args()
    metadata = json.loads(subprocess.check_output(["cargo", "metadata", "--format-version=1", "--no-deps"], cwd=ROOT))
    binary = Path(metadata["target_directory"]) / "debug/macroscope-runner"
    launcher = binary.with_name("macroscope-runner-launch")
    profile = EXP / "profile.sb"
    specs = {p.stem: {"code": p.read_text(), "inputs": {}} for p in sorted((EXP / "canaries").glob("*.js"))}
    specs["orders"] = {"code": (EXP / "orders.js").read_text(), "inputs": {"input_csv": (ROOT / "fixtures/orders/input.csv").read_text()}}
    for name in ["cpu", "wall"]:
        specs[name]["limits"] = {"cpu_ms": 100 if name == "cpu" else 10000, "wall_ms": 2000 if name == "cpu" else 100}
    specs["stdin-flood"] = {"code": "function run(){}", "inputs": {"x": "x" * 20_000_001}}
    # Runs native launch setup as a probe, without silently changing a limit.
    def as_probe():
        try:
            resource.setrlimit(resource.RLIMIT_AS, (512 * 1024 * 1024,) * 2)
        except (ValueError, OSError) as error:
            print(type(error).__name__ + ": " + str(error), flush=True)
            os._exit(2)
        os._exit(0)
    as_child = os.fork()
    if as_child == 0:
        as_probe()
    _, as_status = os.waitpid(as_child, 0)
    as_available = os.waitstatus_to_exitcode(as_status) == 0

    def run(name, mode):
        request = json.dumps(specs[name]).encode()
        argv = [str(launcher), "--" + mode, str(binary), str(profile)]
        # The stdin-flood checks the worker protocol directly: launcher rejects
        # oversized host requests before launch, which is a separate host guard.
        if name == "stdin-flood":
            if mode == "synthetic":
                argv = [str(binary)]
            else:
                # The host launcher must refuse an oversized request before spawning
                # the worker: run it for real and require a non-zero exit with no output.
                proc = subprocess.run(argv, input=request, capture_output=True, env={}, cwd=ROOT, timeout=16)
                refused = proc.returncode != 0 and not proc.stdout
                return {"observed": "DENIED" if refused else "FAIL",
                        "evidence": "launcher exit " + str(proc.returncode) + "; " + proc.stderr.decode(errors="replace")[:160]}
        start = time.monotonic()
        try:
            proc = subprocess.run(argv, input=request, capture_output=True, env={}, cwd=ROOT, timeout=16)
        except subprocess.TimeoutExpired:
            return {"observed": "FAIL", "evidence": "harness timeout (runner deadline failed)", "argv": argv}
        receipt = {"argv": argv, "env": {}, "exit": proc.returncode, "wall_seconds": round(time.monotonic() - start, 4),
                   "stdout_bytes": len(proc.stdout), "stderr": proc.stderr.decode(errors="replace")[:2048]}
        stats = None
        for line in receipt["stderr"].splitlines():
            try:
                candidate = json.loads(line)
                if "peak_rss_bytes" in candidate:
                    stats = candidate
            except (ValueError, TypeError):
                pass
        receipt["usage"] = stats
        try:
            response = json.loads(proc.stdout)
        except ValueError:
            response = None
        # Small excerpt; never persist flood payloads.
        receipt["response"] = (response if response and not response.get("ok") else
                               {"ok": response.get("ok"), "log": response.get("log"), "planned_calls": response.get("planned_calls"), "outputs": [{"name": o["name"], "bytes": len(o["text"].encode())} for o in response.get("outputs", [])]} if response else None)
        expected = expectation(name)
        passed = proc.returncode == 0 and response is not None
        if name == "orders":
            passed = passed and response == {"ok": True, "log": ["rows: 18"], "planned_calls": [], "outputs": [{"name": "orders_ready.csv", "text": (ROOT / "fixtures/orders/expected.csv").read_text()}]}
        elif name == "ok":
            passed = passed and response == {"ok": True, "log": [], "planned_calls": [], "outputs": [{"name": "ok.csv", "text": "value\n42\n"}]}
        elif name == "connection-planned":
            passed = passed and response == {"ok": True, "log": [], "outputs": [], "planned_calls": [{"connectorId":"gmail", "toolName":"create_draft", "args":{"body":"review me"}}]}
        elif expected.startswith("error:"):
            passed = passed and response.get("ok") is False and response.get("error", {}).get("kind") == expected.split(":")[1]
        else:
            passed = passed and response.get("ok") is True and response.get("planned_calls") == [] and not response.get("outputs") and len(response.get("log", [])) == 1 and response["log"][0].startswith("DENIED: ")
        if name.startswith("memory"):
            passed = passed and stats is not None and stats["peak_rss_bytes"] <= CAP + SLACK
        receipt["observed"] = ("PASS" if name in ("orders", "ok", "connection-planned") else "DENIED") if passed else "FAIL"
        receipt["evidence"] = json.dumps(receipt["response"], ensure_ascii=True)[:220] + ("; peak RSS=" + str(stats["peak_rss_bytes"]) if stats else "")
        return receipt

    os_probe = run("ok", "seatbelt") if args.os else {"observed": "UNKNOWN", "evidence": "not attempted inside enclosing sandbox; reviewer must rerun --os"}
    os_available = os_probe["observed"] == "PASS"
    rows = []
    for name in specs:
        synthetic = run(name, "synthetic")
        os_row = run(name, "seatbelt") if os_available else {"observed": "UNKNOWN", "evidence": os_probe["evidence"]}
        rows.append({"canary": name, "expected": expectation(name), "synthetic": synthetic, "os": os_row})
        print(name + ": synthetic=" + synthetic["observed"] + ", OS=" + os_row["observed"], flush=True)
    failures = any(row["synthetic"]["observed"] == "FAIL" or row["os"]["observed"] == "FAIL" for row in rows)
    # Full admission includes OS confinement, finite AS, immutable signed launch
    # and signed bundle evidence. This unsigned spike cannot certify P8.
    unknown = any(row["os"]["observed"] == "UNKNOWN" for row in rows)
    # macOS rejects every finite RLIMIT_AS (EINVAL); there the QuickJS allocator limit is
    # the memory boundary, proven by the memory canaries' peak-RSS checks. Elsewhere a
    # finite RLIMIT_AS remains required. Release admission additionally needs signed,
    # immutable launcher/app-bundle evidence (plan §7.3, P8/P9), which this run cannot give.
    as_required = sys.platform != "darwin"
    if failures or (as_required and not as_available):
        verdict = "FAIL"
    elif unknown or not args.os:
        verdict = "UNKNOWN"
    else:
        verdict = "PASS (developer boundary; release admission pending signed bundle)"
    quickjs = next((p for p in Path(os.environ["CARGO_HOME"]).glob("registry/src/*/rquickjs-sys-0.14.0/quickjs/quickjs.h")), None)
    cargo_lock = (ROOT / "Cargo.lock").read_text()
    import tomllib
    lock = tomllib.loads(cargo_lock)
    pins = {p["name"]: {k: p[k] for k in ("version", "checksum") if k in p} for p in lock["package"] if p["name"].startswith("rquickjs")}
    report = {"measured_at": datetime.datetime.now(datetime.timezone.utc).isoformat(), "verdict": verdict,
              "runner_version": "0.1.0", "pins": pins, "quickjs_ng_version": "0.16.2", "quickjs_header_sha256": sha(quickjs) if quickjs else None,
              "runner_sha256": sha(binary), "launcher_sha256": sha(launcher), "profile_sha256": sha(profile),
              "cargo_lock_sha256": sha(ROOT / "Cargo.lock"), "harness_sha256": sha(Path(__file__)),
              "os": platform.platform(), "sw_vers": command_output(["/usr/bin/sw_vers"]),
              "signatures": [command_output(["/usr/bin/codesign", "-dvv", "--entitlements", ":-", str(p)]) for p in (binary, launcher)],
              "rlimits": {"RLIMIT_AS": 536870912, "RLIMIT_CPU": 11, "RLIMIT_FSIZE": 0, "RLIMIT_NOFILE": 32, "RLIMIT_NPROC": 0, "RLIMIT_CORE": 0},
              "finite_rlimit_as_available": as_available,
              "synthetic_omits": ["Seatbelt", "RLIMIT_AS"], "os_probe": os_probe, "rows": rows,
              "sources": {str(p.relative_to(ROOT)): sha(p) for p in sorted((ROOT / "crates/runner/src").rglob("*")) if p.is_file()},
              "canaries": {str(p.relative_to(ROOT)): sha(p) for p in sorted((EXP / "canaries").glob("*.js"))}}
    report["synthetic_suite"] = "FAIL" if any(row["synthetic"]["observed"] == "FAIL" for row in rows) else "PASS"
    # Synthetic reruns preserve the separately admitted OS-boundary evidence.
    results_name = "results.json" if args.os else "task17-synthetic-results.json"
    evidence_name = "runtime-lock-v2.md" if args.os else "task17-runtime-canaries.md"
    (EXP / results_name).write_text(json.dumps(report, indent=2) + "\n")
    write_markdown(report, evidence_name, results_name)
    print("Synthetic suite: " + report["synthetic_suite"] + "; admission verdict: " + verdict + "; evidence: docs/build/" + evidence_name, flush=True)
    # UNKNOWN OS evidence does not fail a synthetic-only test run or admit a runtime.
    return 1 if failures or (args.os and not verdict.startswith("PASS")) else 0


def expectation(name):
    if name == "model-ask-limit":
        return "error:model_limit"
    if name in ("ok", "orders", "connection-planned"):
        return "exact fixture"
    if name.startswith("memory"):
        return "error:memory"
    if name in ("cpu", "wall"):
        return "error:" + name
    if name in ("stdout", "output-flood", "output-repeat"):
        return "error:output_limit"
    if name in ("connection-args-limit", "connection-call-limit"):
        return "error:forbidden"
    if name.startswith("output-") or name.startswith("host-") or name == "stdin-flood":
        return "error:forbidden"
    if name == "hostile-exception":
        return "error:runtime"
    if name in ("static-private", "static-hosts", "static-remote", "dynamic-data", "dynamic-remote", "dynamic-blob", "symlink-hosts", "symlink-private", "config-import-map"):
        return "error:syntax"
    return "explicit denial marker"


def write_markdown(report, evidence_name, results_name):
    lines = ["# QuickJS runtime admission v2", "", "**Admission verdict: " + report["verdict"] + ". Synthetic suite: " + report["synthetic_suite"] + ".**", "",
             "Measured " + report["measured_at"] + ". Synthetic developer evidence; OS rows are UNKNOWN until an outside rerun with `bash experiments/runtime-gate-v2/run-canaries.sh --os`.", "",
             "The production launcher fails closed if any setrlimit or sandbox setup fails. Finite RLIMIT_AS availability on this host: **" + str(report["finite_rlimit_as_available"]) + "**. The explicit synthetic path omits Seatbelt and RLIMIT_AS; it is never an automatic fallback.", "",
             "## Versions and hashes", "", "Runner 0.1.0; rquickjs **0.14.0**, embedding QuickJS-NG **0.16.2**. Cargo.lock pins all crate archives by checksum. [rquickjs source/API](https://docs.rs/rquickjs/0.14.0/rquickjs/).", "", "```json", json.dumps({k: report[k] for k in ("pins", "runner_sha256", "launcher_sha256", "profile_sha256", "cargo_lock_sha256", "quickjs_header_sha256", "harness_sha256")}, indent=2), "```", "", "## Launch", "",
             "Exact host OS: `" + report["os"] + "`. OS version:\n```text\n" + report["sw_vers"]["stdout"].strip() + "\n```", "",
             "All concrete argv, empty env, source hashes, statuses and kernel usage are in `experiments/runtime-gate-v2/" + results_name + "`. OS argv:", "", "```json", json.dumps(["/usr/bin/sandbox-exec", "-D", "RUNNER=" + str(Path(report["signatures"][0]["argv"][-1])), "-f", str(EXP / "profile.sb"), report["signatures"][0]["argv"][-1]], indent=2), "```", "",
             "Environment: `{}`. Cwd: `/`. Inherited descriptors: stdin/stdout/stderr pipes only (CLOEXEC on other host descriptors). Dedicated process group; parent bounds stdout to 20,000,000 bytes and stderr to 65,536 bytes, kills/reaps at wall budget + 1 s. No descendants, module graph, cache or writable scratch are admitted.", "",
             "pre_exec soft=hard limits (all errors abort launch):", "", "```json", json.dumps(report["rlimits"], indent=2), "```", "",
             "QuickJS limits: 64 MiB allocator, 512 KiB stack, 10 s process CPU, 12 s wall; requests can only lower ceilings. Interrupt checks wall and process CPU; parent deadline also covers parser/native code. Stdin is capped at 20,000,000 bytes. At most one output plus five planned connection calls (JSON arguments ≤64 KiB each), raw output text ≤20,000,000 bytes; serialized response including escaping/logs ≤20,000,000 bytes; logs ≤1 MiB including per-entry overhead. SDK CSV host buffers and conversions are bounded by input/output/shape ceilings; the QuickJS heap ceiling alone is not a claim that all host allocations count as JS heap.", "",
             "## Seatbelt profile", "", "Candidate, not empirically minimized or compatibility-verified here; mach-lookup allowlist is empty. No user-data path, general metadata grant, write, network or process-fork grant.", "", "```scheme", (EXP / "profile.sb").read_text().strip(), "```", "", "## Canaries", "",
             "A generic crash/error never counts as a denial. API rows require an explicit absent-API marker; limit/import rows require the exact error kind. Benign rows require exact bytes. Symlink imports/output traversal are rejected as source/data without opening the target; no user home, credentials or real symlink targets are read. Config discovery is inapplicable: only source bytes from stdin execute, with no filesystem loader. Worker effect/CPU/memory probes all stop at the absent Worker API.", "",
             "| Canary | Expected | Synthetic worker | OS boundary | Evidence |", "|---|---|---|---|---|"]
    for row in report["rows"]:
        evidence = row["synthetic"]["evidence"].replace("|", "\\|").replace("\n", " ")
        lines.append(f'| {row["canary"]} | {row["expected"]} | {row["synthetic"]["observed"]} | {row["os"]["observed"]} | {evidence} |')
    lines += ["", "## Memory peak evidence", "", "Kernel `getrusage(RUSAGE_CHILDREN).ru_maxrss`, bytes on macOS; no RSS polling. Each launcher has exactly one child. Memory canaries touch allocations so demand-zero pages do not hide resident growth. Unit tests independently use wait4 rusage. Admission threshold for these probes: 64 MiB + 16 MiB fixed slack.", "", "| Canary / layer | Peak RSS bytes | Above 64 MiB | Slack bytes |", "|---|---:|---:|---:|"]
    for row in report["rows"]:
        if row["canary"].startswith("memory"):
            for layer in ("synthetic", "os"):
                usage = row[layer].get("usage")
                if usage:
                    peak = usage["peak_rss_bytes"]
                    lines.append(f'| {row["canary"]} / {layer} | {peak} | {max(0, peak-CAP)} | {SLACK} |')
    lines += ["", "## Signatures, remaining gate requirements and verdict", "", "```json", json.dumps(report["signatures"], indent=2), "```", "",
              "PASS requires every benign fixture and canary under a working OS boundary, all resource ceilings supported, and plan §7.3/P8 signed immutable launcher/app-bundle evidence. This developer spike has no trusted release signature or signed-bundle test. Even a successful `--os` canary run cannot assert full release admission without those receipts. UNKNOWN supplies no new OS admission evidence. ADR 0003 records the existing developer-boundary admission; synthetic reruns do not replace it. Windows is unverified.", "",
              "Source admission conservatively rejects the byte sequence `import` (including comments/strings); no module loader is installed. eval/Function globals and function/async/generator constructor links are removed. Promise intrinsics are required for safe async teardown, but the Promise global is absent, returned promises are refused, and no job queue is drained. SDK objects are recursively frozen and have null prototypes. Rust date/money/formula handling is shared with the finite executor.", ""]
    (ROOT / "docs/build" / evidence_name).write_text("\n".join(lines))


if __name__ == "__main__":
    sys.exit(main())
