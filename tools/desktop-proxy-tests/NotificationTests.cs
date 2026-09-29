using System.Diagnostics;
using System.IO.Pipes;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.Json;
using Microsoft.Win32;
using Windows.UI.Notifications;

internal static class NotificationTests
{
    internal static void Activate(string input, string token)
    {
        using var launch = JsonDocument.Parse(File.ReadAllText(input));
        var instance = Activator.CreateInstance(Type.GetTypeFromCLSID(Guid.Parse(launch.RootElement.GetProperty("ClassId").GetString()!))!)!;
        try { ((INotificationActivationCallback)instance).Activate(launch.RootElement.GetProperty("ApplicationId").GetString()!, token + ":click", 0, 0); }
        finally { Marshal.ReleaseComObject(instance); }
    }

    internal static async Task Run(string launcher, string report)
    {
        var root = Directory.CreateTempSubdirectory("codex-notification-test-").FullName;
        var pipeName = "codex2api-test-" + Guid.NewGuid().ToString("N");
        var applicationId = "Codex2API.Test." + Guid.NewGuid().ToString("N");
        var classId = Guid.NewGuid().ToString("B");
        var input = Path.Combine(root, "input.json");
        File.WriteAllText(input, JsonSerializer.Serialize(new { Pipe = pipeName, ApplicationId = applicationId, ClassId = classId, Server = "https://fixture.invalid", Executable = launcher, CodexHome = root, AppData = root }));
        using var host = DesktopProxy.NativeProcess.StartUnpackaged(launcher, ["--notification-host", input]);
        try
        {
            using var deadline = new CancellationTokenSource(TimeSpan.FromSeconds(30));
            while (!File.Exists(input + ".ready"))
            {
                if (host.HasExited) throw new InvalidOperationException(File.Exists(input + ".error") ? File.ReadAllText(input + ".error") : "Notification host exited.");
                await Task.Delay(50, deadline.Token);
            }
            using (var pipe = new NamedPipeClientStream(".", pipeName, PipeDirection.InOut, PipeOptions.Asynchronous))
            {
                await pipe.ConnectAsync(deadline.Token);
                using var writer = new StreamWriter(pipe, new UTF8Encoding(false), leaveOpen: true) { AutoFlush = true };
                using var reader = new StreamReader(pipe, Encoding.UTF8, leaveOpen: true);
                foreach (var action in new[] { "click", "action:0", "reply" })
                {
                    var token = Guid.NewGuid().ToString("N");
                    await writer.WriteLineAsync(JsonSerializer.Serialize(new { kind = "show", token, options = new { title = "Codex2API 通知回归测试", body = "测试独立通知归属与点击回调", silent = true, hasReply = true, actions = new[] { new { text = "查看" } } } }));
                    using var shown = JsonDocument.Parse((await reader.ReadLineAsync(deadline.Token))!);
                    if (shown.RootElement.GetProperty("kind").GetString() != "show") throw new InvalidOperationException(shown.RootElement.ToString());
                    var instance = Activator.CreateInstance(Type.GetTypeFromCLSID(Guid.Parse(classId))!)!;
                    try { ((INotificationActivationCallback)instance).Activate(applicationId, token + ":" + action, 0, 0); }
                    finally { Marshal.ReleaseComObject(instance); }
                    using var activated = JsonDocument.Parse((await reader.ReadLineAsync(deadline.Token))!);
                    if (activated.RootElement.GetProperty("kind").GetString() != action.Split(':')[0] || activated.RootElement.GetProperty("token").GetString() != token)
                        throw new InvalidOperationException("Notification activation was sent to the wrong request.");
                    using var closed = JsonDocument.Parse((await reader.ReadLineAsync(deadline.Token))!);
                    if (closed.RootElement.GetProperty("kind").GetString() != "close") throw new InvalidOperationException("Notification was not removed after activation.");
                }
            }
            await host.WaitForExitAsync(deadline.Token);
            if (host.ExitCode != 0) throw new InvalidOperationException(File.ReadAllText(input + ".error"));
            if (ToastNotificationManager.History.GetHistory(applicationId).Count != 0) throw new InvalidOperationException("Notifications survived their profile connection.");
            File.WriteAllText(report, "PASS: Windows toast creation, profile-specific Windows activation identity, COM click/action/reply dispatch to the originating pipe, host shutdown and notification cleanup.");
        }
        catch (Exception error) { File.WriteAllText(report, error.ToString()); throw; }
        finally
        {
            if (!host.HasExited) { host.Kill(); await host.WaitForExitAsync(); }
            ToastNotificationManager.History.Clear(applicationId);
            Registry.CurrentUser.DeleteSubKeyTree("Software\\Classes\\AppUserModelId\\" + applicationId, false);
            Registry.CurrentUser.DeleteSubKeyTree("Software\\Classes\\CLSID\\" + classId, false);
            Directory.Delete(root, true);
        }
    }

    [ComImport, Guid("53E31837-6600-4A81-9395-75CFFE746F94"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    private interface INotificationActivationCallback
    {
        void Activate([MarshalAs(UnmanagedType.LPWStr)] string applicationId, [MarshalAs(UnmanagedType.LPWStr)] string arguments, nint data, uint count);
    }
}
