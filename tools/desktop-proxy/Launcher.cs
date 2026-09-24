using System.Diagnostics;
using System.Net;
using System.Net.Sockets;
using System.Reflection;
using System.Text.Json;

namespace DesktopProxy;

internal static class Launcher
{
    public static string NormalizeServer(string value)
    {
        if (!Uri.TryCreate(value.Trim(), UriKind.Absolute, out var uri) || uri.Scheme is not ("http" or "https") || uri.UserInfo.Length > 0 || uri.Query.Length > 0 || uri.Fragment.Length > 0)
            throw new InvalidOperationException("请输入有效的 HTTP(S) 服务地址，不要包含密码、查询参数或片段。");
        var root = uri.AbsoluteUri.TrimEnd('/');
        return uri.AbsolutePath == "/" ? root + "/api/oauth/chatgpt" : root;
    }
    public static ProcessStartInfo StartInfo(ClientInstallation client, string server, int port)
    {
        var start = new ProcessStartInfo(client.Executable) { UseShellExecute = false, WorkingDirectory = Path.GetDirectoryName(client.Executable)!, CreateNoWindow = true };
        start.ArgumentList.Add($"--inspect-brk=127.0.0.1:{port}");
        foreach (var name in new[] { "CODEX_HOME", "CODEX_SQLITE_HOME", "CODEX_ELECTRON_USER_DATA_PATH", "CODEX_CLI_PATH", "CODEX2API_REAL_CLI", "CODEX2API_PROXY_ROOT", "CODEX_APP_SERVER_WS_URL", "CODEX_APP_SERVER_FORCE_CLI" }) start.Environment.Remove(name);
        start.Environment["CODEX_API_BASE_URL"] = server + "/backend-api";
        start.Environment["CODEX_APP_SERVER_CHATGPT_BASE_URL"] = server + "/backend-api";
        start.Environment["CODEX_APP_SERVER_OPENAI_BASE_URL"] = server + "/backend-api/codex";
        start.Environment["CODEX_APP_SERVER_LOGIN_ISSUER"] = server;
        start.Environment["CODEX_REFRESH_TOKEN_URL_OVERRIDE"] = server + "/oauth/token";
        start.Environment["CODEX_REVOKE_TOKEN_URL_OVERRIDE"] = server + "/oauth/revoke";
        return start;
    }
    public static string HookSource()
    {
        using var stream = Assembly.GetExecutingAssembly().GetManifestResourceStream("AddressHook")!;
        using var reader = new StreamReader(stream);
        return reader.ReadToEnd();
    }
    public static void CheckInstallation(ClientInstallation client)
    {
        using var stream = File.OpenRead(client.Archive);
        using var reader = new BinaryReader(stream);
        reader.ReadUInt32(); var headerSize = reader.ReadUInt32(); reader.ReadUInt32(); var jsonSize = reader.ReadUInt32();
        if (jsonSize > 16 * 1024 * 1024 || jsonSize > headerSize) throw new InvalidOperationException("无法识别客户端安装包格式。");
        using var header = JsonDocument.Parse(reader.ReadBytes((int)jsonSize));
        var entries = header.RootElement.GetProperty("files").GetProperty(".vite").GetProperty("files").GetProperty("build").GetProperty("files");
        var required = new HashSet<string>(["CODEX_API_BASE_URL", "CODEX_APP_SERVER_CHATGPT_BASE_URL", "CODEX_APP_SERVER_OPENAI_BASE_URL", "CODEX_APP_SERVER_LOGIN_ISSUER", "CODEX_REFRESH_TOKEN_URL_OVERRIDE", "CODEX_REVOKE_TOKEN_URL_OVERRIDE"]);
        foreach (var entry in entries.EnumerateObject().Where(p => p.Name.EndsWith(".js")))
        {
            if (entry.Value.TryGetProperty("unpacked", out var unpacked) && unpacked.GetBoolean()) continue;
            var size = entry.Value.GetProperty("size").GetInt32();
            if (size > 64 * 1024 * 1024) continue;
            stream.Position = 8 + headerSize + long.Parse(entry.Value.GetProperty("offset").GetString()!);
            var source = System.Text.Encoding.UTF8.GetString(reader.ReadBytes(size));
            required.RemoveWhere(source.Contains);
        }
        if (required.Count > 0) throw new InvalidOperationException("此版本缺少所需地址入口：" + string.Join(", ", required));
    }
    public static async Task<int> Start(ClientInstallation client, string server, IProgress<string> log, CancellationToken cancel, IReadOnlyDictionary<string,string>? testEnvironment = null)
    {
        CheckInstallation(client);
        if (testEnvironment is null)
        {
            foreach (var existing in Process.GetProcessesByName(Path.GetFileNameWithoutExtension(client.Executable)))
            {
                using (existing)
                {
                    string? path = null;
                    try { path = existing.MainModule?.FileName; } catch { }
                    if (path is not null && Path.GetFullPath(path).Equals(client.Executable, StringComparison.OrdinalIgnoreCase))
                        throw new InvalidOperationException("请先完全退出当前 Desktop，再点击启动，以便地址设置生效。");
                }
            }
        }
        var listener = new TcpListener(IPAddress.Loopback, 0); listener.Start();
        var port = ((IPEndPoint)listener.LocalEndpoint).Port; listener.Stop();
        var start = StartInfo(client, server, port);
        if (testEnvironment is not null)
        {
            foreach (var entry in testEnvironment) start.Environment[entry.Key] = entry.Value;
            // Test fixtures must also isolate Chromium before Electron executes
            // app.setPath; production starts never pass a profile argument.
            start.ArgumentList.Add("--user-data-dir=" + testEnvironment["CODEX_ELECTRON_USER_DATA_PATH"]);
            if(testEnvironment.TryGetValue("CODEX2API_TEST_RENDERER_PORT",out var value) && int.TryParse(value,out var rendererPort) && rendererPort>0 && rendererPort<65536)
            {
                start.Environment.Remove("CODEX2API_TEST_RENDERER_PORT");
                start.ArgumentList.Add("--remote-debugging-address=127.0.0.1");
                start.ArgumentList.Add("--remote-debugging-port="+rendererPort);
            }
        }
        using var process = Process.Start(start) ?? throw new InvalidOperationException("客户端启动失败。");
        var success = false;
        using var http = new HttpClient(new HttpClientHandler { UseProxy = false }) { Timeout = TimeSpan.FromMilliseconds(700) };
        try
        {
            log.Report("正在连接客户端启动进程…");
            string? debugger = null;
            for (var attempt = 0; attempt < 100; attempt++)
            {
                cancel.ThrowIfCancellationRequested();
                if (process.HasExited) throw new InvalidOperationException("客户端提前退出，请先关闭已运行的 Desktop。");
                try
                {
                    using var doc = JsonDocument.Parse(await http.GetStringAsync($"http://127.0.0.1:{port}/json/list", cancel));
                    debugger = doc.RootElement[0].GetProperty("webSocketDebuggerUrl").GetString();
                    if (debugger is not null) break;
                }
                catch (Exception) when (!cancel.IsCancellationRequested) { }
                await Task.Delay(100, cancel);
            }
            if (debugger is null) throw new InvalidOperationException("无法连接客户端调试入口。");
            var uri = new Uri(debugger);
            if (uri.Host != "127.0.0.1" || uri.Port != port) throw new InvalidOperationException("客户端调试地址不匹配。");
            await using (var inspector = new Inspector())
            {
                await inspector.Connect(uri, cancel);
                await inspector.Send("Debugger.enable", null, cancel);
                await inspector.Send("Runtime.runIfWaitingForDebugger", null, cancel);
                var pause = await inspector.Paused.Task.WaitAsync(TimeSpan.FromSeconds(12), cancel);
                var frame = pause.GetProperty("callFrames")[0].GetProperty("callFrameId").GetString();
                var pid = Inspector.Value(await inspector.Send("Debugger.evaluateOnCallFrame", new { callFrameId = frame, expression = "process.pid", returnByValue = true }, cancel)).GetInt32();
                if (pid != process.Id) throw new InvalidOperationException("调试连接不是本次启动的客户端。");
                var expression = "(()=>{const module={exports:{}};" + HookSource() + "\nconst h=module.exports;const targets=h.readTargets(" + JsonSerializer.Serialize(client.Archive) + ");h.install(" + JsonSerializer.Serialize(server) + ",targets,h.proxyPolicy,h.transform,require);h.isolateLauncherInspector(require('node:worker_threads'),process.execArgv," + JsonSerializer.Serialize(start.ArgumentList[0]) + ");return true;})()";
                Inspector.Value(await inspector.Send("Debugger.evaluateOnCallFrame", new { callFrameId = frame, expression, returnByValue = true }, cancel));
                await inspector.Send("Debugger.resume", null, cancel);
                var applied = false;
                for (var attempt = 0; attempt < 100; attempt++)
                {
                    var value = Inspector.Value(await inspector.Send("Runtime.evaluate", new { expression = "globalThis.__codex2apiDesktopHook?.applied?.length===2", returnByValue = true }, cancel));
                    if (value.ValueKind == JsonValueKind.True) { applied = true; break; }
                    await Task.Delay(100, cancel);
                }
                if (!applied) throw new InvalidOperationException("此版本未完整加载地址 hook，已停止本次启动。");
                await inspector.Send("Runtime.evaluate", new { expression = "setTimeout(()=>process.getBuiltinModule('node:inspector').close(),100);true", returnByValue = true }, cancel);
                await inspector.Send("Debugger.disable", null, cancel);
            }
            var closed = false;
            for (var i = 0; i < 40; i++)
            {
                await Task.Delay(100, cancel);
                try { await http.GetStringAsync($"http://127.0.0.1:{port}/json/list", cancel); }
                catch (Exception) when (!cancel.IsCancellationRequested) { closed = true; break; }
            }
            if (!closed) throw new InvalidOperationException("临时调试入口未关闭，已停止本次启动。");
            success = true; log.Report("已启动原版客户端，地址 hook 已生效，临时调试入口已关闭。");
            return process.Id;
        }
        finally { if (!success) { try { if (!process.HasExited) process.Kill(true); } catch { } } }
    }
}
