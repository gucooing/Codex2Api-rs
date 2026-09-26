using System.Diagnostics;
using System.Reflection;
using System.Security.Cryptography;
using System.Text.Json;

namespace DesktopProxy;

internal static class Launcher
{
    private static readonly string[] ClearedVariables = ["CODEX_HOME", "CODEX_SQLITE_HOME", "CODEX_ELECTRON_USER_DATA_PATH", "CODEX_CLI_PATH", "CODEX2API_REAL_CLI", "CODEX2API_PROXY_ROOT", "CODEX_APP_SERVER_WS_URL", "CODEX_APP_SERVER_FORCE_CLI", "CODEX_API_BASE_URL", "OPENAI_API_KEY", "CODEX_API_KEY", "CODEX_ACCESS_TOKEN", "CODEX_CONNECTORS_TOKEN", "NODE_OPTIONS"];

    public static string NormalizeServer(string value)
    {
        if (!Uri.TryCreate(value.Trim(), UriKind.Absolute, out var uri) || uri.Scheme is not ("http" or "https") || uri.UserInfo.Length > 0 || uri.Query.Length > 0 || uri.Fragment.Length > 0)
            throw new InvalidOperationException("请输入有效的 HTTP(S) 服务地址，不要包含密码、查询参数或片段。");
        var root = uri.AbsoluteUri.TrimEnd('/');
        return uri.AbsolutePath == "/" ? root + "/api/oauth/chatgpt" : root;
    }

    public static ProcessStartInfo StartInfo(ClientInstallation client, string server, ClientProfile profile)
    {
        var start = new ProcessStartInfo(client.Executable) { UseShellExecute = false, WorkingDirectory = Path.GetDirectoryName(client.Executable)!, CreateNoWindow = true };
        start.ArgumentList.Add("--user-data-dir=" + profile.AppData);
        foreach (var name in ClearedVariables) start.Environment.Remove(name);
        start.Environment["CODEX_HOME"] = profile.CodexHome;
        start.Environment["CODEX_SQLITE_HOME"] = Path.Combine(profile.CodexHome, "sqlite");
        start.Environment["CODEX_ELECTRON_USER_DATA_PATH"] = profile.AppData;
        start.Environment["CODEX_APP_SERVER_CHATGPT_BASE_URL"] = server + "/backend-api";
        start.Environment["CODEX_APP_SERVER_OPENAI_BASE_URL"] = server + "/backend-api/codex";
        start.Environment["CODEX_APP_SERVER_LOGIN_ISSUER"] = server;
        start.Environment["CODEX_REFRESH_TOKEN_URL_OVERRIDE"] = server + "/oauth/token";
        start.Environment["CODEX_REVOKE_TOKEN_URL_OVERRIDE"] = server + "/oauth/revoke";
        start.Environment["CODEX2API_HOOK_SERVER"] = server;
        start.Environment["CODEX2API_HOOK_ARCHIVE"] = client.Archive;
        return start;
    }

    public static void CheckInstallation(ClientInstallation client)
    {
        if (!File.Exists(client.Archive)) throw new InvalidOperationException("客户端资源文件不存在，请重新检测安装位置。");
        var runtime = Path.Combine(Path.GetDirectoryName(client.Executable)!, "chrome.dll");
        if (!File.Exists(runtime) || NativeProcess.ExportRva(runtime, "napi_get_global") == 0)
            throw new InvalidOperationException("无法读取客户端启动入口，请重新检测安装位置。");
    }

    private static string ExtractHook()
    {
        using var resource = Assembly.GetExecutingAssembly().GetManifestResourceStream("NativeHook")
            ?? throw new InvalidOperationException("启动器文件不完整，请重新安装。");
        using var data = new MemoryStream(); resource.CopyTo(data);
        var bytes = data.ToArray();
        var root = Path.Combine(Path.GetDirectoryName(Settings.FilePath)!, "native", Convert.ToHexStringLower(SHA256.HashData(bytes)));
        Directory.CreateDirectory(root);
        var file = Path.Combine(root, "Codex2API.NativeHook.node");
        if (!File.Exists(file)) File.WriteAllBytes(file, bytes);
        if (!SHA256.HashData(File.ReadAllBytes(file)).AsSpan().SequenceEqual(SHA256.HashData(bytes))) throw new InvalidOperationException("启动组件校验失败，请重新安装启动器。");
        return file;
    }

    public static async Task<int> Start(ClientInstallation client, string server, IProgress<string> log, CancellationToken cancel, IReadOnlyDictionary<string, string>? testEnvironment = null)
    {
        var profile = testEnvironment is null ? ClientProfile.ForServer(server)
            : new ClientProfile(testEnvironment["CODEX_HOME"], testEnvironment["CODEX_ELECTRON_USER_DATA_PATH"], server);
        profile.Prepare(); CheckInstallation(client);
        if (testEnvironment is null)
        {
            foreach (var existing in Process.GetProcessesByName(Path.GetFileNameWithoutExtension(client.Executable)))
            {
                using (existing)
                {
                    string? path = null; try { path = existing.MainModule?.FileName; } catch { }
                    if (path is null || Path.GetFullPath(path).Equals(client.Executable, StringComparison.OrdinalIgnoreCase))
                        throw new InvalidOperationException("请先完全退出当前 Desktop，再启动独立配置客户端。");
                }
            }
        }
        var hook = ExtractHook();
        var launch = Path.Combine(profile.CodexHome, "launcher", Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(launch);
        var status = Path.Combine(launch, "hook-status.txt");
        var request = new NativeLaunchRequest(client.Executable, server, profile.CodexHome, profile.AppData, hook, status);
        var input = Path.Combine(launch, "request.json");
        var output = Path.Combine(launch, "process.json");
        File.WriteAllText(input, JsonSerializer.Serialize(request));
        log.Report("正在启动客户端…");
        cancel.ThrowIfCancellationRequested();
        if (client.AppUserModelId is not null)
            PackagedApplication.StartHost(client.AppUserModelId, Environment.ProcessPath!, ["--native-launch", input, output]);
        else
            StartNativeHost(input, output);
        Process? process = null;
        var success = false;
        try
        {
            for (var attempt = 0; attempt < 300; attempt++)
            {
                cancel.ThrowIfCancellationRequested();
                if (process is null && File.Exists(output))
                {
                    using var report = JsonDocument.Parse(File.ReadAllText(output));
                    if (report.RootElement.TryGetProperty("error", out var error)) throw new InvalidOperationException(error.GetString());
                    process = Process.GetProcessById(report.RootElement.GetProperty("processId").GetInt32());
                }
                if (File.Exists(status))
                {
                    var state = File.ReadAllText(status);
                    if (state.StartsWith("ERROR:")) throw new InvalidOperationException(state);
                    if (state == "routing-installed" && process is not null)
                    {
                        success = true; log.Report("客户端已启动。"); return process.Id;
                    }
                }
                if (process?.HasExited == true) throw new InvalidOperationException("客户端在启动完成前退出。");
                await Task.Delay(100, cancel);
            }
            throw new TimeoutException("客户端启动超时。");
        }
        finally
        {
            if (!success)
            {
                File.WriteAllText(output + ".cancel", "");
                if (process is null && File.Exists(output))
                {
                    try
                    {
                        using var result = JsonDocument.Parse(File.ReadAllText(output));
                        if (result.RootElement.TryGetProperty("processId", out var id)) process = Process.GetProcessById(id.GetInt32());
                    }
                    catch (ArgumentException) { }
                }
            }
            if (!success && process is not null) { try { if (!process.HasExited) process.Kill(true); } catch { } }
            process?.Dispose();
        }
    }

    internal static void StartNativeHost(string input, string output)
    {
        try
        {
            var request = JsonSerializer.Deserialize<NativeLaunchRequest>(File.ReadAllText(input))!;
            var client = ClientInstallation.FromPath(request.Executable);
            var profile = new ClientProfile(request.CodexHome, request.AppData, request.Server);
            var start = StartInfo(client, request.Server, profile);
            start.Environment["CODEX2API_HOOK_FILE"] = request.HookFile;
            var pid = NativeProcess.Start(start, request.HookFile, request.StatusFile, cancelled: () => File.Exists(output + ".cancel"));
            WriteReport(output, new { processId = pid });
            if (File.Exists(output + ".cancel")) { using var process = Process.GetProcessById(pid); if (!process.HasExited) process.Kill(true); }
        }
        catch (Exception e) { WriteReport(output, new { error = e.Message }); }
    }

    private static void WriteReport(string path, object value)
    {
        File.WriteAllText(path + ".tmp", JsonSerializer.Serialize(value)); File.Move(path + ".tmp", path, true);
    }
}

internal sealed record NativeLaunchRequest(string Executable, string Server, string CodexHome, string AppData, string HookFile, string StatusFile);
