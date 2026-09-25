"""Run original app-server code against supplied proxy fixtures over local HTTPS.

No CODEX_HOME, credential-store, profile, or installed-binary override. The normal
external-token RPC keeps fixture credentials in this process's memory. Threads
are ephemeral; inference is served by this fixture, never by an online provider.
"""
import base64
import datetime
import hashlib
import http.server
import ipaddress
import json
import os
from pathlib import Path
import queue
import ssl
import struct
import subprocess
import sys
import tempfile
import threading
import time
from cryptography import x509
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import rsa
from cryptography.x509.oid import NameOID

sample = json.loads(sys.stdin.read().lstrip("\ufeff"))
requests = []
messages = queue.Queue()
events = []
counter = 0

def completion():
    global counter
    counter += 1
    response = {"id": f"resp_fixture_{counter}", "status": "completed", "output": [],
                "usage": {"input_tokens": 1, "output_tokens": 1, "total_tokens": 2}}
    return {"type": "response.completed", "response": response}

class Handler(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    def handle(self):
        try:
            super().handle()
        except (ConnectionResetError, BrokenPipeError, ssl.SSLEOFError):
            # The app-server tears down pooled and warmup sockets at shutdown.
            pass
    def log_message(self, *_):
        pass

    def reply(self, value, status=200):
        data = json.dumps(value).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        path = self.path.split("?")[0]
        requests.append({"method": "GET", "path": path})
        if self.headers.get("Upgrade", "").lower() == "websocket":
            if sample.get("ws_error"):
                return self.reply({"error": {"code": "fixture_upgrade_required", "message": "Use HTTP for this fixture"}}, sample["ws_error"])
            return self.websocket()
        if path.endswith("/accounts/check"):
            self.reply(sample["workspace"])
        elif path.endswith("/wham/usage"):
            self.reply(sample["quota"])
        elif path.endswith("/codex/models"):
            self.reply(sample["models"])
        elif path.endswith("/config/bundle"):
            self.reply(sample.get("config", {}))
        elif path.endswith("/settings/user"):
            self.reply(sample.get("settings", {"settings": {}, "flags": {}}))
        elif path.endswith("/ps/plugins/installed"):
            self.reply(sample.get("plugins", {"plugins": [], "pagination": {"limit": 1000, "next_page_token": None}}))
        elif path in sample.get("extra_routes", {}):
            route = sample["extra_routes"][path]
            self.reply(route["body"], route["status"])
        else:
            self.reply({"error": {"code": "fixture_unimplemented", "message": "Fixture has no source for this endpoint"}}, 404)

    def do_POST(self):
        body = self.rfile.read(int(self.headers.get("Content-Length", 0)))
        path = self.path.split("?")[0]
        requests.append({"method": "POST", "path": path})
        if path.endswith("/responses"):
            data = ("event: response.completed\ndata: " + json.dumps(completion()) + "\n\n").encode()
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)
        elif path.endswith("/rate-limit-reset-credits/consume") and sample.get("reset_cases"):
            request = json.loads(body)
            requests[-1]["body"] = request
            response = sample["reset_cases"][request["redeem_request_id"]]
            if response["code"] == "reset":
                sample["quota"] = sample["quota_after_reset"]
                sample["extra_routes"]["/backend-api/wham/rate-limit-reset-credits"]["body"] = sample["credits_after_reset"]
            self.reply(response)
        else:
            self.reply({"error": {"code": "fixture_unimplemented", "message": "Fixture has no source for this endpoint"}}, 404)

    def websocket(self):
        key = self.headers["Sec-WebSocket-Key"] + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11"
        self.send_response(101)
        self.send_header("Upgrade", "websocket")
        self.send_header("Connection", "Upgrade")
        self.send_header("Sec-WebSocket-Accept", base64.b64encode(hashlib.sha1(key.encode()).digest()).decode())
        self.end_headers()
        def send(opcode, payload):
            prefix = bytes([0x80 | opcode])
            prefix += bytes([len(payload)]) if len(payload) < 126 else b"\x7e" + struct.pack("!H", len(payload))
            self.wfile.write(prefix + payload)
            self.wfile.flush()
        while True:
            head = self.rfile.read(2)
            if len(head) != 2:
                return
            opcode, size = head[0] & 15, head[1] & 127
            if size == 126:
                size = struct.unpack("!H", self.rfile.read(2))[0]
            elif size == 127:
                size = struct.unpack("!Q", self.rfile.read(8))[0]
            assert size < 64 * 1024 * 1024
            mask = self.rfile.read(4) if head[1] & 128 else None
            payload = self.rfile.read(size)
            if mask:
                payload = bytes(value ^ mask[index % 4] for index, value in enumerate(payload))
            if opcode == 8:
                send(8, payload)
                return
            if opcode == 9:
                send(10, payload)
            elif opcode in (1, 2):
                value = json.loads(payload)
                requests.append({"method": "WS", "path": self.path, "type": value.get("type"), "generate": value.get("generate")})
                send(1, json.dumps(completion()).encode())

with tempfile.TemporaryDirectory(prefix="codex-native-contract-") as scratch:
    scratch = Path(scratch)
    key = rsa.generate_private_key(public_exponent=65537, key_size=2048)
    subject = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, "localhost")])
    now = datetime.datetime.now(datetime.timezone.utc)
    cert = (x509.CertificateBuilder().subject_name(subject).issuer_name(subject)
            .public_key(key.public_key()).serial_number(x509.random_serial_number())
            .not_valid_before(now - datetime.timedelta(minutes=1)).not_valid_after(now + datetime.timedelta(days=1))
            .add_extension(x509.BasicConstraints(ca=False, path_length=None), critical=True)
            .add_extension(x509.SubjectAlternativeName([x509.DNSName("localhost"), x509.IPAddress(ipaddress.ip_address("127.0.0.1"))]), critical=False)
            .sign(key, hashes.SHA256()))
    cert_path, key_path = scratch / "fixture.pem", scratch / "fixture-key.pem"
    cert_path.write_bytes(cert.public_bytes(serialization.Encoding.PEM))
    key_path.write_bytes(key.private_bytes(serialization.Encoding.PEM, serialization.PrivateFormat.PKCS8, serialization.NoEncryption()))
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    context.load_cert_chain(cert_path, key_path)
    server.socket = context.wrap_socket(server.socket, server_side=True)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    root = f"https://localhost:{server.server_port}/backend-api"
    cli = os.environ["CODEX2API_TEST_NATIVE_UPDATE"]
    standalone = "app-server" in Path(cli).name
    args = [cli] + ([] if standalone else ["app-server"])
    args += ["-c", 'model_provider="openai"', "-c", f'chatgpt_base_url="{root}"', "-c", f'openai_base_url="{root}/codex"']
    env = os.environ.copy()
    env["CODEX_CA_CERTIFICATE"] = str(cert_path)
    # Only this fixture process trusts its temporary local TLS certificate.
    process = subprocess.Popen(args, cwd=scratch, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                               creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
    def read_messages():
        for line in process.stdout:
            try:
                messages.put(json.loads(line))
            except json.JSONDecodeError:
                pass
    threading.Thread(target=read_messages, daemon=True).start()
    def send(value):
        process.stdin.write((json.dumps(value) + "\n").encode())
        process.stdin.flush()
    def call(index, method, params):
        send({"id": index, "method": method, "params": params})
        deadline = time.monotonic() + 35
        while time.monotonic() < deadline:
            value = messages.get(timeout=max(.1, deadline - time.monotonic()))
            if value.get("id") == index:
                assert "error" not in value, (method, value.get("error"))
                return value.get("result")
            events.append(value)
        raise AssertionError(f"{method} timed out")
    try:
        initialized = call(1, "initialize", {"clientInfo": {"name": "Codex Desktop", "version": "contract"}, "capabilities": {"experimentalApi": True}})
        send({"method": "initialized"})
        call(2, "account/login/start", {"type": "chatgptAuthTokens", "accessToken": sample["access_token"], "chatgptAccountId": sample["account_id"], "chatgptPlanType": "pro"})
        account = call(3, "account/read", {})
        quota = call(4, "account/rateLimits/read", {})
        assert quota["rateLimits"]["primary"]["usedPercent"] == sample["quota"]["rate_limit"]["primary_window"]["used_percent"]
        catalog = call(5, "model/list", {"includeHidden": True})
        model = sample["models"]["models"][0]["slug"]
        assert any(row["model"] == model for row in catalog["data"])
        if sample.get("reset_cases"):
            summary = quota["rateLimitResetCredits"]
            assert summary["availableCount"] == sample["credits_before_reset"]["available_count"], summary
            # CLI 0.157 returns detailed cards; older Desktop native can return the summary only.
            if summary.get("credits") is not None:
                assert summary["credits"][0]["id"] == sample["credits_before_reset"]["credits"][0]["id"]
                assert summary["credits"][0]["resetType"] == "codexRateLimits"
            for index, (key, expected) in enumerate(sample["reset_cases"].items(), 50):
                result = call(index, "account/rateLimitResetCredit/consume", {"idempotencyKey": key})
                outcome = {"reset":"reset","already_redeemed":"alreadyRedeemed","no_credit":"noCredit","nothing_to_reset":"nothingToReset"}[expected["code"]]
                assert result["outcome"] == outcome, result
            refreshed = call(60, "account/rateLimits/read", {})
            assert refreshed["rateLimits"]["primary"]["usedPercent"] == 0
            assert refreshed["rateLimitResetCredits"]["availableCount"] == sample["credits_after_reset"]["available_count"]
            assert any(r.get("body", {}).get("redeem_request_id") == "reset-use" for r in requests), requests
        if sample.get("generation", True):
            thread = call(6, "thread/start", {"cwd": str(scratch), "model": model, "ephemeral": True})
            call(7, "turn/start", {"threadId": thread["thread"]["id"], "input": [{"type": "text", "text": "Reply with fixture text.", "text_elements": []}]})
            deadline = time.monotonic() + 35
            while not any(event.get("method") == "turn/completed" for event in events):
                events.append(messages.get(timeout=max(.1, deadline - time.monotonic())))
                assert time.monotonic() < deadline, "generation did not complete"
            completed = next(event for event in events if event.get("method") == "turn/completed")
            assert completed["params"]["turn"]["status"] == "completed", completed
            assert any(r["method"] == "POST" and r["path"].endswith("/responses") or r["method"] == "WS" and r.get("generate") is not False for r in requests)
        assert any(r["path"].endswith("/accounts/check") for r in requests)
        print(json.dumps({"client": initialized.get("userAgent"), "requests": requests, "native_readers": ["account/read", "account/rateLimits/read", "model/list"], "turn_completed": bool(sample.get("generation", True)), "auth": "process-local external fixture tokens", "profile": "default; no override"}))
    finally:
        if sys.exc_info()[0] is not None:
            print(json.dumps({"captured_requests": requests}), file=sys.stderr)
        process.terminate()
        process.wait(timeout=10)
        server.shutdown()
        server.server_close()
