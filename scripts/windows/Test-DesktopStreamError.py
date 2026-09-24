"""Run the installed Desktop app-server against a local Responses relay fixture."""
import base64
import json
import os
from pathlib import Path
import queue
import subprocess
import sys
import tempfile
import threading
import time

with tempfile.TemporaryDirectory(prefix="desktop-stream-error-") as profile:
    enc = lambda value: base64.urlsafe_b64encode(json.dumps(value).encode()).decode().rstrip("=")
    token = enc({"alg": "none"}) + "." + enc({"sub": "fixture", "https://api.openai.com/auth": {"chatgpt_account_id": "fixture", "chatgpt_user_id": "fixture", "chatgpt_plan_type": "pro"}}) + ".fixture"
    Path(profile, "auth.json").write_text(json.dumps({"auth_mode": "chatgpt", "tokens": {"id_token": token, "access_token": token, "refresh_token": "fixture", "account_id": "fixture"}, "last_refresh": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())}))
    env = os.environ.copy()
    env.update(CODEX_HOME=profile, CODEX_SQLITE_HOME=str(Path(profile, "sqlite")))
    for key in ["OPENAI_API_KEY", "CODEX_API_KEY", "CODEX_ACCESS_TOKEN", "CODEX_CONNECTORS_TOKEN"]:
        env.pop(key, None)
    settings = [
        'cli_auth_credentials_store="file"', 'model="gpt-test"',
        'model_provider="fixture"', 'features.responses_websockets_v2=true',
        'model_providers.fixture=' + '{name="fixture",base_url="' + sys.argv[1] + '",wire_api="responses",supports_websockets=true,requires_openai_auth=true,websocket_max_retries=0,request_max_retries=0}',
    ]
    args = [os.environ["CODEX2API_TEST_CLI"], "app-server"]
    for setting in settings:
        args.extend(["-c", setting])
    process = subprocess.Popen(args, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
    messages = queue.Queue()
    def read():
        for line in process.stdout:
            messages.put(json.loads(line))
    threading.Thread(target=read, daemon=True).start()
    def call(index, method, params):
        process.stdin.write((json.dumps({"id": index, "method": method, "params": params}) + "\n").encode())
        process.stdin.flush()
        while True:
            value = messages.get(timeout=20)
            if value.get("id") == index:
                assert "error" not in value, value
                return value["result"]
    try:
        call(1, "initialize", {"clientInfo": {"name": "Codex Desktop", "version": "26.915.31945"}, "capabilities": {"experimentalApi": True}})
        thread = call(2, "thread/start", {"cwd": profile, "model": "gpt-test", "approvalPolicy": "never", "sandbox": "read-only", "ephemeral": True})
        call(3, "turn/start", {"threadId": thread["thread"]["id"], "input": [{"type": "text", "text": "hello", "text_elements": []}]})
        observed = []
        deadline = time.monotonic() + 40
        while time.monotonic() < deadline:
            message = messages.get(timeout=max(.1, deadline - time.monotonic()))
            if message.get("method") == "error":
                observed.append(message["params"])
            if message.get("method") == "turn/completed":
                observed.append(message["params"])
                break
        rendered = json.dumps(observed)
        assert '"httpStatusCode": 429' in rendered, rendered
        assert "Connection reset without closing handshake" not in rendered, rendered
        print("Installed Desktop app-server consumed the relay quota error and ended the turn without a WebSocket reset.")
    finally:
        process.terminate()
        process.wait(timeout=5)
