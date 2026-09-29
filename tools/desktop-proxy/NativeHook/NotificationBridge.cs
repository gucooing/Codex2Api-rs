using System.Text.Json;

namespace DesktopProxy.NativeHook;

internal sealed class NotificationBridge(Js js)
{
    private readonly Dictionary<string, Js.Ref> notifications = new();
    private Js.Ref? connection;
    private string buffered = "";

    internal void Install(nint electron, string pipe)
    {
        var options = js.Object(); js.Set(options, "path", js.Text("\\\\.\\pipe\\" + pipe));
        var socket = js.Call(js.Require("node:net"), "createConnection", options);
        connection = js.Hold(socket);
        js.Call(socket, "setEncoding", js.Text("utf8"));
        js.Call(socket, "on", js.Text("data"), js.Function((_, arguments) =>
        {
            buffered += js.String(arguments[0]);
            if (buffered.Length > 131072) throw new InvalidOperationException("Notification response exceeds limit.");
            while (buffered.IndexOf('\n') is var newline && newline >= 0)
            {
                var line = buffered[..newline]; buffered = buffered[(newline + 1)..];
                Dispatch(line);
            }
            return js.Undefined;
        }));
        js.Call(socket, "on", js.Text("error"), js.Function((_, arguments) =>
        {
            foreach (var notification in notifications.Values.ToArray()) js.Call(notification.Value, "emit", js.Text("failed"), arguments[0]);
            return js.Undefined;
        }));
        js.Call(socket, "unref");
        var constructor = js.Get(electron, "Notification");
        if (js.Type(constructor) != 7) throw new InvalidOperationException("Unsupported Desktop notification API.");
        var prototype = js.Get(constructor, "prototype");
        var showDescriptor = js.Object();
        js.Set(showDescriptor, "configurable", js.Bool(true));
        js.Set(showDescriptor, "writable", js.Bool(true));
        js.Set(showDescriptor, "value", js.Function((receiver, _) =>
        {
            var token = js.OptionalString(js.Get(receiver, "__codex2apiNotificationToken")) ?? Guid.NewGuid().ToString("N");
            js.Set(receiver, "__codex2apiNotificationToken", js.Text(token));
            if (notifications.Remove(token, out var previous)) previous.Dispose();
            notifications[token] = js.Hold(receiver);
            var values = js.Object();
            foreach (var name in new[] { "title", "body", "silent", "timeoutType", "hasReply", "replyPlaceholder", "actions" }) js.Set(values, name, js.Get(receiver, name));
            var serialized = js.String(js.Call(js.Get(js.Global, "JSON"), "stringify", values));
            Send("{\"kind\":\"show\",\"token\":\"" + token + "\",\"options\":" + serialized + "}");
            return js.Undefined;
        }));
        js.Call(js.Get(js.Global, "Object"), "defineProperty", prototype, js.Text("show"), showDescriptor);
        var originalClose = js.Hold(js.Get(prototype, "close"));
        var closeDescriptor = js.Object();
        js.Set(closeDescriptor, "configurable", js.Bool(true));
        js.Set(closeDescriptor, "writable", js.Bool(true));
        js.Set(closeDescriptor, "value", js.Function((receiver, _) =>
        {
            var token = js.OptionalString(js.Get(receiver, "__codex2apiNotificationToken"));
            if (token is null) return js.Invoke(originalClose.Value, receiver);
            Send("{\"kind\":\"close\",\"token\":\"" + token + "\"}");
            return js.Undefined;
        }, originalClose));
        js.Call(js.Get(js.Global, "Object"), "defineProperty", prototype, js.Text("close"), closeDescriptor);
        if (Environment.GetEnvironmentVariable("CODEX2API_NOTIFICATION_TEST_REPORT") is { Length: > 0 } report)
        {
            var savedElectron = js.Hold(electron);
            js.Call(js.Call(js.Get(electron, "app"), "whenReady"), "then", js.Function((_, _) =>
            {
                var options = js.Object();
                js.Set(options, "title", js.Text("Codex2API 通知回归测试"));
                js.Set(options, "body", js.Text("验证原客户端的通知点击回到当前进程"));
                js.Set(options, "silent", js.Bool(true));
                var notice = js.New(js.Get(savedElectron.Value, "Notification"), options);
                js.Call(notice, "on", js.Text("click"), js.Function((_, _) => { File.WriteAllText(report, "click:" + Environment.ProcessId); return js.Undefined; }));
                js.Call(notice, "on", js.Text("show"), js.Function((_, _) => { File.WriteAllText(report + ".shown", "shown"); return js.Undefined; }));
                js.Call(notice, "show");
                File.WriteAllText(report + ".token", js.String(js.Get(notice, "__codex2apiNotificationToken")));
                return js.Undefined;
            }, savedElectron));
        }
    }

    private void Send(string message) => js.Call(connection!.Value, "write", js.Text(message + "\n"));

    private void Dispatch(string line)
    {
        using var response = JsonDocument.Parse(line);
        var message = response.RootElement;
        var token = message.GetProperty("token").GetString()!;
        if (!notifications.TryGetValue(token, out var notification)) return;
        var kind = message.GetProperty("kind").GetString()!;
        if (kind == "action") js.Call(notification.Value, "emit", js.Text(kind), js.Object(), js.Number(message.GetProperty("index").GetInt32()));
        else if (kind == "reply") js.Call(notification.Value, "emit", js.Text(kind), js.Object(), js.Text(message.GetProperty("reply").GetString() ?? ""));
        else if (kind == "failed") js.Call(notification.Value, "emit", js.Text(kind), js.Object(), js.Text(message.GetProperty("error").GetString() ?? ""));
        else js.Call(notification.Value, "emit", js.Text(kind), js.Object());
        if (kind is "close" or "failed") { notifications.Remove(token); notification.Dispose(); }
    }
}
