using System.Diagnostics;
using System.Text.Json;
using System.Xml.Linq;

namespace DesktopProxy;

internal sealed record ClientInstallation(string Executable, string Version, string? AppUserModelId = null)
{
    public string Archive => Path.Combine(Path.GetDirectoryName(Executable)!, "resources", "app.asar");
    public static ClientInstallation FromPath(string path)
    {
        path = Path.GetFullPath(path.Trim().Trim('"'));
        if (!File.Exists(path)) throw new InvalidOperationException("找不到客户端程序，请重新检测或选择文件。");
        var client = new ClientInstallation(path, FileVersionInfo.GetVersionInfo(path).ProductVersion ?? "未知版本");
        if (!File.Exists(client.Archive)) throw new InvalidOperationException("该程序目录没有 resources/app.asar，请选择桌面客户端主程序。");
        return client;
    }
    public static async Task<ClientInstallation> ResolvePath(string path, CancellationToken cancel)
    {
        var client = FromPath(path);
        for (var directory = new DirectoryInfo(Path.GetDirectoryName(client.Executable)!); directory is not null; directory = directory.Parent)
        {
            if (!File.Exists(Path.Combine(directory.FullName, "AppxManifest.xml"))) continue;
            return (await Installed(cancel)).FirstOrDefault(c => c.Executable.Equals(client.Executable, StringComparison.OrdinalIgnoreCase))
                ?? throw new InvalidOperationException("该商店客户端未注册到当前 Windows 用户，请重新安装或自动检测。");
        }
        return client;
    }
    public static async Task<ClientInstallation> Detect(CancellationToken cancel)
        => (await Installed(cancel)).FirstOrDefault()
            ?? throw new InvalidOperationException("未发现已安装的 Codex / ChatGPT 桌面端，请手动选择客户端。");

    private static async Task<List<ClientInstallation>> Installed(CancellationToken cancel)
    {
        var start = new ProcessStartInfo("powershell.exe") { UseShellExecute = false, CreateNoWindow = true, RedirectStandardOutput = true, RedirectStandardError = true };
        start.ArgumentList.Add("-NoProfile"); start.ArgumentList.Add("-NonInteractive"); start.ArgumentList.Add("-Command");
        start.ArgumentList.Add("[Console]::OutputEncoding=[Text.UTF8Encoding]::new($false); @(Get-AppxPackage -ErrorAction Stop | Where-Object { $_.Name -like '*Codex*' -or $_.Name -like '*ChatGPT*' } | Select-Object Name,Version,InstallLocation,PackageFamilyName) | ConvertTo-Json -Compress");
        using var process = Process.Start(start) ?? throw new InvalidOperationException("无法检测已安装应用。");
        using var registration = cancel.Register(() => { try { process.Kill(true); } catch { } });
        var output = process.StandardOutput.ReadToEndAsync(cancel);
        var error = process.StandardError.ReadToEndAsync(cancel);
        await process.WaitForExitAsync(cancel);
        if (process.ExitCode != 0) throw new InvalidOperationException("自动检测失败，请手动选择客户端。" + await error);
        using var doc = JsonDocument.Parse(string.IsNullOrWhiteSpace(await output) ? "[]" : await output);
        return ReadPackages(doc.RootElement);
    }
    internal static List<ClientInstallation> ReadPackages(JsonElement packagesJson)
    {
        var clients = new List<ClientInstallation>();
        var packages = packagesJson.ValueKind == JsonValueKind.Array ? packagesJson.EnumerateArray().ToArray() : [packagesJson];
        foreach (var package in packages.OrderByDescending(p => System.Version.TryParse(p.GetProperty("Version").GetString(), out var v) ? v : new()))
        {
            var root = package.GetProperty("InstallLocation").GetString();
            if (string.IsNullOrWhiteSpace(root)) continue;
            var family = package.GetProperty("PackageFamilyName").GetString();
            if (string.IsNullOrWhiteSpace(family)) continue;
            var manifest = Path.Combine(root, "AppxManifest.xml");
            if (!File.Exists(manifest)) continue;
            foreach (var app in XDocument.Load(manifest).Descendants().Where(e => e.Name.LocalName == "Application"))
            {
                var relative = app.Attribute("Executable")?.Value;
                var id = app.Attribute("Id")?.Value;
                if (string.IsNullOrWhiteSpace(relative) || string.IsNullOrWhiteSpace(id)) continue;
                var path = Path.GetFullPath(Path.Combine(root, relative));
                if (!path.StartsWith(Path.GetFullPath(root) + Path.DirectorySeparatorChar, StringComparison.OrdinalIgnoreCase)) continue;
                if (File.Exists(path) && File.Exists(Path.Combine(Path.GetDirectoryName(path)!, "resources", "app.asar")))
                    clients.Add(new(path, package.GetProperty("Version").GetString() ?? "未知版本", family + "!" + id));
            }
        }
        return clients;
    }
}
