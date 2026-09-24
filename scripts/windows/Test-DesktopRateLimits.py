"""Exercise the installed Desktop's bundled app-server against proxy response fixtures."""
import base64
import http.server
import json
import os
from pathlib import Path
import queue
import subprocess
import sys
import tempfile
import threading
import time

sample = json.loads(sys.stdin.read().lstrip("\ufeff"))
cli = os.environ["CODEX2API_TEST_CLI"]

def run(installed, expect_invalid=False):
    hits = []
    class Handler(http.server.BaseHTTPRequestHandler):
        def do_GET(self):
            hits.append(self.path)
            value = installed
            data = json.dumps(value).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(data)
        def log_message(self, *_):
            pass
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    try:
        with tempfile.TemporaryDirectory(prefix="desktop-plugin-contract-") as home:
            enc = lambda v: base64.urlsafe_b64encode(json.dumps(v).encode()).decode().rstrip("=")
            token = enc({"alg": "none"}) + "." + enc({"sub": "user-fixture", "https://api.openai.com/auth": {"chatgpt_account_id": "fixture", "chatgpt_user_id": "user-fixture", "chatgpt_plan_type": "pro"}}) + ".fixture"
            Path(home, "auth.json").write_text(json.dumps({"auth_mode": "chatgpt", "tokens": {"id_token": token, "access_token": token, "refresh_token": "fixture", "account_id": "fixture"}, "last_refresh": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())}))
            env = os.environ.copy()
            env.update(CODEX_HOME=home, CODEX_SQLITE_HOME=str(Path(home, "sqlite")))
            for key in ["OPENAI_API_KEY", "CODEX_API_KEY", "CODEX_ACCESS_TOKEN", "CODEX_CONNECTORS_TOKEN"]:
                env.pop(key, None)
            process = subprocess.Popen([cli, "app-server", "-c", 'cli_auth_credentials_store="file"', "-c", f'chatgpt_base_url="http://127.0.0.1:{server.server_port}/backend-api"'], env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
            messages = queue.Queue()
            def read():
                for line in process.stdout:
                    messages.put(json.loads(line))
            threading.Thread(target=read, daemon=True).start()
            def call(index, method, params):
                process.stdin.write((json.dumps({"id": index, "method": method, "params": params}) + "\n").encode())
                process.stdin.flush()
                deadline = time.monotonic() + 15
                while True:
                    value = messages.get(timeout=max(.1, deadline-time.monotonic()))
                    if value.get("id") == index:
                        return value
            try:
                assert "error" not in call(1, "initialize", {"clientInfo": {"name": "Codex Desktop", "version": "26.915.31945"}, "capabilities": {"experimentalApi": True}})
                result = call(2, "account/rateLimits/read", {})
                assert any(path.endswith("/wham/usage") for path in hits), hits
                if expect_invalid:
                    assert "expected i32" in json.dumps(result), result
                    return result
                assert "error" not in result, result
                limits = result["result"]["rateLimits"]
                for key, field, minutes in [("primary", "primary_window", 300), ("secondary", "secondary_window", 10080)]:
                    raw = installed["rate_limit"][field]
                    if raw is None:
                        assert limits[key] is None, limits
                    else:
                        assert limits[key]["windowDurationMins"] == minutes, limits
                        assert limits[key]["usedPercent"] == raw["used_percent"], limits
                        assert limits[key]["resetsAt"] == raw["reset_at"], limits
                return result
            finally:
                process.terminate()
                process.wait(timeout=5)
    finally:
        server.shutdown()

import copy
old=copy.deepcopy(sample)
for window in [old["rate_limit"]["primary_window"],old["rate_limit"]["secondary_window"]]:
    if window is not None:
        window["used_percent"]=float(window["used_percent"])
if any(window is not None for window in [old["rate_limit"]["primary_window"],old["rate_limit"]["secondary_window"]]):
    run(old,expect_invalid=True)
run(sample)
print("Installed Desktop app-server successfully reads the actual virtual-account 5h/week rate limit response.")
