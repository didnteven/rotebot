#!/usr/bin/env python3
"""MSP (Muse Session Protocol) v1 probe client for Macroscope planning.

Spawns `muse serve`, speaks newline-delimited JSON-RPC 2.0 over stdio, and
records every raw frame (both directions, plus stderr) to a JSONL transcript.

Usage examples:
  python3 msp_probe.py --scenario basic   --out transcripts/echo-basic.jsonl -- --provider echo --no-session-log
  python3 msp_probe.py --scenario badkey  --out transcripts/meta-badkey.jsonl --env META_API_KEY=sk-invalid-test-key \
      -- --provider meta --no-session-log

Nothing in here is product code; it is a planning probe.
"""
import argparse
import json
import os
import queue
import secrets
import subprocess
import sys
import tempfile
import threading
import time


def uuid7() -> str:
    ms = int(time.time() * 1000)
    rand = secrets.token_bytes(10)
    b = ms.to_bytes(6, "big") + bytes([0x70 | (rand[0] & 0x0F), rand[1]]) + bytes([0x80 | (rand[2] & 0x3F)]) + rand[3:10]
    h = b.hex()
    return f"{h[0:8]}-{h[8:12]}-{h[12:16]}-{h[16:20]}-{h[20:32]}"


class Probe:
    def __init__(self, serve_args, env_extra, out_path, approve_choice="approve-once"):
        env = dict(os.environ)
        # Never let an ambient key leak into a probe unless explicitly passed.
        env.pop("META_API_KEY", None)
        env.update(env_extra)
        self.t0 = time.time()
        self.out = open(out_path, "w")
        self.lock = threading.Lock()
        self.next_id = 1
        self.pending = {}  # id -> queue
        self.events = queue.Queue()  # notifications + server requests
        self.approve_choice = approve_choice
        cmd = ["muse", "serve"] + serve_args
        self.log("meta", {"cmd": cmd, "env_overrides": {k: ("<redacted>" if "KEY" in k else v) for k, v in env_extra.items()}})
        self.proc = subprocess.Popen(cmd, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env, cwd=tempfile.gettempdir())
        threading.Thread(target=self._read_stdout, daemon=True).start()
        threading.Thread(target=self._read_stderr, daemon=True).start()

    def log(self, direction, payload):
        rec = {"t_ms": round((time.time() - self.t0) * 1000), "dir": direction}
        if isinstance(payload, str):
            rec["raw"] = payload
        else:
            rec["obj"] = payload
        with self.lock:
            self.out.write(json.dumps(rec) + "\n")
            self.out.flush()

    def _read_stdout(self):
        for line in self.proc.stdout:
            line = line.decode("utf-8", "replace").rstrip("\n")
            if not line:
                continue
            self.log("s2c", line)
            try:
                msg = json.loads(line)
            except Exception:
                continue
            if "id" in msg and ("result" in msg or "error" in msg) and "method" not in msg:
                q = self.pending.pop(msg["id"], None)
                if q:
                    q.put(msg)
            else:
                self.events.put(msg)
        self.events.put({"__eof__": True})

    def _read_stderr(self):
        for line in self.proc.stderr:
            self.log("stderr", line.decode("utf-8", "replace").rstrip("\n"))

    def send_raw(self, obj):
        line = json.dumps(obj)
        self.log("c2s", line)
        self.proc.stdin.write((line + "\n").encode())
        self.proc.stdin.flush()

    def request(self, method, params=None, timeout=30):
        rid = self.next_id
        self.next_id += 1
        q = queue.Queue()
        self.pending[rid] = q
        frame = {"jsonrpc": "2.0", "id": rid, "method": method}
        if params is not None:
            frame["params"] = params
        self.send_raw(frame)
        try:
            return q.get(timeout=timeout)
        except queue.Empty:
            return {"timeout": True}

    def notify(self, method, params=None):
        frame = {"jsonrpc": "2.0", "method": method}
        if params is not None:
            frame["params"] = params
        self.send_raw(frame)

    def respond(self, rid, result):
        self.send_raw({"jsonrpc": "2.0", "id": rid, "result": result})

    def pump_until(self, pred, timeout=60):
        """Drain events, auto-handling server requests, until pred(msg) is True."""
        deadline = time.time() + timeout
        seen = []
        while time.time() < deadline:
            try:
                msg = self.events.get(timeout=max(0.05, deadline - time.time()))
            except queue.Empty:
                break
            if msg.get("__eof__"):
                return None, seen
            seen.append(msg)
            if msg.get("method") == "approval/request" and "id" in msg:
                self.handle_approval(msg)
            elif msg.get("method") == "userInput/request" and "id" in msg:
                self.respond(msg["id"], {})
                p = msg["params"]
                self.request("userInput/cancel", {"commandId": uuid7(), "sessionId": p["sessionId"], "userInputId": p["userInputId"], "reason": "probe"})
            if pred(msg):
                return msg, seen
        return None, seen

    def handle_approval(self, msg):
        p = msg["params"]
        # 1) presentation receipt (the response to the server-initiated request)
        self.respond(msg["id"], {})
        # 2) decision travels as a separate command
        choices = p.get("availableChoices", [])
        pick = None
        for c in choices:
            if self.approve_choice == "deny" and c["decision"] == "denied":
                pick = c
                break
            if self.approve_choice == "approve-once" and c["decision"] == "approved":
                pick = c
                break
        pick = pick or (choices[0] if choices else None)
        if pick is None:
            return
        res = self.request("approval/decide", {
            "sessionId": p["sessionId"],
            "approvalId": p["approvalId"],
            "requirementId": p["currentRequirementId"],
            "choiceId": pick["choiceId"],
            "commandId": uuid7(),
        })
        self.log("note", {"approval_decided_with": pick, "result": res})

    def close(self, wait=10):
        try:
            self.proc.stdin.close()
        except Exception:
            pass
        try:
            rc = self.proc.wait(timeout=wait)
        except subprocess.TimeoutExpired:
            self.proc.kill()
            rc = "killed"
        time.sleep(0.2)
        self.log("meta", {"exit": rc})
        self.out.close()
        return rc


def is_turn_done(msg):
    return msg.get("method") in ("turn/completed", "turn/unqueued")


def make_workspace():
    d = tempfile.mkdtemp(prefix="msp-probe-ws-")
    subprocess.run(["git", "init", "-q", d], check=False)
    with open(os.path.join(d, "README.md"), "w") as f:
        f.write("probe workspace\n")
    return os.path.realpath(d)


def handshake(p, caps=None):
    params = {"clientInfo": {"name": "macroscope_probe", "title": "Macroscope probe", "version": "0.0.1"}}
    if caps is not None:
        params["capabilities"] = caps
    init = p.request("initialize", params)
    p.notify("initialized")
    return init


def start_session(p, ws, approval_mode=None, model=None):
    params = {"commandId": uuid7(), "workspaceRoot": ws}
    if approval_mode:
        params["approvalMode"] = approval_mode
    if model:
        params["modelId"] = model
    return p.request("session/start", params)


def turn(p, sid, text, effort=None, timeout=120):
    params = {"commandId": uuid7(), "sessionId": sid, "input": [{"type": "text", "text": text}]}
    if effort:
        params["reasoningEffort"] = effort
    t = time.time()
    ack = p.request("turn/start", params)
    done, seen = p.pump_until(is_turn_done, timeout=timeout)
    p.log("note", {"turn_wall_ms": round((time.time() - t) * 1000), "ack": ack, "terminal": done})
    return ack, done, seen


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--scenario", default="basic")
    ap.add_argument("--out", required=True)
    ap.add_argument("--env", action="append", default=[])
    ap.add_argument("--prompt", default="hello from macroscope")
    ap.add_argument("--approval-mode", default=None)
    ap.add_argument("--approve", default="approve-once", choices=["approve-once", "deny", "first"])
    ap.add_argument("--model", default=None)
    ap.add_argument("serve_args", nargs="*")
    a = ap.parse_args()
    env_extra = dict(kv.split("=", 1) for kv in a.env)
    p = Probe(a.serve_args, env_extra, a.out, approve_choice=a.approve)
    caps = {"requestedCapabilities": ["userShell", "sessionListStream", "sessionRename", "feedback"]}

    if a.scenario == "fork":
        handshake(p, caps)
        ws = make_workspace()
        st = start_session(p, ws, approval_mode="promptUnmatched")
        sid = st["result"]["session"]["sessionId"]
        _, d1, _ = turn(p, sid, "approach A: write a python script")
        t1 = d1["params"]["turnId"]
        turn(p, sid, "approach A: add tests")
        fk = p.request("session/fork", {"commandId": uuid7(), "sessionId": sid, "cutPoint": {"lastTurnId": t1}, "excludeItems": True})
        fk = p.request("session/fork", {"commandId": uuid7(), "sessionId": sid, "excludeItems": True})
        fsid = fk.get("result", {}).get("session", {}).get("sessionId")
        if fsid:
            turn(p, fsid, "approach B: write a shell script instead")
        p.request("session/read", {"sessionId": sid})
        p.request("session/list", {"limit": 5})
        p.request("session/rename", {"commandId": uuid7(), "sessionId": sid, "name": "csv-cleanup build 1"})
        p.pump_until(lambda m: False, timeout=1.5)
        p.close()
        # restart host, resume the original session
        q = Probe(a.serve_args, env_extra, a.out.replace(".jsonl", "-resume.jsonl"))
        handshake(q, caps)
        r = q.request("session/resume", {"commandId": uuid7(), "sessionId": sid, "history": "snapshot"})
        q.request("approval/listPending", {"sessionId": sid})
        turn(q, sid, "continue after host restart")
        q.pump_until(lambda m: False, timeout=1.5)
        q.close()
        return

    if a.scenario == "crash":
        handshake(p, caps)
        ws = make_workspace()
        st = start_session(p, ws)
        sid = st["result"]["session"]["sessionId"]
        p.request("turn/start", {"commandId": uuid7(), "sessionId": sid, "input": [{"type": "text", "text": "slow reply please"}]})
        p.pump_until(lambda m: m.get("method") == "turn/started", timeout=10)
        p.log("note", {"action": "SIGKILL muse serve mid-turn"})
        p.proc.kill()
        p.close()
        q = Probe(a.serve_args, env_extra, a.out.replace(".jsonl", "-after.jsonl"))
        handshake(q, caps)
        q.request("session/read", {"sessionId": sid})
        q.request("session/resume", {"commandId": uuid7(), "sessionId": sid, "excludeItems": True})
        turn(q, sid, "are you still there?", timeout=30)
        q.close()
        return

    if a.scenario == "sandbox":
        handshake(p, caps)
        ws = make_workspace()
        st = start_session(p, ws)
        sid = st["result"]["session"]["sessionId"]
        cmd = os.environ.get("PROBE_CMD") or "curl -s -m 5 -o /dev/null -w 'net=%{http_code}\\n' https://example.com; rm -f /tmp/msp-outside-write; touch /tmp/msp-outside-write && echo wrote-outside || echo outside-write-denied; touch ./inside && echo wrote-inside"
        for attempt in range(10):
            r = p.request("session/userShell", {"commandId": uuid7(), "sessionId": sid, "commandText": cmd})
            if "error" in r and r["error"].get("data", {}).get("reason") == "runtime_busy":
                p.pump_until(lambda m: False, timeout=1.0)
                continue
            break
        p.pump_until(lambda m: m.get("method") == "item/completed" and m["params"]["item"].get("kind") == "userShell", timeout=30)
        p.close()
        return

    if a.scenario == "worktree":
        handshake(p, caps)
        ws = make_workspace()
        subprocess.run(["git", "-C", ws, "add", "."], check=False)
        subprocess.run(["git", "-C", ws, "-c", "user.email=p@p", "-c", "user.name=p", "commit", "-qm", "init"], check=False)
        wt = ws + "-attempt-b"
        subprocess.run(["git", "-C", ws, "worktree", "add", "-q", "-b", "attempt-b", wt], check=False)
        wt = os.path.realpath(wt)
        st = start_session(p, ws)
        sid = st["result"]["session"]["sessionId"]
        turn(p, sid, "approach A")
        fk = p.request("session/fork", {"commandId": uuid7(), "sessionId": sid, "excludeItems": True})
        fsid = fk["result"]["session"]["sessionId"]
        params = {"commandId": uuid7(), "sessionId": fsid, "input": [{"type": "text", "text": "approach B in its own worktree"}], "workspaceRoots": [wt]}
        p.request("turn/start", params)
        p.pump_until(lambda m: m.get("method") == "turn/completed" and m["params"].get("sessionId") == fsid, timeout=30)
        for attempt in range(10):
            r = p.request("session/userShell", {"commandId": uuid7(), "sessionId": fsid, "commandText": "pwd; git branch --show-current"})
            if "error" in r and r["error"].get("data", {}).get("reason") == "runtime_busy":
                p.pump_until(lambda m: False, timeout=1.0)
                continue
            break
        p.pump_until(lambda m: m.get("method") == "item/completed" and m["params"]["item"].get("kind") == "userShell", timeout=20)
        p.close()
        return

    if a.scenario == "account":
        init = handshake(p, {"experimentalApi": True})
        p.request("account/read", {})
        p.request("account/loginStart", {"type": "apiKey", "apiKey": "sk-invalid-test-key-2"})
        p.pump_until(lambda m: m.get("method") == "account/loginCompleted", timeout=20)
        p.request("account/read", {})
        ws = make_workspace()
        st = start_session(p, ws)
        sid = st.get("result", {}).get("session", {}).get("sessionId")
        if sid:
            turn(p, sid, "reply OK", timeout=60)
        p.request("account/logout", {})
        p.pump_until(lambda m: False, timeout=2)
        p.request("account/read", {})
        p.close()
        return

    if a.scenario == "framing-negative":
        # Notification before initialize; request before initialize; garbage line.
        p.send_raw({"jsonrpc": "2.0", "id": 99, "method": "session/list", "params": {}})
        p.proc.stdin.write(b"this is not json\n"); p.proc.stdin.flush(); p.log("c2s", "this is not json")
        time.sleep(1.0)
        handshake(p, caps)
        p.request("initialize", {"clientInfo": {"name": "again", "version": "0"}})
        p.request("no/such/method", {})
        time.sleep(0.5)
        p.close()
        return

    init = handshake(p, caps)
    print("initialize ->", json.dumps(init)[:600], file=sys.stderr)
    ws = make_workspace()

    if a.scenario in ("basic", "badkey", "goodkey", "approval", "features"):
        ml = p.request("model/list", {})
        st = start_session(p, ws, approval_mode=a.approval_mode, model=a.model)
        sid = st.get("result", {}).get("session", {}).get("sessionId")
        print("session/start ->", json.dumps(st)[:400], file=sys.stderr)
        if not sid:
            p.pump_until(lambda m: False, timeout=2)
            p.close()
            return
        ack, done, seen = turn(p, sid, a.prompt, effort=("none" if a.scenario == "goodkey" else None))
        print("turn terminal ->", json.dumps(done)[:800], file=sys.stderr)
        if a.scenario == "features":
            # Exercise the non-turn surface the app would use.
            for attempt in range(10):
                r = p.request("session/userShell", {"commandId": uuid7(), "sessionId": sid, "commandText": "echo hi-from-userShell && ls -a"})
                if "error" in r and r["error"].get("data", {}).get("reason") == "runtime_busy":
                    p.pump_until(lambda m: False, timeout=1.0)
                    continue
                break
            p.pump_until(lambda m: m.get("method") == "item/completed" and m["params"]["item"].get("kind") == "userShell", timeout=20)
            p.request("goal/set", {"commandId": uuid7(), "sessionId": sid, "objective": "Build a CSV-cleanup micro-app"})
            p.pump_until(lambda m: False, timeout=3)
            p.request("goal/pause", {"commandId": uuid7(), "sessionId": sid})
            p.pump_until(lambda m: False, timeout=2)
            p.request("skill/list", {"sessionId": sid})
            p.request("plugin/list", {"sessionId": sid})
            p.request("hook/list", {"sessionId": sid})
            p.request("task/list", {"sessionId": sid})
            p.request("session/tokenUsage", {"sessionId": sid})  # notification name, expect methodNotFound
            p.request("usage/read", {})
            last_turn = (done or {}).get("params", {}).get("turnId")
            fk = p.request("session/fork", {"commandId": uuid7(), "sessionId": sid, **({"cutPoint": {"lastTurnId": last_turn}} if last_turn else {})})
            fsid = fk.get("result", {}).get("session", {}).get("sessionId")
            if fsid:
                turn(p, fsid, "second approach", timeout=30)
            p.request("session/list", {})
            p.request("session/read", {"sessionId": sid, "excludeItems": False})
        p.pump_until(lambda m: False, timeout=1.5)
    p.close()


if __name__ == "__main__":
    main()
