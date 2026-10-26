#!/usr/bin/env python3
"""Print a compact, human-readable timeline of an msp_probe transcript."""
import json
import sys


def brief(msg):
    m = msg.get("method")
    p = msg.get("params", {}) or {}
    if m in ("item/started", "item/updated", "item/completed"):
        it = p.get("item", {})
        extra = {k: it[k] for k in ("tool", "status", "text", "commandText", "exitCode", "visibleOutput", "fallbackText", "failureKind", "failureReason", "event", "label", "runStatus", "approvalId", "args") if k in it}
        s = json.dumps(extra)
        return f"{m} kind={it.get('kind')} rev={it.get('revision')} {s[:300]}"
    if m == "item/delta":
        return f"item/delta field={p.get('field','text')} delta={json.dumps(p.get('delta'))[:200]}"
    if m == "turn/completed":
        return f"turn/completed terminal={p.get('terminal')} error={json.dumps(p.get('error'))} reason={json.dumps(p.get('reason'))} usage={json.dumps(p.get('usage'))} dur={p.get('durationMs')} ttft={p.get('timeToFirstTokenMs')}"
    if m in ("session/tokenUsage",):
        return f"{m} model={p.get('modelId')} usage={json.dumps(p.get('usage'))} cumulative={json.dumps(p.get('cumulative'))} dur={p.get('durationMs')} finish={p.get('finishReason')}"
    if m:
        q = {k: v for k, v in p.items() if k not in ("sourceRange", "viewCursor", "sessionId")}
        tag = " [server-request id=%s]" % msg["id"] if "id" in msg else ""
        return f"{m}{tag} {json.dumps(q)[:400]}"
    if "result" in msg:
        return f"result id={msg['id']} {json.dumps(msg['result'])[:400]}"
    if "error" in msg:
        return f"ERROR id={msg.get('id')} {json.dumps(msg['error'])[:400]}"
    return json.dumps(msg)[:300]


for line in open(sys.argv[1]):
    r = json.loads(line)
    d = r["dir"]
    if d in ("s2c", "c2s"):
        try:
            msg = json.loads(r["raw"])
        except Exception:
            print(f"{r['t_ms']:>7} {d} RAW {r['raw'][:200]}")
            continue
        if d == "c2s":
            if "method" in msg:
                print(f"{r['t_ms']:>7} -> {msg['method']}{' id=%s' % msg['id'] if 'id' in msg else ''} {json.dumps(msg.get('params', {}))[:250]}")
            else:
                print(f"{r['t_ms']:>7} -> response id={msg.get('id')} {json.dumps(msg.get('result', msg.get('error')))[:200]}")
        else:
            print(f"{r['t_ms']:>7} <- {brief(msg)}")
    elif d == "stderr":
        print(f"{r['t_ms']:>7} !! stderr {r['raw'][:300]}")
    else:
        print(f"{r['t_ms']:>7} ## {d} {json.dumps(r.get('obj', r.get('raw')))[:400]}")
