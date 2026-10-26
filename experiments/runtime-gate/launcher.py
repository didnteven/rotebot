"""Synthetic experiment, deliberately not a production generated-code runner.

Default launch ALWAYS uses Seatbelt. Deno-only fallback is an explicit harness
API, never an automatic downgrade in run-sandboxed.sh. All state stays here.
"""
import hashlib
import ctypes
import json
import os
from pathlib import Path
import resource
import selectors
import shutil
import signal
import subprocess
import sys
import time
import uuid

ROOT = Path(__file__).resolve().parent
RUNS = ROOT / ".runs"
DENO = ROOT / ".runtime/deno"
FLAGS = ["run", "--no-prompt", "--no-remote", "--no-npm", "--no-config",
         "--no-lock", "--cached-only", "--v8-flags=--max-old-space-size=200"]
RSS_LIMIT = 256 * 1024 * 1024
CPU_LIMIT = 30
WALL_LIMIT = 120
OUT_LIMIT = 20_000_000  # MB, as specified; stderr and scratch use MiB.
ERR_LIMIT = 1024 * 1024
DISK_LIMIT = 32 * 1024 * 1024
POLL_SECONDS = 0.1


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def verified_runtime():
    lock = json.loads((ROOT / "runtime.lock.json").read_text())
    if sha(DENO) != lock["binary_sha256"]:
        raise RuntimeError("Pinned Deno binary digest mismatch")


class BsdInfo(ctypes.Structure):
    # sys/proc_info.h, proc_bsdinfo, macOS arm64 (136 bytes).
    _fields_ = [(n, ctypes.c_uint32) for n in (
        "flags", "status", "xstatus", "pid", "ppid", "uid", "gid", "ruid", "rgid",
        "svuid", "svgid", "reserved")] + [
        ("comm", ctypes.c_char * 16), ("name", ctypes.c_char * 32)] + [
        (n, ctypes.c_uint32) for n in ("nfiles", "pgid", "jobc", "tdev", "tpgid", "nice")] + [
        ("start_sec", ctypes.c_uint64), ("start_usec", ctypes.c_uint64)]


class TaskInfo(ctypes.Structure):
    _fields_ = [(n, ctypes.c_uint64) for n in (
        "virtual", "resident", "user", "system", "threads_user", "threads_system")] + [
        ("rest", ctypes.c_int32 * 12)]


LIBPROC = ctypes.CDLL("/usr/lib/libproc.dylib", use_errno=True)
LIBPROC.proc_listpids.argtypes = [ctypes.c_uint32, ctypes.c_uint32, ctypes.c_void_p, ctypes.c_int]
LIBPROC.proc_pidinfo.argtypes = [ctypes.c_int, ctypes.c_int, ctypes.c_uint64, ctypes.c_void_p, ctypes.c_int]
assert ctypes.sizeof(BsdInfo) == 136 and ctypes.sizeof(TaskInfo) == 96
class Timebase(ctypes.Structure):
    _fields_ = [("numer", ctypes.c_uint32), ("denom", ctypes.c_uint32)]


TIMEBASE = Timebase()
LIBSYSTEM = ctypes.CDLL("/usr/lib/libSystem.B.dylib")
if LIBSYSTEM.mach_timebase_info(ctypes.byref(TIMEBASE)) != 0:
    raise RuntimeError("Cannot establish Mach CPU timebase")


def listed_pids(kind, target):
    values = (ctypes.c_int * 4096)()
    count = LIBPROC.proc_listpids(kind, target, values, ctypes.sizeof(values))
    if count < 0 or count >= ctypes.sizeof(values):
        raise OSError("libproc process enumeration failed or overflowed")
    return {v for v in values[:count // ctypes.sizeof(ctypes.c_int)] if v > 0}


def processes(root_pid, known):
    # /bin/ps cannot execute under this enclosing sandbox. Read metadata only
    # for our process group, known descendants and recursively listed children.
    rows = {}
    pending = {root_pid} | known | listed_pids(2, root_pid)  # PROC_PGRP_ONLY
    seen = set()
    while pending:
        pid = pending.pop()
        if pid in seen:
            continue
        seen.add(pid)
        bsd = BsdInfo()
        got = LIBPROC.proc_pidinfo(pid, 3, 0, ctypes.byref(bsd), ctypes.sizeof(bsd))
        if got != ctypes.sizeof(bsd):
            if ctypes.get_errno() in (3, 0):  # ESRCH or already gone
                continue
            raise OSError(ctypes.get_errno(), "Cannot inspect child process metadata")
        task = TaskInfo()
        got = LIBPROC.proc_pidinfo(pid, 4, 0, ctypes.byref(task), ctypes.sizeof(task))
        if got != ctypes.sizeof(task) and bsd.status != 5:  # SZOMB
            if ctypes.get_errno() == 3:
                continue
            raise OSError(ctypes.get_errno(), "Cannot inspect child RSS/CPU")
        rows[pid] = dict(ppid=bsd.ppid, pgid=bsd.pgid, rss=task.resident,
                         cpu=(task.user + task.system) * TIMEBASE.numer / TIMEBASE.denom / 1e9)
        pending.update(listed_pids(6, pid) - seen)  # PROC_PPID_ONLY
    return rows


def tree(rows, root_pid, known):
    # Retain observed descendants even if reparented or in a new process group.
    members = {pid for pid, row in rows.items()
               if pid == root_pid or row["pgid"] == root_pid or pid in known}
    while True:
        expanded = members | {pid for pid, row in rows.items() if row["ppid"] in members}
        if expanded == members:
            return members
        members = expanded


def freeze(package):
    # Preserve malicious symlinks so that the OS loader canary really exercises
    # them. A production admission copier must REJECT them (PLAN §7.3).
    for path in package.rglob("*"):
        if not path.is_symlink():
            path.chmod(0o555 if path.is_dir() else 0o444)
    package.chmod(0o555)


def setup_child():
    os.setsid()
    resource.setrlimit(resource.RLIMIT_CPU, (CPU_LIMIT, CPU_LIMIT))
    resource.setrlimit(resource.RLIMIT_CORE, (0, 0))


def run(package_dir, entry, *, seatbelt=True, input_bytes=b'{"value":21}\n', replacements=None):
    verified_runtime()
    source = Path(package_dir).resolve(strict=True)
    relative = Path(entry)
    if relative.is_absolute() or ".." in relative.parts or not (source / relative).is_file():
        raise ValueError("Entry must be a file below package-dir")
    # This harness accepts only its synthetic canaries, never arbitrary home data.
    if not source.is_relative_to(ROOT):
        raise ValueError("Synthetic packages must be inside experiments/runtime-gate")
    RUNS.mkdir(exist_ok=True)
    work = RUNS / uuid.uuid4().hex
    work.mkdir(mode=0o700)
    package = work / "package"
    scratch = work / "scratch"
    scratch.mkdir(mode=0o700)
    shutil.copytree(source, package, symlinks=True)
    for path in package.rglob("*"):
        if path.is_symlink():
            target = os.readlink(path)
            for key, value in (replacements or {}).items():
                target = target.replace(key, value)
            path.unlink()
            path.symlink_to(target)
    for path in package.rglob("*.ts"):
        if not path.is_symlink():
            content = path.read_text()
            for key, value in (replacements or {}).items():
                content = content.replace(key, value)
            path.write_text(content)
    manifest = {str(p.relative_to(package)): ("symlink:" + os.readlink(p)
                if p.is_symlink() else sha(p)) for p in package.rglob("*") if not p.is_dir()}
    freeze(package)
    env = {"DENO_DIR": str(scratch), "NO_COLOR": "1", "HOME": str(scratch)}
    argv = [str(DENO), *FLAGS, str(package / relative)]
    if seatbelt:
        argv = ["/usr/bin/sandbox-exec", "-D", f"DENO={DENO}", "-D", f"PACKAGE={package}",
                "-D", f"SCRATCH={scratch}", "-f", str(ROOT / "profile.sb"), *argv]
    started = time.monotonic()
    p = subprocess.Popen(argv, cwd=package, env=env, stdin=subprocess.PIPE,
                         stdout=subprocess.PIPE, stderr=subprocess.PIPE, preexec_fn=setup_child)
    # Canary broker input is small and fixed, never copied from the ambient env.
    p.stdin.write(input_bytes)
    p.stdin.close()
    selector = selectors.DefaultSelector()
    for pipe, label in ((p.stdout, "stdout"), (p.stderr, "stderr")):
        os.set_blocking(pipe.fileno(), False)
        selector.register(pipe, selectors.EVENT_READ, label)
    buffers = {"stdout": bytearray(), "stderr": bytearray()}
    total = {"stdout": 0, "stderr": 0}
    caps = {"stdout": OUT_LIMIT, "stderr": ERR_LIMIT}
    reason = None
    known = set()
    cpu_by_pid = {}
    peak_rss = peak_disk = 0
    max_poll_gap = 0.0
    last_poll = started
    next_poll = started
    kill_at = None
    leader_usage = None
    signal_errors = []

    def kill(why):
        nonlocal reason, kill_at
        if reason is None:
            reason, kill_at = why, time.monotonic()
        try:
            os.killpg(p.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        except PermissionError as error:
            # The enclosing sandbox can return EPERM, rather than ESRCH, for
            # an already-dead process group while buffered flood bytes drain.
            # Signal individual owned targets too; survivor checks remain strict.
            if len(signal_errors) < 10:
                signal_errors.append(f"killpg: {error}")
        for pid in known | ({p.pid} if p.returncode is None else set()):
            try:
                os.kill(pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            except PermissionError as error:
                if len(signal_errors) < 10:
                    signal_errors.append(f"kill({pid}): {error}")

    while selector.get_map() or p.returncode is None:
        if p.returncode is None:
            waited, status, usage = os.wait4(p.pid, os.WNOHANG)
            if waited:
                p.returncode = os.waitstatus_to_exitcode(status)
                leader_usage = usage
        now = time.monotonic()
        if now >= next_poll:
            max_poll_gap = max(max_poll_gap, now - last_poll)
            last_poll, next_poll = now, now + POLL_SECONDS
            try:
                rows = processes(p.pid, known)
                members = tree(rows, p.pid, known)
                known.update(members)
                rss = sum(rows[pid]["rss"] for pid in members)
                peak_rss = max(peak_rss, rss)
                for pid in members:
                    cpu_by_pid[pid] = max(cpu_by_pid.get(pid, 0), rows[pid]["cpu"])
                disk = subprocess.run(["/usr/bin/du", "-sk", str(scratch)],
                                      capture_output=True, text=True, check=True, timeout=3)
                peak_disk = max(peak_disk, int(disk.stdout.split()[0]) * 1024)
                if rss > RSS_LIMIT:
                    kill("rss")
                elif sum(cpu_by_pid.values()) >= CPU_LIMIT:
                    kill("cpu")
                elif peak_disk > DISK_LIMIT:
                    kill("scratch")
            except (subprocess.SubprocessError, ValueError, OSError) as error:
                kill("monitor-error:" + str(error))
        if now - started >= WALL_LIMIT:
            kill("wall")
        if kill_at is not None and now - kill_at > 5:
            # A descendant retaining a pipe cannot hang the trusted harness.
            for key in list(selector.get_map().values()):
                selector.unregister(key.fileobj)
                key.fileobj.close()
            if p.returncode is not None:
                break
        for key, _ in selector.select(timeout=max(0, min(0.02, next_poll - time.monotonic()))):
            chunk = os.read(key.fileobj.fileno(), 65536)
            if not chunk:
                selector.unregister(key.fileobj)
                key.fileobj.close()
                continue
            label = key.data
            total[label] += len(chunk)
            remaining = caps[label] - len(buffers[label])
            buffers[label].extend(chunk[:remaining])
            if total[label] > caps[label]:
                kill(label)
    p.wait(timeout=5)  # wait4 above already reaped and populated returncode.
    selector.close()
    # Kill group members even after a clean leader exit, then verify absence.
    rows = processes(p.pid, known)
    remaining = tree(rows, p.pid, known)
    if remaining:
        kill("descendants-after-exit")
    for _ in range(20):
        rows = processes(p.pid, known)
        remaining = tree(rows, p.pid, known)
        if not remaining:
            break
        time.sleep(0.01)
    code = p.returncode
    final_disk = subprocess.run(["/usr/bin/du", "-sk", str(scratch)], capture_output=True,
                                text=True, check=True, timeout=3)
    final_disk_bytes = int(final_disk.stdout.split()[0]) * 1024
    peak_disk = max(peak_disk, final_disk_bytes)
    if reason is None and (code == -signal.SIGXCPU or
                           (code == -signal.SIGKILL and sum(cpu_by_pid.values()) >= 29)):
        reason = "cpu-os"
    result = dict(mode="seatbelt" if seatbelt else "deno-permissions-only", argv=argv, env=env,
                  package_manifest=manifest, exit_code=code, kill_reason=reason,
                  wall_seconds=round(time.monotonic() - started, 3),
                  process_tree_peak_rss_bytes=peak_rss, rss_overshoot_bytes=max(0, peak_rss-RSS_LIMIT),
                  leader_kernel_peak_rss_bytes=leader_usage.ru_maxrss if leader_usage else None,
                  leader_kernel_cpu_seconds=round(leader_usage.ru_utime + leader_usage.ru_stime, 3) if leader_usage else None,
                  rss_overshoot_lower_bound_bytes=max(0, peak_rss-RSS_LIMIT,
                      (leader_usage.ru_maxrss if leader_usage else 0)-RSS_LIMIT),
                  tree_cpu_seconds=round(sum(cpu_by_pid.values()), 3),
                  scratch_peak_allocated_bytes=peak_disk,
                  scratch_final_allocated_bytes=final_disk_bytes,
                  scratch_overshoot_bytes=max(0, peak_disk-DISK_LIMIT),
                  max_poll_gap_seconds=round(max_poll_gap, 4),
                  stdout_bytes_seen=total["stdout"], stderr_bytes_seen=total["stderr"],
                  stdout_bytes_retained=len(buffers["stdout"]), stderr_bytes_retained=len(buffers["stderr"]),
                  descendants_remaining=sorted(remaining), leader_reaped=True,
                  signal_errors=signal_errors,
                  stdout=buffers["stdout"].decode("utf-8", "replace"),
                  stderr=buffers["stderr"].decode("utf-8", "replace"))
    # Bounded captures: keep full capped bytes in ignored run dir, not results.json.
    for label in buffers:
        (work / (label + ".bin")).write_bytes(buffers[label])
    receipt = dict(result)
    receipt["stdout"] = result["stdout"][:1000]
    receipt["stderr"] = result["stderr"][:1000]
    (work / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
    result["receipt"] = str(work / "receipt.json")
    return result


if __name__ == "__main__":
    if len(sys.argv) != 3:
        raise SystemExit("usage: launcher.py <package-dir> <entry.ts>")
    broker_input = sys.stdin.buffer.read(1024 * 1024 + 1)
    if len(broker_input) > 1024 * 1024:
        raise SystemExit("Broker input exceeds 1 MiB")
    result = run(sys.argv[1], sys.argv[2], input_bytes=broker_input)
    # Flood captures are never relayed to a caller as a valid broker message.
    if result["kill_reason"] is None and result["exit_code"] == 0:
        try:
            message = json.loads(result["stdout"])
        except ValueError:
            sys.stderr.write("Invalid single JSON broker response\n")
            raise SystemExit(65)
        print(json.dumps(message, separators=(",", ":")))
    sys.stderr.write(result["stderr"])
    sys.stderr.write(f"\nReceipt: {result['receipt']}\n")
    raise SystemExit(1 if result["kill_reason"] else (result["exit_code"] if result["exit_code"] >= 0 else 128-result["exit_code"]))
