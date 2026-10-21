"""Verify the installed Desktop app-server's full-prompt retry against a local WS fixture."""
import json
import os
from pathlib import Path
import queue
import subprocess
import tempfile
import threading
import time

from websockets.sync.server import serve


def verify(cli, error_code):
    frames = []
    injected = threading.Event()
    replayed = threading.Event()

    def upstream(socket):
        try:
            for raw in socket:
                frame = json.loads(raw)
                if frame.get("type") != "response.create":
                    continue
                frames.append(frame)
                if frame.get("previous_response_id") and not injected.is_set():
                    injected.set()
                    socket.send(json.dumps({"type": "error", "status": 400, "error": {
                        "type": "invalid_request_error", "code": error_code,
                        "message": "Invalid `previous_response_id`."}}))
                    socket.close(1008, "Responses connection ended")
                    return
                if injected.is_set():
                    assert not frame.get("previous_response_id"), "Client replayed the expired anchor"
                    assert "continuation-fixture" in json.dumps(frame["input"]), "Full prompt missing"
                    replayed.set()
                response_id = "resp_fixture_" + str(len(frames))
                output = [] if frame.get("generate") is False else [{
                    "id": "msg_fixture_" + str(len(frames)), "type": "message",
                    "role": "assistant", "status": "completed",
                    "content": [{"type": "output_text", "text": "ok", "annotations": []}],
                }]
                socket.send(json.dumps({"type": "response.created", "response": {"id": response_id}}))
                socket.send(json.dumps({"type": "response.completed", "response": {
                    "id": response_id, "status": "completed", "output": output,
                    "usage": {"input_tokens": 10, "output_tokens": 1, "total_tokens": 11},
                }}))
        except Exception as error:
            if error.__class__.__name__ not in ("ConnectionClosedOK", "ConnectionClosedError"):
                raise

    with serve(upstream, "127.0.0.1", 0) as server, tempfile.TemporaryDirectory(prefix="desktop-continuation-") as profile:
        server_thread = threading.Thread(target=server.serve_forever, daemon=True)
        server_thread.start()
        port = server.socket.getsockname()[1]
        env = os.environ.copy()
        env.update(CODEX_HOME=profile, CODEX_SQLITE_HOME=str(Path(profile, "sqlite")))
        for key in ("OPENAI_API_KEY", "CODEX_API_KEY", "CODEX_ACCESS_TOKEN", "CODEX_CONNECTORS_TOKEN"):
            env.pop(key, None)
        args = [cli, "app-server"]
        for setting in [
            'cli_auth_credentials_store="file"', 'model="gpt-test"', 'model_provider="fixture"',
            'features.responses_websockets_v2=true',
            'model_providers.fixture={name="fixture",base_url="http://127.0.0.1:' + str(port)
            + '",wire_api="responses",supports_websockets=true,requires_openai_auth=false,stream_max_retries=2,websocket_max_retries=2}',
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
                value = messages.get(timeout=max(.1, deadline - time.monotonic()))
                if value.get("id") == index:
                    assert "error" not in value, value
                    return value["result"]
                deferred.append(value)

        statuses = []
        try:
            call(1, "initialize", {"clientInfo": {"name": "continuation-test", "version": "1"}, "capabilities": {"experimentalApi": True}})
            thread = call(2, "thread/start", {"cwd": profile, "model": "gpt-test", "approvalPolicy": "never", "sandbox": "read-only", "ephemeral": True})
            for index in range(3, 6):
                call(index, "turn/start", {"threadId": thread["thread"]["id"], "input": [{"type": "text", "text": "continuation-fixture " + str(index), "text_elements": []}]})
                deadline = time.monotonic() + 40
                while True:
                    value = deferred.pop(0) if deferred else messages.get(timeout=max(.1, deadline - time.monotonic()))
                    if value.get("method") == "turn/completed":
                        statuses.append(value["params"]["turn"]["status"])
                        break
                if injected.is_set():
                    break
            assert injected.is_set(), "Installed client did not issue a continuation request"
            if error_code == "previous_response_not_found":
                assert replayed.is_set() and statuses[-1] == "completed", statuses
            else:
                assert not replayed.is_set() and statuses[-1] == "failed", statuses
            return {"error_code": error_code, "requests": len(frames), "full_prompt_retried": replayed.is_set(), "turn_statuses": statuses}
        finally:
            if os.name == "nt":
                subprocess.run(["taskkill", "/PID", str(process.pid), "/T", "/F"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False)
            else:
                process.terminate()
            process.wait(timeout=5)
            server.shutdown()


if __name__ == "__main__":
    cli = os.environ["CODEX2API_TEST_CLI"]
    print(json.dumps({"runtime": subprocess.check_output([cli, "--version"], text=True).strip(),
                      "cases": [verify(cli, code) for code in ("invalid_request_error", "previous_response_not_found")]}))
