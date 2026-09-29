"""Run the installed app-server/code-mode host with the launcher's file hook.

The local Responses fixture requests a real shell tool and checks its output on
the following request. No production credentials, database or inference is used.
"""
import argparse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import queue
import subprocess
import tempfile
import threading
import time


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--runtime", required=True)
    parser.add_argument("--fixture", required=True)
    parser.add_argument("--hook", required=True)
    arguments = parser.parse_args()
    requests = []
    marker = "desktop-native-tool-execution-ok"

    class Responses(BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_POST(self):
            body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
            requests.append(body)
            if len(requests) == 1:
                item = {"type": "custom_tool_call", "id": "tool-fixture", "call_id": "call-fixture", "name": "exec",
                        "input": 'text(await tools.exec_command({cmd:"echo ' + marker + '",shell:"cmd.exe",login:false}));'}
            else:
                item = {"type": "message", "id": "message-fixture", "role": "assistant", "status": "completed",
                        "content": [{"type": "output_text", "text": "Tool fixture completed.", "annotations": []}]}
            response = {"id": "response-" + str(len(requests)), "object": "response", "status": "completed",
                        "output": [item], "usage": {"input_tokens": 1, "output_tokens": 1, "total_tokens": 2}}
            events = [{"type": "response.created", "response": {**response, "status": "in_progress", "output": []}},
                      {"type": "response.output_item.done", "output_index": 0, "item": item},
                      {"type": "response.completed", "response": response}]
            data = "".join("event: " + event["type"] + "\ndata: " + json.dumps(event) + "\n\n" for event in events).encode()
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)

    server = ThreadingHTTPServer(("127.0.0.1", 0), Responses)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    try:
        with tempfile.TemporaryDirectory(prefix="desktop-tool-execution-") as temporary:
            root = Path(temporary)
            shared, private = root / "shared", root / "private"
            shared.mkdir()
            private.mkdir()
            (private / "config.toml").write_text('cli_auth_credentials_store="file"\n', encoding="utf-8")
            environment = os.environ.copy()
            for key in ["CODEX_SQLITE_HOME", "CODEX2API_HOOK_SERVER", "OPENAI_API_KEY", "CODEX_API_KEY", "CODEX_ACCESS_TOKEN", "CODEX_CONNECTORS_TOKEN"]:
                environment.pop(key, None)
            environment.update(CODEX_HOME=str(shared), CODEX2API_SHARED_HOME=str(shared), CODEX2API_PRIVATE_HOME=str(private))
            command = [arguments.runtime, "app-server"]
            for setting in ['features.code_mode_host=true', 'model_provider="fixture"', 'features.responses_websockets_v2=false',
                            'model_providers.fixture={name="fixture",base_url="http://127.0.0.1:' + str(server.server_port)
                            + '",wire_api="responses",requires_openai_auth=false,request_max_retries=0,stream_max_retries=0}']:
                command.extend(["-c", setting])
            with (root / "stderr.txt").open("w", encoding="utf-8") as stderr:
                process = subprocess.Popen(command, env=environment, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=stderr,
                                           text=True, encoding="utf-8", creationflags=0x08000004)
                try:
                    subprocess.run([arguments.fixture, "--inject-files", str(process.pid), arguments.hook], check=True, timeout=30)
                    messages = queue.Queue()
                    deferred = []

                    def read():
                        for line in process.stdout:
                            messages.put(json.loads(line))

                    threading.Thread(target=read, daemon=True).start()

                    def call(method, params):
                        process.stdin.write(json.dumps({"id": method, "method": method, "params": params}) + "\n")
                        process.stdin.flush()
                        deadline = time.monotonic() + 30
                        while True:
                            message = messages.get(timeout=max(.1, deadline - time.monotonic()))
                            if message.get("id") == method:
                                assert "error" not in message, message
                                return message["result"]
                            deferred.append(message)

                    call("initialize", {"clientInfo": {"name": "tool-regression", "version": "1"}, "capabilities": {"experimentalApi": True}})
                    process.stdin.write('{"method":"initialized"}\n')
                    process.stdin.flush()
                    thread = call("thread/start", {"model": "gpt-6-astra", "cwd": str(root), "approvalPolicy": "never",
                                                   "sandbox": "danger-full-access", "ephemeral": True})["thread"]
                    call("turn/start", {"threadId": thread["id"], "input": [{"type": "text", "text": "Run the local tool fixture.", "text_elements": []}]})
                    deadline = time.monotonic() + 40
                    while True:
                        message = deferred.pop(0) if deferred else messages.get(timeout=max(.1, deadline - time.monotonic()))
                        if message.get("method") == "turn/completed":
                            assert message["params"]["turn"]["status"] == "completed", message
                            break
                    assert len(requests) == 2, "Expected tool output to reach the next Responses request"
                    outputs = [item for item in requests[1]["input"] if item.get("type") == "custom_tool_call_output"]
                    assert any(marker in str(item.get("output")) and '"exit_code":0' in str(item.get("output")).replace(" ", "") for item in outputs), outputs
                    print(json.dumps({"result": "PASS", "checks": ["installed app-server turn", "native code-mode execution", "real shell command", "tool output in next Responses request", "completed turn"]}))
                finally:
                    process.kill()
                    process.wait(timeout=10)
                    process.stdin.close()
                    process.stdout.close()
    finally:
        server.shutdown()
        server.server_close()


if __name__ == "__main__":
    main()
