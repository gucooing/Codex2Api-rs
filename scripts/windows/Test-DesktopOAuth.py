"""Called by the ignored Rust integration test with an isolated router and database."""
import http.cookiejar
import json
import os
from pathlib import Path
import queue
import re
import subprocess
import sys
import threading
import time
import tomllib
import urllib.parse
import urllib.request

base, client_home = sys.argv[1:]
proxy = base + "/api/oauth/chatgpt"
cli = os.environ["CODEX2API_TEST_CLI"]
client_env = os.environ.copy()
client_env.update({
    "CODEX_HOME": client_home,
    "CODEX_SQLITE_HOME": str(Path(client_home) / "sqlite"),
    "CODEX_APP_SERVER_LOGIN_ISSUER": proxy,
    "CODEX_REFRESH_TOKEN_URL_OVERRIDE": proxy + "/oauth/token",
    "CODEX_REVOKE_TOKEN_URL_OVERRIDE": proxy + "/oauth/revoke",
})
for name in ["OPENAI_API_KEY", "CODEX_API_KEY", "CODEX_ACCESS_TOKEN", "CODEX_CONNECTORS_TOKEN"]:
    client_env.pop(name, None)


class Client:
    def __init__(self):
        self.messages = queue.Queue()
        self.notifications = []
        self.next_id = 0
        arguments = [
            cli, "app-server", "-c", 'cli_auth_credentials_store="file"',
            "-c", 'chatgpt_base_url=' + json.dumps(proxy + "/backend-api"),
            "-c", 'openai_base_url=' + json.dumps(proxy + "/backend-api/codex"),
        ]
        if os.environ.get("CODEX2API_TEST_CUSTOM_PROVIDER"):
            source = Path(client_home, 'native-config-input.json')
            result = Path(client_home, 'native-config-output.json')
            source.write_text(json.dumps(tomllib.loads(Path(client_home, 'config.toml').read_text(encoding='utf-8'))), encoding='utf-8')
            resolved=subprocess.run([os.environ['CODEX2API_TEST_NATIVE_PREPARE'], '--route-config', proxy, str(source), str(result)],capture_output=True,text=True,timeout=18)
            assert resolved.returncode==0,resolved.stderr
            for key, value in json.loads(result.read_text(encoding='utf-8')).items(): arguments += ['-c', key + '=' + json.dumps(value)]
        prepare = os.environ.get('CODEX2API_TEST_NATIVE_PREPARE')
        self.process = subprocess.Popen(arguments, env=client_env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, creationflags=4 if prepare else 0)
        if prepare:
            try:
                subprocess.run([prepare, '--prepare', str(self.process.pid), cli], check=True, capture_output=True, timeout=20)
            except BaseException:
                self.process.kill()
                raise
        threading.Thread(target=self.read, daemon=True).start()
        self.call("initialize", {"clientInfo": {"name": "codex2api_test", "version": "0.1.0"}, "capabilities": {"experimentalApi": True}})
        self.send({"method": "initialized"})

    def read(self):
        for line in self.process.stdout:
            self.messages.put(json.loads(line))

    def send(self, value):
        self.process.stdin.write((json.dumps(value) + "\n").encode())
        self.process.stdin.flush()

    def call(self, method, params):
        self.next_id += 1
        request_id = self.next_id
        self.send({"id": request_id, "method": method, "params": params})
        deadline = time.monotonic() + 20
        while True:
            message = self.messages.get(timeout=max(0.1, deadline - time.monotonic()))
            if message.get("id") == request_id:
                assert "error" not in message, (method, message.get("error"))
                return message.get("result")
            self.notifications.append(message)

    def close(self):
        self.process.stdin.close()
        try:
            self.process.wait(timeout=8)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait()


if os.environ.get("CODEX2API_TEST_CUSTOM_PROVIDER"):
    Path(client_home,"config.toml").write_text('model_provider="custom"\n[model_providers.custom]\nname="OpenAI"\nrequires_openai_auth=true\n',encoding="utf-8")

client = Client()
try:
    login = client.call("account/login/start", {"type":"chatgpt", "codexStreamlinedLogin":True, "useHostedLoginSuccessPage":False})
    authorization=urllib.parse.parse_qs(urllib.parse.urlsplit(login["authUrl"]).query).get("authorize_url", [login["authUrl"]])[0]
    assert urllib.parse.urlsplit(authorization).netloc == urllib.parse.urlsplit(proxy).netloc, 'Native login escaped the configured service'
    callback=urllib.parse.parse_qs(urllib.parse.urlsplit(authorization).query)["redirect_uri"][0]
    allowed_origins={(urllib.parse.urlsplit(value).scheme,urllib.parse.urlsplit(value).netloc) for value in [proxy,callback]}
    class LocalLoginRedirect(urllib.request.HTTPRedirectHandler):
        def redirect_request(self, req, fp, code, msg, headers, newurl):
            target=urllib.parse.urlsplit(newurl)
            assert (target.scheme,target.netloc) in allowed_origins, "Login redirected outside configured proxy/local callback"
            return super().redirect_request(req,fp,code,msg,headers,newurl)
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), LocalLoginRedirect(), urllib.request.HTTPCookieProcessor(http.cookiejar.CookieJar()))
    query=urllib.parse.urlsplit(authorization).query
    with opener.open(proxy + "/oauth/authorize/bootstrap?" + query, timeout=10) as response:
        flow=json.load(response)
    payload=json.dumps({"request_id":flow["request_id"],"csrf_token":flow["csrf_token"],"username":"alice","password":"fixture-password"}).encode()
    with opener.open(urllib.request.Request(proxy+"/oauth/authorize/submit",data=payload,headers={"Content-Type":"application/json"}),timeout=20) as response:
        approved=json.load(response)
    with opener.open(approved["redirect_uri"],timeout=20) as response:
        response.read()
    deadline = time.monotonic() + 20
    while True:
        message = client.notifications.pop(0) if client.notifications else client.messages.get(timeout=max(0.1, deadline-time.monotonic()))
        if message.get("method") == "account/login/completed":
            assert message["params"]["success"], message["params"].get("error")
            break
    account = client.call("account/read", {"refreshToken": False})["account"]
    assert account is not None
    client.call("configRequirements/read", {})
    models=client.call("model/list",{"includeHidden":True})
    if os.environ.get("CODEX2API_TEST_EMPTY_CATALOG"):
        assert models["data"],"Expected the installed client's documented builtin fallback for an empty remote catalog"
        print("Native Desktop observed builtin fallback for an empty authorized remote catalog: "+json.dumps([model["model"] for model in models["data"]]))
    else:
        expected_models={"gpt-5.6-luna","gpt-6-astra","gpt-6-sol","gpt-6-luna"}
        assert {model["model"] for model in models["data"]}==expected_models, [model["model"] for model in models["data"]]
        assert models["nextCursor"] is None,models
        assert all(model["supportedReasoningEfforts"] for model in models["data"]),models
        pages=[]
        cursor=None
        while True:
            page=client.call("model/list",{"includeHidden":True,"limit":1,"cursor":cursor})
            pages.extend(page["data"])
            cursor=page["nextCursor"]
            if cursor is None: break
            assert len(pages)<10,"Model pagination did not terminate"
        assert {model["model"] for model in pages}==expected_models
        if os.environ.get("CODEX2API_TEST_MODEL_OUTPUT"):
            Path(os.environ["CODEX2API_TEST_MODEL_OUTPUT"]).write_text(json.dumps(models["data"]),encoding="utf-8")
        print("Native Desktop model/list decoded the four granted pinned descriptors, including GPT-6 Sol/Luna, with reasoning capabilities and pagination; the Rust fixture also requires an actual proxy catalog fetch.")

    if os.environ.get("CODEX2API_TEST_CUSTOM_PROVIDER"):
        config=client.call("config/read",{"includeLayers":False})["config"]
        assert config["model_provider"]=="custom"
        assert config["model_providers"]["custom"]["base_url"]==proxy+"/backend-api/codex"
        thread=client.call("thread/start",{"cwd":client_home,"model":"gpt-5.6-sol","approvalPolicy":"never","sandbox":"read-only","ephemeral":True,"config":{"model_providers.custom.request_max_retries":0,"model_providers.custom.stream_max_retries":0}})
        assert thread["modelProvider"]=="custom"
        client.call("turn/start",{"threadId":thread["thread"]["id"],"input":[{"type":"text","text":"Reply OK","text_elements":[]}]})
        errors=[];deadline=time.monotonic()+30
        while True:
            item=client.notifications.pop(0) if client.notifications else client.messages.get(timeout=max(.1,deadline-time.monotonic()))
            if item.get("method")=="error":errors.append(item["params"])
            if item.get("method")=="turn/completed":break
            assert time.monotonic()<deadline,"Native test turn did not finish"
        text=json.dumps(errors)
        assert "chatgpt.com/backend-api/codex/responses" not in text,text
        assert "virtual_account_unbound" in text or proxy+"/backend-api/codex/responses" in text,text
        print("Native custom-provider inference reached the local proxy's explicit unbound-account response, with provider/model selection preserved.")
    registered = client.call("remoteControl/enable", {"ephemeral": True})
    deadline = time.monotonic() + 15
    while True:
        remote = client.call("remoteControl/status/read", {})
        if remote["status"] == "connected":
            assert remote["environmentId"], "Native app-server did not enroll a remote host"
            break
        assert time.monotonic() < deadline, remote
        time.sleep(0.2)
    client.call("remoteControl/disable", {"ephemeral": True})
    print("Native Desktop app-server: real enrollment request, response decoding and authenticated WebSocket connection succeeded.")
    # The GUI branch below exercises the original installed readers at runtime.
    # Individual response-selector scripts remain separate endpoint contract tests.
    print("Native desktop CLI: login notification succeeded and config requirements loaded.")
finally:
    client.close()

desktop = os.environ.get("CODEX2API_TEST_DESKTOP_EXE")
if desktop:
    # Only used by the ignored test's temporary router, database and fake credentials.
    launcher=os.environ["CODEX2API_TEST_GUI"]
    options={"executable":desktop,"proxyRoot":proxy,"clientHome":client_home,"appData":str(Path(client_home)/"desktop-app")}
    fixture_path=Path(client_home)/"gui-fixture.json"
    report_path=Path(client_home)/"gui-result.json"
    fixture_path.write_text(json.dumps(options))
    result=subprocess.run([launcher,"--test-launch",str(fixture_path),str(report_path)],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,timeout=70)
    assert result.returncode==0,report_path.read_text() if report_path.exists() else result.stderr
    launched=json.loads(report_path.read_text())
    print('Desktop fixture PID:', launched['processId'], flush=True)
    try:
        assert launched["nativeHookInstalled"]
        time.sleep(12)
        import ctypes
        kernel = ctypes.WinDLL('kernel32', use_last_error=True)
        kernel.OpenProcess.restype = ctypes.c_void_p
        process_handle = kernel.OpenProcess(0x1000, False, launched['processId'])
        assert process_handle, 'Desktop exited after startup'
        try:
            exit_code = ctypes.c_ulong()
            assert kernel.GetExitCodeProcess(ctypes.c_void_p(process_handle), ctypes.byref(exit_code)) and exit_code.value == 259, 'Desktop exited after startup'
        finally:
            kernel.CloseHandle(ctypes.c_void_p(process_handle))
        if os.environ.get('CODEX2API_TEST_NATIVE_PREPARE'):
            rows = json.loads(subprocess.check_output(['powershell', '-NoProfile', '-Command', "@(Get-CimInstance Win32_Process | Select-Object ProcessId,ParentProcessId,Name,ExecutablePath,@{Name='Mode';Expression={if($_.CommandLine -match 'app-server-daemon'){'daemon'}elseif($_.CommandLine -match 'app-server'){'server'}else{'other'}}}) | ConvertTo-Json -Compress"], text=True))
            owned = {launched['processId']}
            for _ in range(12):
                owned.update(row['ProcessId'] for row in rows if row['ParentProcessId'] in owned)
            cores = [row['ProcessId'] for row in rows if row['ProcessId'] in owned and row['Name'].lower() == 'codex.exe']
            print('Desktop native process tree:', [(row['ProcessId'], row['ParentProcessId'], row['Mode']) for row in rows if row['ProcessId'] in cores], flush=True)
            assert cores, 'Desktop did not start its native runtime'
            for core in cores:
                actual_runtime = next(row['ExecutablePath'] for row in rows if row['ProcessId'] == core)
                verified = subprocess.run([os.environ['CODEX2API_TEST_NATIVE_PREPARE'], '--verify', str(core), actual_runtime], capture_output=True, text=True, timeout=20)
                assert verified.returncode == 0, verified.stderr
        print("Native desktop GUI: compiled launcher completed startup; the Rust fixture separately checks real workspace, quota and settings requests.")
    finally:
        subprocess.run(["taskkill", "/PID", str(launched["processId"]), "/T", "/F"], capture_output=True)
        time.sleep(0.5)

client = Client()
try:
    restored = client.call("account/read", {"refreshToken": False})["account"]
    assert restored == account, {"initialType": account.get("type"), "restoredType": restored.get("type") if restored else None, "changedFields": [key for key in set(account) | set(restored or {}) if account.get(key)!=(restored or {}).get(key)]}
    status = client.call("getAuthStatus", {"includeToken": False, "refreshToken": True})
    assert status["authMethod"] == "chatgpt"
    client.call("configRequirements/read", {})
    print("Native desktop CLI: identity persisted across restart and refresh succeeded.")
finally:
    client.close()
