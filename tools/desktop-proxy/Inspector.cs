using System.Collections.Concurrent;
using System.Net.WebSockets;
using System.Text;
using System.Text.Json;

namespace DesktopProxy;

internal sealed class Inspector : IAsyncDisposable
{
    private readonly ClientWebSocket socket = new();
    private readonly ConcurrentDictionary<int, TaskCompletionSource<JsonElement>> pending = new();
    private readonly CancellationTokenSource lifetime = new();
    private readonly SemaphoreSlim sending = new(1);
    private int sequence;
    public TaskCompletionSource<JsonElement> Paused { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
    public async Task Connect(Uri uri, CancellationToken cancel)
    {
        socket.Options.Proxy = null;
        await socket.ConnectAsync(uri, cancel);
        _ = ReadLoop();
    }
    private async Task ReadLoop()
    {
        try
        {
            var buffer = new byte[16384];
            while (!lifetime.IsCancellationRequested)
            {
                using var message = new MemoryStream();
                WebSocketReceiveResult result;
                do
                {
                    result = await socket.ReceiveAsync(buffer, lifetime.Token);
                    if (result.MessageType == WebSocketMessageType.Close) return;
                    message.Write(buffer, 0, result.Count);
                    if (message.Length > 16 * 1024 * 1024) throw new InvalidOperationException("调试消息超过大小限制。");
                } while (!result.EndOfMessage);
                using var doc = JsonDocument.Parse(message.ToArray());
                var root = doc.RootElement;
                if (root.TryGetProperty("id", out var id) && pending.TryRemove(id.GetInt32(), out var completion))
                {
                    if (root.TryGetProperty("error", out var error)) completion.TrySetException(new InvalidOperationException(error.ToString()));
                    else completion.TrySetResult(root.GetProperty("result").Clone());
                }
                else if (root.TryGetProperty("method", out var method) && method.GetString() == "Debugger.paused")
                    Paused.TrySetResult(root.GetProperty("params").Clone());
            }
        }
        catch (Exception e)
        {
            Paused.TrySetException(e);
            foreach (var item in pending.Values) item.TrySetException(e);
        }
    }
    public async Task<JsonElement> Send(string method, object? args, CancellationToken cancel)
    {
        var id = Interlocked.Increment(ref sequence);
        var completion = new TaskCompletionSource<JsonElement>(TaskCreationOptions.RunContinuationsAsynchronously);
        pending[id] = completion;
        try
        {
            var bytes = JsonSerializer.SerializeToUtf8Bytes(new { id, method, @params = args ?? new { } });
            await sending.WaitAsync(cancel);
            try { await socket.SendAsync(bytes, WebSocketMessageType.Text, true, cancel); }
            finally { sending.Release(); }
            return await completion.Task.WaitAsync(TimeSpan.FromSeconds(12), cancel);
        }
        finally { pending.TryRemove(id, out _); }
    }
    public static JsonElement Value(JsonElement reply)
    {
        if (reply.TryGetProperty("exceptionDetails", out var error))
            throw new InvalidOperationException("地址 hook 安装失败：" + (reply.GetProperty("result").TryGetProperty("description", out var description) ? description.GetString() : error.ToString()));
        return reply.GetProperty("result").TryGetProperty("value", out var value) ? value.Clone() : default;
    }
    public async ValueTask DisposeAsync()
    {
        if (socket.State == WebSocketState.Open)
        {
            using var timeout = new CancellationTokenSource(1000);
            try { await socket.CloseOutputAsync(WebSocketCloseStatus.NormalClosure, "", timeout.Token); } catch { }
        }
        lifetime.Cancel(); socket.Dispose(); lifetime.Dispose(); sending.Dispose();
    }
}
