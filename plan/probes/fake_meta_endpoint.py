#!/usr/bin/env python3
"""Local stand-in for the Meta API, used to learn what muse sends (path, headers).

It always answers 401, so nothing is ever billed. It logs each request line plus
header NAMES, and the Authorization scheme with a masked value, to stdout as JSONL.
Usage: python3 -I fake_meta_endpoint.py <port> <logfile>
"""
import json
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

PORT = int(sys.argv[1])
LOG = open(sys.argv[2], "a")


class H(BaseHTTPRequestHandler):
    def _handle(self):
        n = int(self.headers.get("content-length") or 0)
        body = self.rfile.read(n) if n else b""
        auth = self.headers.get("authorization", "")
        scheme, _, val = auth.partition(" ")
        rec = {
            "method": self.command,
            "path": self.path,
            "header_names": sorted(self.headers.keys()),
            "auth_scheme": scheme,
            "auth_value_masked": (val[:6] + "..." + val[-4:]) if val else "",
            "user_agent": self.headers.get("user-agent"),
            "body_keys": sorted(json.loads(body).keys()) if body.startswith(b"{") else None,
            "model": (json.loads(body).get("model") if body.startswith(b"{") else None),
        }
        LOG.write(json.dumps(rec) + "\n")
        LOG.flush()
        payload = json.dumps({"error": {"message": "fake endpoint: unauthorized", "type": "authentication_error", "code": 401}}).encode()
        self.send_response(401)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

    do_GET = _handle
    do_POST = _handle

    def log_message(self, *a):
        pass


ThreadingHTTPServer(("127.0.0.1", PORT), H).serve_forever()
