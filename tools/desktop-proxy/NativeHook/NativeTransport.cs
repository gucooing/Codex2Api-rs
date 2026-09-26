using System.Diagnostics;
using System.Text.Json.Nodes;

namespace DesktopProxy.NativeHook;

internal static class NativeTransport
{
    internal static (string[] Arguments, JsonObject Saved) Configure(string executable, string[] arguments, string? directory,
        IReadOnlyDictionary<string, string>? environment, AddressPolicy policy)
    {
        if (!Path.GetFileNameWithoutExtension(executable).Equals("codex", StringComparison.OrdinalIgnoreCase))
            throw new InvalidOperationException("当前客户端的运行方式暂不支持自定义服务。");
        var start = new ProcessStartInfo(executable) { UseShellExecute = false, CreateNoWindow = true, RedirectStandardInput = true, RedirectStandardOutput = true, RedirectStandardError = true };
        if (directory is not null) start.WorkingDirectory = directory;
        foreach (var argument in arguments) start.ArgumentList.Add(argument);
        start.ArgumentList.Add("-c"); start.ArgumentList.Add("model_provider=\"openai\"");
        if (environment is not null) { start.Environment.Clear(); foreach (var entry in environment) start.Environment[entry.Key] = entry.Value; }
        using var process = Process.Start(start) ?? throw new InvalidOperationException("无法读取客户端配置。");
        using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(12));
        _ = process.StandardError.ReadToEndAsync(timeout.Token);
        try
        {
            process.StandardInput.WriteLine("{\"id\":\"launcher-init\",\"method\":\"initialize\",\"params\":{\"clientInfo\":{\"name\":\"codex2api_launcher\",\"version\":\"1\"},\"capabilities\":{\"experimentalApi\":true}}}");
            var total = 0;
            while (true)
            {
                var line = process.StandardOutput.ReadLineAsync(timeout.Token).AsTask().GetAwaiter().GetResult()
                    ?? throw new InvalidOperationException("客户端在配置读取完成前退出。");
                total += line.Length; if (total > 8 * 1024 * 1024) throw new InvalidOperationException("客户端配置响应过大。");
                var response = JsonNode.Parse(line)?.AsObject();
                var id = response?["id"]?.ToString();
                if (id is not ("launcher-init" or "launcher-config")) continue;
                if (response?["error"] is not null) throw new InvalidOperationException("客户端拒绝读取配置。");
                if (id == "launcher-init")
                {
                    process.StandardInput.WriteLine("{\"method\":\"initialized\"}");
                    process.StandardInput.WriteLine("{\"id\":\"launcher-config\",\"method\":\"config/read\",\"params\":{\"includeLayers\":false}}");
                    continue;
                }
                var saved = response?["result"]?["config"]?.AsObject() ?? throw new InvalidOperationException("客户端配置响应不完整。");
                var output = arguments.ToList();
                foreach (var (key, value) in policy.NativeConfig(saved)) { output.Add("-c"); output.Add(key + "=" + value!.ToJsonString()); }
                return (output.ToArray(), saved);
            }
        }
        finally { try { if (!process.HasExited) process.Kill(true); } catch { } }
    }
}
