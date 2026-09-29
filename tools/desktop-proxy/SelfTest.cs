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
            var profile = ClientProfile.ForServer(settings.Server, root, Path.Combine(root, "shared"));
            Require(profile == ClientProfile.ForServer(settings.Server + "/", root, profile.SharedHome), "Equivalent server URLs split profiles");
            Require(profile.CodexHome != ClientProfile.ForServer("https://third.example/next", root).CodexHome, "Different servers share credentials");
            Require(profile.CodexHome != ClientProfile.ForServer("https://second.example/other", root).CodexHome, "Different service paths share credentials");
            profile.Prepare();
            Require(!File.Exists(profile.AuthFile), "Launcher manufactured login credentials");
            Require(File.ReadAllText(profile.ConfigFile).Contains("cli_auth_credentials_store = \"file\""), "Profile does not use auth.json");
            Require(File.ReadAllText(profile.ConfigFile).Contains(settings.Server + "/backend-api"), "Profile lacks configured service");
            File.AppendAllText(profile.ConfigFile, "model = \"test-model\"\n");
            var config = File.ReadAllText(profile.ConfigFile);
            File.WriteAllText(profile.AuthFile, "fixture-credential-sentinel");
            profile.Prepare();
            Require(File.ReadAllText(profile.ConfigFile) == config && File.ReadAllText(profile.AuthFile) == "fixture-credential-sentinel", "Existing preferences or credentials were overwritten");
            CheckPackageDiscovery(root, Require);
            CheckArgumentQuoting(Require);
            var policy = new NativeHook.AddressPolicy(settings.Server);
            Require(policy.Map("https://chatgpt.com/backend-api/me?x=1") == settings.Server + "/backend-api/me?x=1", "Backend address routing failed");
            Require(policy.Map("https://chatgpt.com.evil.test/backend-api/me") == "https://chatgpt.com.evil.test/backend-api/me", "Unrelated host was routed");
            Require(policy.Map("http://localhost:1455/auth/callback?code=fixture") == "http://localhost:1455/auth/callback?code=fixture", "Local login callback changed");
            // Render on the STA thread before the first await, without displaying a window.
            using (var form = new MainForm(true))
            {
                form.ShowInTaskbar=false;form.Opacity=0;form.Show();Application.DoEvents();form.PerformLayout();
                using var bitmap = new Bitmap(form.Width, form.Height);form.DrawToBitmap(bitmap, new Rectangle(Point.Empty, bitmap.Size));
                bitmap.Save(Path.ChangeExtension(report, ".png"));
            }
            var client = await Task.Run(() => ClientInstallation.Detect(CancellationToken.None)).ConfigureAwait(false);
            Require(!string.IsNullOrEmpty(client.AppUserModelId), "Store registration was discarded");
            var manual = await ClientInstallation.ResolvePath(client.Executable, CancellationToken.None).ConfigureAwait(false);
            Require(manual == client, "Manual Store selection bypasses package activation");
            Launcher.CheckInstallation(client);
            var start = Launcher.StartInfo(client, settings.Server, profile);
            Require(start.FileName == client.Executable, "Client executable changed");
            Require(start.ArgumentList.Count == 0, "Shared desktop was given an isolated user data directory");
            Require(start.Environment["CODEX_HOME"] == profile.SharedHome, "Session home is not shared");
            Require(!start.Environment.ContainsKey("CODEX_SQLITE_HOME") || start.Environment["CODEX_SQLITE_HOME"] == Environment.GetEnvironmentVariable("CODEX_SQLITE_HOME", EnvironmentVariableTarget.User), "Database location does not follow the original client");
            Require(!start.Environment.ContainsKey("CODEX_ELECTRON_USER_DATA_PATH"), "Desktop data is not shared");
            Require(start.Environment["CODEX2API_PRIVATE_HOME"] == profile.CodexHome, "Private credential/config destination missing");
            foreach (var name in new[] { "CODEX_CLI_PATH", "CODEX2API_REAL_CLI", "OPENAI_API_KEY", "CODEX_API_KEY", "CODEX_ACCESS_TOKEN", "CODEX_CONNECTORS_TOKEN" }) Require(!start.Environment.ContainsKey(name), "Foreign runtime/credential override remained: " + name);
            Require(!start.Environment.ContainsKey("NODE_OPTIONS"), "An inherited script preload remained");
            Require(start.Environment["CODEX_APP_SERVER_LOGIN_ISSUER"] == settings.Server, "Login origin mismatch");
            Require(start.Environment["CODEX_REFRESH_TOKEN_URL_OVERRIDE"] == settings.Server + "/oauth/token", "Token origin mismatch");
            var notice = JsonDocument.Parse("{\"title\":\"A < B\",\"body\":\"text & body\",\"hasReply\":true,\"actions\":[{\"text\":\"Open\"}]}");
            var xml = System.Xml.Linq.XElement.Parse(NotificationHost.BuildXml(notice.RootElement, "fixture"));
            Require(xml.Attribute("launch")?.Value == "fixture:click", "Notification click token changed");
            Require(xml.Descendants("text").First().Value == "A < B", "Notification XML did not escape text");
            Require(xml.Descendants("action").Select(element => element.Attribute("arguments")?.Value).SequenceEqual(["fixture:action:0", "fixture:reply"]), "Notification actions/reply changed");
            Require(NotificationHost.Describe(client, profile).ApplicationId != NotificationHost.Describe(client, profile with { CodexHome = Path.Combine(root, "other") }).ApplicationId, "Notification identities are shared between services");
            File.WriteAllText(report, "PASS: URL routing, shared session/desktop paths, separate credential/config paths and preservation, package/manual discovery, argument quoting, notification XML/actions/profile identity, runtime exports and GUI rendering.\nClient: " + client.Version + "\nThis self-test does not launch Desktop or prove login/inference.");
        }
        finally { Directory.Delete(root, true); }
    }

    private static void CheckPackageDiscovery(string root, Action<bool, string> require)
    {
        var packageRoot = Path.Combine(root, "package");
        Directory.CreateDirectory(Path.Combine(packageRoot, "app", "resources"));
        File.WriteAllText(Path.Combine(packageRoot, "app", "client.exe"), "fixture");
        File.WriteAllText(Path.Combine(packageRoot, "app", "resources", "app.asar"), "fixture");
        File.WriteAllText(Path.Combine(packageRoot, "AppxManifest.xml"), "<Package><Applications><Application Id=\"Desktop\" Executable=\"app/client.exe\"/><Application Id=\"Escape\" Executable=\"../outside.exe\"/></Applications></Package>");
        using var packages = JsonDocument.Parse(JsonSerializer.Serialize(new[] {
            new { InstallLocation = packageRoot, Version = "1.0.0.0", PackageFamilyName = "Fixture_family" },
            new { InstallLocation = packageRoot, Version = "2.0.0.0", PackageFamilyName = "Updated_family" }
        }));
        var clients = ClientInstallation.ReadPackages(packages.RootElement);
        require(clients.Count == 2 && clients[0].AppUserModelId == "Updated_family!Desktop", "Manifest application identity/version ordering changed");
    }

    private static void CheckArgumentQuoting(Action<bool, string> require)
    {
        var expected = new[] { "", "--user-data-dir=C:\\a path\\配置\\", "embedded\"quote", "slash\\\"quote" };
        var commandLine = "fixture.exe " + string.Join(" ", expected.Select(PackagedApplication.QuoteArgument));
        var argv = CommandLineToArgvW(commandLine, out var count);
        if (argv == IntPtr.Zero) throw new System.ComponentModel.Win32Exception();
        try
        {
            require(count == expected.Length + 1, "Activation argument count changed");
            for (var i = 0; i < expected.Length; i++)
                require(System.Runtime.InteropServices.Marshal.PtrToStringUni(System.Runtime.InteropServices.Marshal.ReadIntPtr(argv, (i + 1) * IntPtr.Size)) == expected[i], "Activation argument quoting changed a value");
        }
        finally { LocalFree(argv); }
    }

    [System.Runtime.InteropServices.DllImport("shell32.dll", CharSet = System.Runtime.InteropServices.CharSet.Unicode, SetLastError = true)]
    private static extern IntPtr CommandLineToArgvW(string commandLine, out int count);
    [System.Runtime.InteropServices.DllImport("kernel32.dll")]
    private static extern IntPtr LocalFree(IntPtr memory);
}
