using System.Text.Json;

namespace DesktopProxy;

internal static class SelfTest
{
    public static async Task Run(string report)
    {
        void Require(bool condition, string message) { if (!condition) throw new InvalidOperationException(message); }
        Require(Launcher.NormalizeServer("https://proxy.example:8443/") == "https://proxy.example:8443/api/oauth/chatgpt", "Origin normalization failed");
        Require(Launcher.NormalizeServer("https://another.example/custom/path/") == "https://another.example/custom/path", "Custom path changed");
        foreach (var bad in new[] { "", "file:///x", "https://a:b@example.test", "https://example.test/?secret=x" })
        {
            var rejected = false;try { Launcher.NormalizeServer(bad); } catch (InvalidOperationException) { rejected = true; }
            Require(rejected, "Invalid URL accepted");
        }
        var root = Directory.CreateTempSubdirectory("desktop-launcher-test-").FullName;
        try
        {
            var settings = new Settings { Server = "https://first.example/custom", AutoDetect = true };
            var file = Path.Combine(root, "settings.json");settings.Save(file);
            settings = Settings.Read(file);settings.Server = "https://second.example/next";settings.Save(file);
            Require(Settings.Read(file).Server == settings.Server, "Updated server did not persist");
            Require(Settings.Read(file).ClientPath == "", "Automatic discovery persisted a machine path");
            // Render on the STA thread before the first await, without displaying a window.
            using (var form = new MainForm(true))
            {
                form.ShowInTaskbar=false;form.Opacity=0;form.Show();Application.DoEvents();form.PerformLayout();
                using var bitmap = new Bitmap(form.Width, form.Height);form.DrawToBitmap(bitmap, new Rectangle(Point.Empty, bitmap.Size));
                bitmap.Save(Path.ChangeExtension(report, ".png"));
            }
            var client = await Task.Run(() => ClientInstallation.Detect(CancellationToken.None)).ConfigureAwait(false);
            Launcher.CheckInstallation(client);
            var start = Launcher.StartInfo(client, settings.Server, 12345);
            Require(start.FileName == client.Executable, "Client executable changed");
            Require(start.ArgumentList.Count == 1 && !start.ArgumentList[0].Contains("user-data"), "Unexpected profile argument");
            foreach (var name in new[] { "CODEX_HOME", "CODEX_SQLITE_HOME", "CODEX_ELECTRON_USER_DATA_PATH", "CODEX_CLI_PATH", "CODEX2API_REAL_CLI" }) Require(!start.Environment.ContainsKey(name), "Profile/runtime override remained: " + name);
            Require(start.Environment["CODEX_APP_SERVER_LOGIN_ISSUER"] == settings.Server, "Login origin mismatch");
            Require(start.Environment["CODEX_REFRESH_TOKEN_URL_OVERRIDE"] == settings.Server + "/oauth/token", "Token origin mismatch");
            File.WriteAllText(report, "PASS: URL validation, address changes, portable settings, current-package discovery, default credentials/runtime, address overrides and GUI rendering.\nClient: " + client.Version);
        }
        finally { Directory.Delete(root, true); }
    }
}
