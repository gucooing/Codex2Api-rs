using System.Text.Json;

namespace DesktopProxy;

internal static class Program
{
    [STAThread]
    private static void Main(string[] args)
    {
        if (args.Length == 3 && args[0] == "--native-launch") { Launcher.StartNativeHost(args[1], args[2]); return; }
        ApplicationConfiguration.Initialize();
        if (args.Length == 3 && args[0] == "--test-launch")
        {
            try
            {
                using var doc=JsonDocument.Parse(File.ReadAllText(args[1]));var input=doc.RootElement;
                var client=ClientInstallation.ResolvePath(input.GetProperty("executable").GetString()!, CancellationToken.None).GetAwaiter().GetResult();
                var server=Launcher.NormalizeServer(input.GetProperty("proxyRoot").GetString()!);
                var environment=new Dictionary<string,string>{["CODEX_HOME"]=input.GetProperty("clientHome").GetString()!,["CODEX_SQLITE_HOME"]=Path.Combine(input.GetProperty("clientHome").GetString()!,"sqlite"),["CODEX_ELECTRON_USER_DATA_PATH"]=input.GetProperty("appData").GetString()!};
                using var timeout=new CancellationTokenSource(TimeSpan.FromSeconds(60));
                var pid=Launcher.Start(client,server,new Progress<string>(),timeout.Token,environment).GetAwaiter().GetResult();
                File.WriteAllText(args[2],JsonSerializer.Serialize(new {processId=pid,nativeHookInstalled=true}));
            }
            catch(Exception e) {File.WriteAllText(args[2],e.ToString());Environment.ExitCode=1;}
            return;
        }
        if (args.Length == 2 && args[0] == "--self-test")
        {
            try { SelfTest.Run(args[1]).GetAwaiter().GetResult(); }
            catch (Exception e) { File.WriteAllText(args[1], e.ToString()); Environment.ExitCode = 1; }
            return;
        }
        Application.Run(new MainForm());
    }
}

internal sealed class Settings
{
    public string Server { get; set; } = "";
    public bool AutoDetect { get; set; } = true;
    public string ClientPath { get; set; } = "";
    public static string FilePath => Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "Codex2API", "DesktopLauncher", "settings.json");
    public static Settings Read(string path) => File.Exists(path) ? JsonSerializer.Deserialize<Settings>(File.ReadAllText(path)) ?? new() : new();
    public void Save(string path)
    {
        Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(path))!);
        var temporary = path + "." + Guid.NewGuid().ToString("N") + ".tmp";
        try
        {
            File.WriteAllText(temporary, JsonSerializer.Serialize(this, new JsonSerializerOptions { WriteIndented = true }));
            File.Move(temporary, path, true);
        }
        finally { if (File.Exists(temporary)) File.Delete(temporary); }
    }
}
