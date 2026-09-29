"""Exercise the installed native app-server against the Rust Responses fixture.

Uses an isolated temporary profile, and never modifies the installed client.
"""
import json
import os
from pathlib import Path
import queue
import subprocess
import sys
import tempfile
import threading
import time

cli = os.environ["CODEX2API_TEST_CLI"]
version = subprocess.check_output([cli, "--version"], text=True).strip()
base_url, expected = sys.argv[1:3]
websocket = len(sys.argv) > 3 and sys.argv[3] == "websocket"
with tempfile.TemporaryDirectory(prefix="desktop-response-outcomes-") as profile:
    env = os.environ.copy()
    env.update(CODEX_HOME=profile, CODEX_SQLITE_HOME=str(Path(profile, "sqlite")))
    for key in ["OPENAI_API_KEY", "CODEX_API_KEY", "CODEX_ACCESS_TOKEN", "CODEX_CONNECTORS_TOKEN"]:
        env.pop(key, None)
    args = [cli, "app-server"]
    for setting in [
        'cli_auth_credentials_store="file"', 'model="gpt-test"', 'model_provider="fixture"',
        'features.responses_websockets_v2=' + str(websocket).lower(),
        'model_providers.fixture={name="fixture",base_url="' + base_url + '",wire_api="responses",supports_websockets=' + str(websocket).lower() + ',requires_openai_auth=false,request_max_retries=0,stream_max_retries=0,websocket_max_retries=0}',
    ]:
        args.extend(["-c", setting])
    process = subprocess.Popen(args, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
    messages = queue.Queue()
    def read():
        for line in process.stdout:
            messages.put(json.loads(line))
    threading.Thread(target=read, daemon=True).start()
    deferred = []
    def call(index, method, params):
        process.stdin.write((json.dumps({"id": index, "method": method, "params": params}) + "\n").encode())
        process.stdin.flush()
        deadline = time.monotonic() + 20
        while True:
            value = messages.get(timeout=max(.1, deadline-time.monotonic()))
            if value.get("id") == index:
                assert "error" not in value, value
                return value["result"]
            deferred.append(value)
    try:
        call(1, "initialize", {"clientInfo": {"name": "response-contract-test", "version": "1"}, "capabilities": {"experimentalApi": True}})
        thread = call(2, "thread/start", {"cwd": profile, "model": "gpt-test", "approvalPolicy": "never", "sandbox": "read-only", "ephemeral": True})
        call(3, "turn/start", {"threadId": thread["thread"]["id"], "input": [{"type": "text", "text": "fixture", "text_elements": []}]})
        deadline = time.monotonic() + 40
        observed = []
        while True:
            value = deferred.pop(0) if deferred else messages.get(timeout=max(.1, deadline-time.monotonic()))
            if value.get("method") == "error":
                observed.append(value["params"])
            if value.get("method") == "turn/completed":
                observed.append(value["params"])
                break
        rendered = json.dumps(observed)
        assert expected.lower() in rendered.lower(), rendered
        assert '"status": "failed"' in rendered, rendered
        assert "Connection reset without closing handshake" not in rendered, rendered
        print(json.dumps({"runtime": version, "expected": expected, "observed": observed}))
    finally:
        process.terminate()
        process.wait(timeout=5)
