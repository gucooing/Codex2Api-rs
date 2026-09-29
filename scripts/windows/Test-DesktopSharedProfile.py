import argparse
import hashlib
import json
import os
from pathlib import Path
import queue
import subprocess
import tempfile
import threading


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--runtime", required=True)
    parser.add_argument("--fixture", required=True)
    parser.add_argument("--hook", required=True)
    arguments = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="codex-shared-profile-") as temporary:
        root = Path(temporary)
        shared = root / "shared"
        private = root / "service"
        shared.mkdir()
        private.mkdir()
        shared_config = shared / "config.toml"
        shared_config.write_text('model = "shared-model"\ncli_auth_credentials_store = "file"\n', encoding="utf-8")
        private_config = private / "config.toml"
        private_config.write_text('model = "private-model"\ncli_auth_credentials_store = "file"\n', encoding="utf-8")
        original_config = shared_config.read_bytes()
        (shared / "auth.json").write_text('{"OPENAI_API_KEY":"shared-fixture-key"}', encoding="utf-8")
        original_auth = (shared / "auth.json").read_bytes()
        environment = os.environ.copy()
        for key in ["CODEX_HOME", "CODEX_SQLITE_HOME", "CODEX2API_SHARED_HOME", "CODEX2API_PRIVATE_HOME", "CODEX2API_HOOK_SERVER", "OPENAI_API_KEY", "CODEX_API_KEY"]:
            environment.pop(key, None)
        environment.update(CODEX_HOME=str(shared), CODEX2API_SHARED_HOME=str(shared), CODEX2API_PRIVATE_HOME=str(private))
        with (root / "stderr.txt").open("w", encoding="utf-8") as error_log:
            process = subprocess.Popen([arguments.runtime, "app-server"], env=environment, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                       stderr=error_log, text=True, encoding="utf-8", creationflags=0x08000004)
            try:
                subprocess.run([arguments.fixture, "--inject-files", str(process.pid), arguments.hook], check=True, timeout=30)
                messages = queue.Queue()
                def read_messages():
                    for line in process.stdout:
                        messages.put(json.loads(line))
                threading.Thread(target=read_messages, daemon=True).start()
                def request(method, params):
                    process.stdin.write(json.dumps({"id": method, "method": method, "params": params}) + "\n")
                    process.stdin.flush()
                    while True:
                        response = messages.get(timeout=30)
                        if response.get("id") == method:
                            assert "error" not in response, response
                            return response["result"]
                request("initialize", {"clientInfo": {"name": "profile-regression", "version": "1"}, "capabilities": {"experimentalApi": True}})
                process.stdin.write('{"method":"initialized"}\n')
                process.stdin.flush()
                # Rust creates named pipes for child stdio. Config-only tests
                # miss routing bugs that break every shell/MCP/code-mode child.
                executed = request("command/exec", {
                    "command": [str(Path(os.environ["WINDIR"]) / "System32/cmd.exe"), "/c", "echo shared-profile-child-ok"],
                    "cwd": str(root), "sandboxPolicy": {"type": "dangerFullAccess"},
                })
                assert executed["exitCode"] == 0 and "shared-profile-child-ok" in executed["stdout"], executed
                host = request("command/exec", {
                    "command": [str(Path(arguments.runtime).with_name("codex-code-mode-host.exe")), "--help"],
                    "cwd": str(root), "sandboxPolicy": {"type": "dangerFullAccess"},
                })
                assert host["exitCode"] == 0 and "Transport endpoint" in host["stdout"], host
                config = request("config/read", {"includeLayers": True})
                assert config["config"]["model"] == "private-model", config
                account = request("account/read", {"refreshToken": False})
                assert account["account"] is None, "Shared credentials leaked into the hooked process"
                request("config/value/write", {"keyPath": "model", "value": "updated-private-model", "mergeStrategy": "replace"})
                assert "updated-private-model" in private_config.read_text(encoding="utf-8")
                assert shared_config.read_bytes() == original_config, "Shared config was overwritten"
                request("account/login/start", {"type": "apiKey", "apiKey": "private-fixture-key"})
                assert "private-fixture-key" in (private / "auth.json").read_text(encoding="utf-8")
                assert (shared / "auth.json").read_bytes() == original_auth, "Shared credentials were overwritten"
                request("account/logout", {})
                assert not (private / "auth.json").exists(), "Logout failed to remove private credentials"
                assert (shared / "auth.json").read_bytes() == original_auth, "Logout removed shared credentials"
                thread = request("thread/start", {"model": "gpt-6-sol", "cwd": str(root)})["thread"]
                request("thread/inject_items", {"threadId": thread["id"], "items": [{"type": "message", "role": "user", "content": [{"type": "input_text", "text": "Shared profile regression fixture."}]}]})
                request("thread/archive", {"threadId": thread["id"]})
                assert any(shared.glob("*.sqlite*")), "Native state was not written to shared home"
                assert not list(private.glob("*.sqlite*")), "Native state was isolated alongside auth/config"
                process.kill()
                process.wait(timeout=10)
                process.stdin.close()
                process.stdout.close()
                environment.pop("CODEX2API_SHARED_HOME")
                environment.pop("CODEX2API_PRIVATE_HOME")
                process = subprocess.Popen([arguments.runtime, "app-server"], env=environment, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                           stderr=error_log, text=True, encoding="utf-8", creationflags=0x08000000)
                messages = queue.Queue()
                threading.Thread(target=read_messages, daemon=True).start()
                request("initialize", {"clientInfo": {"name": "shared-profile-reader", "version": "1"}, "capabilities": {"experimentalApi": True}})
                process.stdin.write('{"method":"initialized"}\n')
                process.stdin.flush()
                restored = request("thread/read", {"threadId": thread["id"], "includeTurns": False})["thread"]
                assert restored["id"] == thread["id"], "The original client could not read the hooked client's shared session"
                print(json.dumps({"result": "PASS", "runtime": arguments.runtime, "runtime_sha256": hashlib.sha256(Path(arguments.runtime).read_bytes()).hexdigest(),
                                  "checks": ["shell child with captured stdio", "installed code-mode host child", "private config read", "atomic config write", "no shared credential fallback", "private login write", "private logout", "shared native SQLite", "original client reads hooked session"]}))
            finally:
                process.kill()
                process.wait(timeout=10)
                process.stdin.close()
                process.stdout.close()


if __name__ == "__main__":
    main()
