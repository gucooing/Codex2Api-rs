using System.Security.Cryptography;
using System.Text;
using System.Text.Json;

namespace DesktopProxy;

internal sealed record ClientProfile(string CodexHome, string AppData, string Server)
{
    public string ConfigFile => Path.Combine(CodexHome, "config.toml");
    public string AuthFile => Path.Combine(CodexHome, "auth.json");

    public static ClientProfile ForServer(string server, string? profilesRoot = null)
    {
        var normalized = Launcher.NormalizeServer(server);
        var key = Convert.ToHexStringLower(SHA256.HashData(Encoding.UTF8.GetBytes(normalized)));
        var root = Path.Combine(profilesRoot ?? Path.Combine(Path.GetDirectoryName(Settings.FilePath)!, "profiles"), key);
        return new(Path.Combine(root, "codex"), Path.Combine(root, "desktop"), normalized);
    }

    public void Prepare()
    {
        Directory.CreateDirectory(CodexHome);
        Directory.CreateDirectory(AppData);
        // Create once. Desktop owns subsequent preference writes and token
        // refreshes; never overwrite an existing config or manufacture auth.json.
        if (File.Exists(ConfigFile)) return;
        var temporary = ConfigFile + "." + Guid.NewGuid().ToString("N") + ".tmp";
        try
        {
            File.WriteAllText(temporary, "# Codex2API Desktop launcher profile\ncli_auth_credentials_store = \"file\"\n"
                + "chatgpt_base_url = " + JsonSerializer.Serialize(Server + "/backend-api") + "\n"
                + "openai_base_url = " + JsonSerializer.Serialize(Server + "/backend-api/codex") + "\n", new UTF8Encoding(false));
            try { File.Move(temporary, ConfigFile); }
            catch (IOException) when (File.Exists(ConfigFile)) { }
        }
        finally { if (File.Exists(temporary)) File.Delete(temporary); }
    }
}
