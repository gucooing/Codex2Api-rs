using System.Text.Json.Nodes;

namespace DesktopProxy.NativeHook;

internal sealed class AddressPolicy(string server)
{
    private readonly Uri service = new(server);
    public string Origin => service.GetLeftPart(UriPartial.Authority);
    private static readonly string[] Domains = ["chatgpt.com", "chatgpt-staging.com", "openai.com", "openai.internal", "oaistatic.com", "oaiusercontent.com", "oaistatsig.com"];
    public string Map(string value)
    {
        if (!Uri.TryCreate(value, UriKind.Absolute, out var source) || source.Scheme is not ("http" or "https" or "ws" or "wss")) return value;
        var prefix = service.AbsolutePath.TrimEnd('/');
        if (source.GetLeftPart(UriPartial.Authority) == Origin && prefix.Length > 0
            && (source.AbsolutePath == "/backend-api" || source.AbsolutePath.StartsWith("/backend-api/", StringComparison.Ordinal)))
            return Origin + prefix + source.PathAndQuery + source.Fragment;
        if (!Domains.Any(domain => source.Host.Equals(domain, StringComparison.OrdinalIgnoreCase) || source.Host.EndsWith("." + domain, StringComparison.OrdinalIgnoreCase))
            && !source.Host.Equals("oaisidekickupdates.blob.core.windows.net", StringComparison.OrdinalIgnoreCase)) return value;
        if (source.UserInfo.Length != 0) throw new InvalidOperationException("Service URL contains credentials.");
        var path = source.AbsolutePath;
        if (path != prefix && !path.StartsWith(prefix + "/", StringComparison.Ordinal)) path = prefix + path;
        var scheme = source.Scheme is "ws" or "wss" ? service.Scheme == "https" ? "wss" : "ws" : service.Scheme;
        var authority = service.Authority;
        var result = scheme + "://" + authority + path + source.Query + source.Fragment;
        if (source.AbsolutePath == "/codex/desktop-auth")
        {
            var query = source.Query.TrimStart('?').Split('&', StringSplitOptions.RemoveEmptyEntries).Select(part =>
            {
                var pieces = part.Split('=', 2);
                return Uri.UnescapeDataString(pieces[0]) == "authorize_url" && pieces.Length == 2
                    ? pieces[0] + "=" + Uri.EscapeDataString(Map(Uri.UnescapeDataString(pieces[1]))) : part;
            });
            result = service.GetLeftPart(UriPartial.Authority) + "/codex/desktop-auth?" + string.Join('&', query);
        }
        return result;
    }
    public bool IsRouted(string value)
        => Uri.TryCreate(value, UriKind.Absolute, out var uri) && (Map(value) != value
            || uri.GetLeftPart(UriPartial.Authority) == service.GetLeftPart(UriPartial.Authority)
            && (uri.AbsolutePath == service.AbsolutePath.TrimEnd('/') || uri.AbsolutePath.StartsWith(service.AbsolutePath.TrimEnd('/') + "/", StringComparison.Ordinal)));

    public JsonObject NativeConfig(JsonObject saved, JsonObject? supplied = null)
    {
        var result = supplied?.DeepClone().AsObject() ?? new JsonObject();
        void MapFields(JsonObject source, string prefix = "")
        {
            foreach (var (key, value) in source)
            {
                var name = prefix + key;
                if (value is JsonObject nested) { MapFields(nested, name + "."); continue; }
                if (value is not JsonValue scalar || !scalar.TryGetValue<string>(out var address)) continue;
                if (!(key.EndsWith("url", StringComparison.Ordinal) || key.EndsWith("endpoint", StringComparison.Ordinal) || key.EndsWith("issuer", StringComparison.Ordinal))) continue;
                var mapped = Map(address);
                if (mapped != address) result[name] = mapped;
            }
        }
        MapFields(saved); if (supplied is not null) MapFields(supplied);
        result["chatgpt_base_url"] = Map("https://chatgpt.com/backend-api");
        result["openai_base_url"] = Map("https://chatgpt.com/backend-api/codex");
        var providers = saved["model_providers"]?.DeepClone().AsObject() ?? new JsonObject();
        if (supplied?["model_providers"] is JsonObject extra) foreach (var (key, value) in extra) providers[key] = value?.DeepClone();
        foreach (var (name, node) in providers)
        {
            if (node is not JsonObject provider || name == "openai" || name.StartsWith("amazon-bedrock", StringComparison.Ordinal) || provider["aws"] is not null) continue;
            var key = "model_providers." + name + ".base_url";
            var address = supplied?[key]?.GetValue<string>() ?? provider["base_url"]?.GetValue<string>()
                ?? (provider["requires_openai_auth"]?.GetValue<bool>() == true ? "https://chatgpt.com/backend-api/codex" : "https://api.openai.com/v1");
            var target = Map(address);
            if (target != address)
            {
                if (name.Any(c => !char.IsAsciiLetterOrDigit(c) && c is not ('_' or '-'))) throw new InvalidOperationException("无法识别模型服务配置名称。");
                result[key] = target;
            }
        }
        return result;
    }
}
