#!/usr/bin/env python3
"""Benchmark CLI sync against a disposable local API with simulated request latency.

Example: python3 scripts/benchmark-sync.py --binary target/debug/skl \
  --baseline /tmp/skl-perf-baseline --api-base http://localhost:18787
Fixtures are written to the local API; point it at a disposable database.
"""
import argparse
import http.server
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import tempfile
import threading
import time
import urllib.error
import urllib.parse
import urllib.request
import uuid


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--api-base", default="http://localhost:18787")
    parser.add_argument("--binary", default="target/debug/skl")
    parser.add_argument("--baseline")
    parser.add_argument("--skills", type=int, default=20)
    parser.add_argument("--files", type=int, default=4)
    parser.add_argument("--latency-ms", type=float, default=50)
    args = parser.parse_args()
    if urllib.parse.urlparse(args.api_base).hostname not in ("localhost", "127.0.0.1", "::1"):
        parser.error("use a disposable local API")
    lock = threading.Lock()
    counts = {"requests": 0, "active": 0, "peak": 0}

    class Proxy(http.server.BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"

        def log_message(self, *_):
            pass

        def forward(self):
            with lock:
                counts["requests"] += 1
                counts["active"] += 1
                counts["peak"] = max(counts["peak"], counts["active"])
            try:
                time.sleep(args.latency_ms / 1000)
                body = self.rfile.read(int(self.headers.get("Content-Length", 0)))
                headers = {k: v for k, v in self.headers.items() if k.lower() not in ("host", "connection", "content-length")}
                request = urllib.request.Request(args.api_base.rstrip("/") + self.path,
                                                 data=body if self.command in ("PUT", "POST") else None,
                                                 headers=headers, method=self.command)
                try:
                    response = urllib.request.urlopen(request, timeout=120)
                except urllib.error.HTTPError as error:
                    response = error
                with response:
                    payload = response.read()
                    self.send_response(response.status)
                    for key, value in response.headers.items():
                        if key.lower() not in ("content-length", "transfer-encoding", "connection"):
                            self.send_header(key, value)
                    self.send_header("Content-Length", str(len(payload)))
                    self.end_headers()
                    self.wfile.write(payload)
                    self.wfile.flush()
            finally:
                with lock:
                    counts["active"] -= 1

        do_GET = do_POST = do_PUT = do_DELETE = forward

    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Proxy)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    proxy_base = f"http://127.0.0.1:{server.server_port}"
    try:
        for label, binary in [("baseline", args.baseline), ("optimized", args.binary)]:
            if not binary:
                continue
            binary = str(Path(binary).resolve())
            fixture = uuid.uuid4().hex
            with tempfile.TemporaryDirectory(prefix="skl-perf-") as root:
                root = Path(root)

                def machine(name, seed):
                    data, config = root / name / "data", root / name / "config"
                    data.mkdir(parents=True)
                    config.mkdir()
                    sqlite3.connect(data / "state.db").close()
                    (config / "config.toml").write_text("[sync]\nauto = false\n")
                    if seed:
                        for i in range(args.skills):
                            skill = data / "skills" / f"perf-{i}"
                            skill.mkdir(parents=True)
                            for j in range(args.files):
                                filename = "SKILL.md" if j == 0 else f"file-{j}.md"
                                (skill / filename).write_text(f"# Performance fixture {fixture} {i} {j}\n")
                    return {**os.environ, "SKL_DATA_DIR": str(data), "SKL_CONFIG_DIR": str(config), "SKL_TOKEN": f"dev:perf-{fixture}"}

                sender, receiver = machine("sender", True), machine("receiver", False)
                for scenario, env in [("upload", sender), ("unchanged", sender), ("download", receiver)]:
                    with lock:
                        counts.update(requests=0, peak=0)
                    start = time.perf_counter()
                    result = subprocess.run([binary, "--api-base", proxy_base, "sync"], env=env,
                                            cwd=root, capture_output=True, text=True, timeout=180)
                    elapsed = time.perf_counter() - start
                    if result.returncode:
                        raise RuntimeError(result.stderr)
                    print(json.dumps({"binary": label, "scenario": scenario, "seconds": round(elapsed, 3),
                                      "requests": counts["requests"], "peak_concurrency": counts["peak"],
                                      "latency_ms": args.latency_ms, "skills": args.skills, "files_per_skill": args.files}), flush=True)
    finally:
        server.shutdown()
        server.server_close()


if __name__ == "__main__":
    main()
