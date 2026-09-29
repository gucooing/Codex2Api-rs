using System.Diagnostics;
using System.IO.Pipes;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Xml.Linq;
using Microsoft.Win32;
using Windows.Data.Xml.Dom;
using Windows.UI.Notifications;

namespace DesktopProxy;

internal sealed record NotificationLaunch(string Pipe, string ApplicationId, string ClassId, string Server, string Executable, string CodexHome, string AppData);

internal static class NotificationHost
{
    internal static NotificationLaunch Describe(ClientInstallation client, ClientProfile profile)
    {
        var digest = SHA256.HashData(Encoding.UTF8.GetBytes(Path.GetFullPath(profile.CodexHome).ToUpperInvariant()));
        var identity = Convert.ToHexStringLower(digest);
        return new("codex2api-notifications-" + identity, "Codex2API.Desktop." + identity, new Guid(digest.AsSpan(0, 16)).ToString("B"),
            profile.Server, client.Executable, profile.CodexHome, profile.AppData);
    }

    internal static Process Start(NotificationLaunch launch, string directory)
    {
        var input = Path.Combine(directory, "notifications.json");
        File.WriteAllText(input, JsonSerializer.Serialize(launch));
        return NativeProcess.StartUnpackaged(Environment.ProcessPath!, ["--notification-host", input]);
    }

    internal static string BuildXml(JsonElement options, string token)
    {
        string? Text(string key) => options.TryGetProperty(key, out var value) && value.ValueKind == JsonValueKind.String ? value.GetString() : null;
        bool Flag(string key) => options.TryGetProperty(key, out var value) && value.ValueKind == JsonValueKind.True;
        var toast = new XElement("toast", new XAttribute("launch", token + ":click"),
            new XElement("visual", new XElement("binding", new XAttribute("template", "ToastGeneric"),
                new XElement("text", Text("title") ?? ""), new XElement("text", Text("body") ?? ""))));
        if (Text("timeoutType") == "never") toast.SetAttributeValue("scenario", "reminder");
        if (Flag("silent")) toast.Add(new XElement("audio", new XAttribute("silent", "true")));
        var actions = new XElement("actions");
        if (Flag("hasReply")) actions.Add(new XElement("input", new XAttribute("id", "reply"), new XAttribute("type", "text"), new XAttribute("placeHolderContent", Text("replyPlaceholder") ?? "")));
        if (options.TryGetProperty("actions", out var buttons) && buttons.ValueKind == JsonValueKind.Array)
        {
            var index = 0;
            foreach (var button in buttons.EnumerateArray().Take(4))
            {
                actions.Add(new XElement("action", new XAttribute("content", button.GetProperty("text").GetString() ?? ""),
                    new XAttribute("activationType", "foreground"), new XAttribute("arguments", token + ":action:" + index)));
                index++;
            }
        }
        if (Flag("hasReply")) actions.Add(new XElement("action", new XAttribute("content", "回复"), new XAttribute("activationType", "foreground"),
            new XAttribute("arguments", token + ":reply"), new XAttribute("hint-inputId", "reply")));
        if (actions.HasElements) toast.Add(actions);
        return toast.ToString(SaveOptions.DisableFormatting);
    }

    internal static async Task Run(string input)
    {
        var launch = JsonSerializer.Deserialize<NotificationLaunch>(File.ReadAllText(input))!;
        using var singleton = new Mutex(true, "Local\\" + launch.Pipe, out var created);
        if (!created) return;
        using var pipe = new NamedPipeServerStream(launch.Pipe, PipeDirection.InOut, 1, PipeTransmissionMode.Byte, PipeOptions.Asynchronous | PipeOptions.CurrentUserOnly);
        var notifications = new System.Collections.Concurrent.ConcurrentDictionary<string, ToastNotification>();
        using var sendLock = new SemaphoreSlim(1);
        StreamWriter? writer = null;
        async Task Send(object value)
        {
            await sendLock.WaitAsync();
            try { if (pipe.IsConnected && writer is not null) await writer.WriteLineAsync(JsonSerializer.Serialize(value)); }
            catch (IOException) { }
            finally { sendLock.Release(); }
        }
        async Task Activate(string arguments, IReadOnlyDictionary<string, string> values)
        {
            var parts = arguments.Split(':');
            if (parts.Length < 2 || !notifications.TryRemove(parts[0], out _)) return;
            var action = parts[1];
            if (action == "click") await Send(new { token = parts[0], kind = "click" });
            else if (action == "action" && parts.Length == 3 && int.TryParse(parts[2], out var index)) await Send(new { token = parts[0], kind = "action", index });
            else if (action == "reply") await Send(new { token = parts[0], kind = "reply", reply = values.GetValueOrDefault("reply", "") });
            await Send(new { token = parts[0], kind = "close" });
        }
        using var registration = new ToastActivation(launch, input, (arguments, values) => _ = Activate(arguments, values));
        var notifier = ToastNotificationManager.CreateToastNotifier(launch.ApplicationId);
        File.WriteAllText(input + ".ready", "ready");
        using var connectTimeout = new CancellationTokenSource(TimeSpan.FromSeconds(60));
        try
        {
            await pipe.WaitForConnectionAsync(connectTimeout.Token);
            File.WriteAllText(input + ".connected", "connected");
            writer = new StreamWriter(pipe, new UTF8Encoding(false), leaveOpen: true) { AutoFlush = true };
            using var reader = new StreamReader(pipe, Encoding.UTF8, leaveOpen: true);
            while (await reader.ReadLineAsync() is { } line)
            {
                if (line.Length > 131072) throw new InvalidOperationException("通知内容超过限制。");
                using var message = JsonDocument.Parse(line);
                var request = message.RootElement;
                var token = request.GetProperty("token").GetString()!;
                if (!Guid.TryParseExact(token, "N", out _)) throw new InvalidOperationException("无效的通知标识。");
                if (request.GetProperty("kind").GetString() == "close")
                {
                    if (notifications.TryRemove(token, out var previous)) notifier.Hide(previous);
                    await Send(new { token, kind = "close" });
                    continue;
                }
                try
                {
                    var xml = new XmlDocument(); xml.LoadXml(BuildXml(request.GetProperty("options"), token));
                    var toast = new ToastNotification(xml) { Tag = token[..16], Group = "desktop", ExpiresOnReboot = true };
                    toast.Activated += (sender, activation) =>
                    {
                        if (activation is not ToastActivatedEventArgs args) return;
                        _ = Activate(args.Arguments, args.UserInput.ToDictionary(pair => pair.Key, pair => pair.Value?.ToString() ?? ""));
                    };
                    toast.Dismissed += (_, _) => { if (notifications.TryRemove(token, out _)) _ = Send(new { token, kind = "close" }); };
                    toast.Failed += (sender, error) => { notifications.TryRemove(token, out _); _ = Send(new { token, kind = "failed", error = error.ErrorCode.Message }); };
                    if (notifications.TryRemove(token, out var previous)) notifier.Hide(previous);
                    notifications[token] = toast;
                    notifier.Show(toast);
                    await Send(new { token, kind = "show" });
                }
                catch (Exception error) { await Send(new { token, kind = "failed", error = error.Message }); }
            }
        }
        finally
        {
            foreach (var toast in notifications.Values) notifier.Hide(toast);
            ToastNotificationManager.History.Clear(launch.ApplicationId);
            writer?.Dispose();
        }
    }

    internal static async Task Reopen(string input)
    {
        var launch = JsonSerializer.Deserialize<NotificationLaunch>(File.ReadAllText(input))!;
        using var cancel = new CancellationTokenSource(TimeSpan.FromSeconds(60));
        var client = File.Exists(launch.Executable) ? await ClientInstallation.ResolvePath(launch.Executable, cancel.Token) : await ClientInstallation.Detect(cancel.Token);
        await Launcher.Start(client, launch.Server, new Progress<string>(), cancel.Token);
    }
}

public sealed class ToastActivation : IDisposable
{
    private readonly uint cookie;
    private readonly Factory factory;

    internal ToastActivation(NotificationLaunch launch, string input, Action<string, IReadOnlyDictionary<string, string>> activate)
    {
        using (var key = Registry.CurrentUser.CreateSubKey("Software\\Classes\\AppUserModelId\\" + launch.ApplicationId))
        {
            key.SetValue("DisplayName", "Codex2API · " + new Uri(launch.Server).Authority);
            key.SetValue("CustomActivator", launch.ClassId);
        }
        using (var key = Registry.CurrentUser.CreateSubKey("Software\\Classes\\CLSID\\" + launch.ClassId + "\\LocalServer32"))
            key.SetValue("", PackagedApplication.QuoteArgument(Environment.ProcessPath!) + " --notification-reopen " + PackagedApplication.QuoteArgument(input));
        factory = new Factory(activate);
        var classId = Guid.Parse(launch.ClassId);
        Marshal.ThrowExceptionForHR(CoRegisterClassObject(ref classId, factory, 4, 1, out cookie));
    }

    public void Dispose() => CoRevokeClassObject(cookie);

    [ComVisible(true), ClassInterface(ClassInterfaceType.None)]
    public sealed class Factory(Action<string, IReadOnlyDictionary<string, string>> activate) : IClassFactory
    {
        public int CreateInstance(nint outer, ref Guid interfaceId, out nint instance)
        {
            instance = 0;
            if (outer != 0) return unchecked((int)0x80040110);
            var unknown = Marshal.GetIUnknownForObject(new Callback(activate));
            try { return Marshal.QueryInterface(unknown, in interfaceId, out instance); }
            finally { Marshal.Release(unknown); }
        }
        public int LockServer(bool locked) => 0;
    }

    [ComVisible(true), ClassInterface(ClassInterfaceType.None)]
    public sealed class Callback(Action<string, IReadOnlyDictionary<string, string>> activate) : INotificationActivationCallback
    {
        public void Activate(string applicationId, string arguments, nint data, uint count)
        {
            var values = new Dictionary<string, string>();
            for (var index = 0; index < count; index++)
            {
                var item = data + index * IntPtr.Size * 2;
                values[Marshal.PtrToStringUni(Marshal.ReadIntPtr(item)) ?? ""] = Marshal.PtrToStringUni(Marshal.ReadIntPtr(item + IntPtr.Size)) ?? "";
            }
            activate(arguments, values);
        }
    }

    [ComVisible(true), Guid("00000001-0000-0000-C000-000000000046"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    public interface IClassFactory
    {
        [PreserveSig] int CreateInstance(nint outer, ref Guid interfaceId, out nint instance);
        [PreserveSig] int LockServer([MarshalAs(UnmanagedType.Bool)] bool locked);
    }
    [ComVisible(true), Guid("53E31837-6600-4A81-9395-75CFFE746F94"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    public interface INotificationActivationCallback
    {
        void Activate([MarshalAs(UnmanagedType.LPWStr)] string applicationId, [MarshalAs(UnmanagedType.LPWStr)] string arguments, nint data, uint count);
    }
    [DllImport("ole32.dll")] private static extern int CoRegisterClassObject(ref Guid classId, [MarshalAs(UnmanagedType.Interface)] IClassFactory factory, uint context, uint flags, out uint cookie);
    [DllImport("ole32.dll")] private static extern int CoRevokeClassObject(uint cookie);
}
