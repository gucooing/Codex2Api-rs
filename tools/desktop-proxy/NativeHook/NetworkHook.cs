using System.Text.Json.Nodes;
using System.Text.RegularExpressions;

namespace DesktopProxy.NativeHook;

internal sealed class NetworkHook(Js js, AddressPolicy policy)
{
    internal static void Install(nint env)
    {
        var js = new Js(env);
        if (js.Has(js.Global, "__codex2apiNativeRouting")) return;
        var server = NativeEntry.Server;
        var hook = new NetworkHook(js, new AddressPolicy(server));
        hook.Install();
        js.Set(js.Global, "__codex2apiNativeRouting", js.Bool(true));
    }

    private void Install()
    {
        InstallOriginValidation();
        var http = js.Require("node:http"); var https = js.Require("node:https");
        var httpRequest = js.Hold(js.Get(http, "request")); var httpsRequest = js.Hold(js.Get(https, "request"));
        foreach (var (transport, scheme, original) in new[] { (http, "http", httpRequest), (https, "https", httpsRequest) })
        {
            var request = js.Function((receiver, args) =>
            {
                var mapped = RequestArguments(args, scheme);
                return mapped is null ? js.Invoke(original.Value, receiver, args)
                    : js.Invoke(mapped.Value.Scheme == "http" ? httpRequest.Value : httpsRequest.Value, receiver, mapped.Value.Arguments);
            });
            js.Set(transport, "request", request);
            var requestRef = js.Hold(request);
            js.Set(transport, "get", js.Function((receiver, args) => { var response = js.Invoke(requestRef.Value, receiver, args); js.Call(response, "end"); return response; }, requestRef));
        }
        WrapFetch(js.Global, "fetch");
        var webSocket = js.Get(js.Global, "WebSocket");
        if (js.Type(webSocket) == 7)
        {
            var original = js.Hold(webSocket);
            js.Set(js.Global, "WebSocket", js.PreserveConstructor(js.Function((_, args) =>
            {
                if (args.Length > 0) args[0] = js.Text(policy.Map(js.String(args[0])));
                return js.New(original.Value, args);
            }, original), webSocket));
        }
        InstallWorkers(); InstallHttp2(); InstallNativeTransport();
        js.Call(js.Require("node:module"), "syncBuiltinESMExports");
        var electron = js.TryRequire("electron");
        if (!js.IsObject(electron) || !js.Has(electron, "app") || !js.Has(electron, "net")) return;
        var net = js.Get(electron, "net"); var netRequest = js.Hold(js.Get(net, "request"));
        js.Set(net, "request", js.Function((receiver, args) => js.Invoke(netRequest.Value, receiver, RequestArguments(args, "https")?.Arguments ?? args), netRequest));
        WrapFetch(net, "fetch");
        var shell = js.Get(electron, "shell"); var external = js.Hold(js.Get(shell, "openExternal"));
        js.Set(shell, "openExternal", js.Function((receiver, args) => { if (args.Length > 0) args[0] = js.Text(policy.Map(js.String(args[0]))); return js.Invoke(external.Value, receiver, args); }, external));
        var app = js.Get(electron, "app");
        js.Call(app, "on", js.Text("session-created"), js.Function((_, args) => { AttachSession(args[0]); return js.Undefined; }));
        var electronRef = js.Hold(electron);
        js.Call(js.Call(app, "whenReady"), "then", js.Function((_, _) => { AttachSession(js.Get(js.Get(electronRef.Value, "session"), "defaultSession")); return js.Undefined; }, electronRef));
    }

    private void InstallOriginValidation()
    {
        if (!policy.Origin.StartsWith("http://", StringComparison.OrdinalIgnoreCase)) return;
        // The Desktop account reader and persisted workspace-routing schema both
        // call URL.parse(origin), then test protocol === "https:" and origin ===
        // input. Adapt that parser result for this one configured HTTP origin.
        // The origin/href remain HTTP; requests never use a TLS endpoint or a
        // fabricated workspace response. Paths, credentials and other origins
        // continue through the original parser and validation unchanged.
        var url = js.Get(js.Global, "URL"); var parse = js.Hold(js.Get(url, "parse"));
        js.Set(url, "parse", js.Function((receiver, args) =>
        {
            var result = js.Invoke(parse.Value, receiver, args);
            if (args.Length > 0 && js.Type(args[0]) == 4 && js.String(args[0]) == policy.Origin
                && js.IsObject(result) && js.OptionalString(js.Get(result, "origin")) == policy.Origin
                && js.OptionalString(js.Get(result, "protocol")) == "http:")
            {
                var descriptor = js.Object(); js.Set(descriptor, "value", js.Text("https:"));
                js.Call(js.Get(js.Global, "Object"), "defineProperty", result, js.Text("protocol"), descriptor);
            }
            return result;
        }, parse));
    }

    private (string Scheme, nint[] Arguments)? RequestArguments(nint[] args, string scheme)
    {
        if (args.Length == 0) return null;
        var input = args[0];
        var value = js.Type(input) == 4 ? js.String(input) : js.OptionalString(js.Get(input, "href"));
        var raw = value is null ? input : args.Length > 1 && js.IsObject(args[1]) ? args[1] : js.Object();
        if (js.Has(raw, "socketPath")) return null;
        value ??= js.OptionalString(js.Get(raw, "url"));
        var host = js.OptionalString(js.Get(raw, "hostname")) ?? js.OptionalString(js.Get(raw, "host"));
        Uri? source;
        if (value is null)
        {
            host ??= "localhost";
            if (host.Contains(':') && !host.StartsWith('[') && host.Count(c => c == ':') > 1) host = "[" + host + "]";
            var port = js.OptionalString(js.Get(raw, "port"));
            var protocol = js.OptionalString(js.Get(raw, "protocol"))?.TrimEnd(':') ?? scheme;
            value = protocol + "://" + host + (port is null ? "" : ":" + port) + (js.OptionalString(js.Get(raw, "path")) ?? "/");
        }
        if (!Uri.TryCreate(value, UriKind.Absolute, out source)) return null;
        var builder = new UriBuilder(source);
        if (host is not null && Uri.CheckHostName(host.Trim('[', ']')) != UriHostNameType.Unknown) builder.Host = host;
        if (int.TryParse(js.OptionalString(js.Get(raw, "port")), out var explicitPort)) builder.Port = explicitPort;
        var explicitPath = js.OptionalString(js.Get(raw, "path"));
        if (explicitPath is not null) { var parts = explicitPath.Split('?', 2); builder.Path = parts[0]; builder.Query = parts.Length > 1 ? parts[1] : ""; }
        value = builder.Uri.AbsoluteUri;
        var mapped = policy.Map(value);
        if (mapped == value) return null;
        var target = new Uri(mapped); var options = js.Clone(raw);
        foreach (var key in new[] { "host", "hostname", "port", "protocol", "path", "url", "servername", "createConnection", "socketPath", "lookup" }) js.Delete(options, key);
        js.Set(options, "protocol", js.Text(target.Scheme + ":")); js.Set(options, "hostname", js.Text(target.Host));
        js.Set(options, "port", js.Text(target.Port.ToString())); js.Set(options, "path", js.Text(target.PathAndQuery));
        if (js.Has(options, "headers")) js.Set(options, "headers", CleanHeaders(js.Get(options, "headers")));
        var agent = js.Get(options, "agent");
        if (js.IsObject(agent) && js.OptionalString(js.Get(agent, "protocol")) is string agentProtocol && agentProtocol != target.Scheme + ":") js.Delete(options, "agent");
        var callback = args.FirstOrDefault(value => js.Type(value) == 7);
        return (target.Scheme, callback == 0 ? [options] : [options, callback]);
    }

    private nint CleanHeaders(nint headers)
    {
        static bool Remove(string name) => name.Equals("host", StringComparison.OrdinalIgnoreCase) || name.Equals("x-openai-account-routing-override", StringComparison.OrdinalIgnoreCase);
        if (js.IsArray(headers))
        {
            var original = js.Items(headers); var result = new List<nint>();
            for (var i = 0; i + 1 < original.Length; i += 2) if (!Remove(js.String(original[i]))) { result.Add(original[i]); result.Add(original[i + 1]); }
            return js.Array(result);
        }
        if (!js.IsObject(headers)) return headers;
        var copy = js.Clone(headers);
        foreach (var key in js.Keys(copy).Where(Remove)) js.Delete(copy, key);
        return copy;
    }

    private void WrapFetch(nint owner, string name)
    {
        var value = js.Get(owner, name); if (js.Type(value) != 7) return;
        var original = js.Hold(value);
        js.Set(owner, name, js.Function((receiver, args) =>
        {
            if (args.Length == 0) return js.Invoke(original.Value, receiver, args);
            var input = args[0]; var url = js.Type(input) == 4 ? js.String(input) : js.OptionalString(js.Get(input, "url")) ?? js.OptionalString(js.Get(input, "href"));
            if (url is null || !policy.IsRouted(url)) return js.Invoke(original.Value, receiver, args);
            var init = args.Length > 1 ? args[1] : js.Undefined;
            var request = js.New(js.Get(js.Global, "Request"), input, init);
            var extensions = js.Object();
            foreach (var key in new[] { "session", "useSessionCookies", "bypassCustomProtocolHandlers", "dispatcher" }) if (js.Has(init, key)) js.Set(extensions, key, js.Get(init, key));
            return Fetch(original, receiver, request, extensions, js.String(js.Get(request, "redirect")), 0);
        }, original));
    }

    private nint Fetch(Js.Ref original, nint receiver, nint request, nint extensions, string mode, int count)
    {
        var url = js.String(js.Get(request, "url")); var mapped = policy.Map(url);
        if (mapped != url)
        {
            request = js.New(js.Get(js.Global, "Request"), js.Text(mapped), request);
            var headers = js.Get(request, "headers"); js.Call(headers, "delete", js.Text("host")); js.Call(headers, "delete", js.Text("x-openai-account-routing-override"));
        }
        var retry = mode == "follow" && js.Type(js.Get(request, "body")) is not (0 or 1) ? js.Call(request, "clone") : request;
        var options = js.Object(); js.Set(options, "redirect", js.Text("manual"));
        var promise = js.Invoke(original.Value, receiver, js.New(js.Get(js.Global, "Request"), request, options), extensions);
        var saved = js.Hold(retry); var ext = js.Hold(extensions); var self = js.IsObject(receiver) ? js.Hold(receiver) : null;
        return js.Call(promise, "then", js.Function((_, args) =>
        {
            var response = args[0]; var status = js.Integer(js.Get(response, "status"));
            var location = js.OptionalString(js.Call(js.Get(response, "headers"), "get", js.Text("location")));
            if (status is not (301 or 302 or 303 or 307 or 308) || location is null || mode == "manual") return response;
            var body = js.Get(response, "body"); if (js.IsObject(body)) js.Call(body, "cancel");
            if (mode == "error" || count >= 20) throw new InvalidOperationException("Request redirect rejected.");
            var previous = saved.Value; var previousUrl = new Uri(js.String(js.Get(previous, "url")));
            var target = new Uri(policy.Map(new Uri(previousUrl, location).AbsoluteUri));
            var headers = js.New(js.Get(js.Global, "Headers"), js.Get(previous, "headers"));
            if (target.GetLeftPart(UriPartial.Authority) != previousUrl.GetLeftPart(UriPartial.Authority))
                foreach (var key in new[] { "authorization", "cookie", "proxy-authorization" }) js.Call(headers, "delete", js.Text(key));
            var method = js.String(js.Get(previous, "method")); var get = status == 303 && method is not ("GET" or "HEAD") || status is 301 or 302 && method == "POST";
            if (get) foreach (var key in new[] { "content-type", "content-length", "content-encoding", "content-language", "content-location" }) js.Call(headers, "delete", js.Text(key));
            var next = js.Object(); js.Set(next, "method", js.Text(get ? "GET" : method)); js.Set(next, "headers", headers);
            if (!get) js.Set(next, "body", js.Get(previous, "body"));
            foreach (var key in new[] { "signal", "credentials" }) js.Set(next, key, js.Get(previous, key));
            js.Set(next, "duplex", js.Text("half")); js.Set(next, "redirect", js.Text(mode));
            return Fetch(original, self?.Value ?? js.Undefined, js.New(js.Get(js.Global, "Request"), js.Text(target.AbsoluteUri), next), ext.Value, mode, count + 1);
        }, self is null ? [saved, ext] : [saved, ext, self]));
    }

    private void InstallWorkers()
    {
        var module = js.Require("node:worker_threads"); var value = js.Get(module, "Worker"); var original = js.Hold(value);
        var path = NativeEntry.ComponentFile;
        js.Set(module, "Worker", js.PreserveConstructor(js.Function((_, args) =>
        {
            var options = args.Length > 1 && js.IsObject(args[1]) ? js.Clone(args[1]) : js.Object();
            var supplied = js.Has(options, "execArgv") ? js.Get(options, "execArgv") : js.Get(js.Get(js.Global, "process"), "execArgv");
            var flag = "--require=" + path;
            js.Set(options, "execArgv", js.Array(js.Items(supplied).Where(arg => js.String(arg) != flag).Append(js.Text(flag))));
            return js.New(original.Value, args[0], options);
        }, original), value));
    }

    private void InstallHttp2()
    {
        var module = js.Require("node:http2"); var original = js.Hold(js.Get(module, "connect"));
        js.Set(module, "connect", js.Function((receiver, args) =>
        {
            var authority = new Uri(js.String(args[0])); args[0] = js.Text(policy.Map(authority.AbsoluteUri));
            var session = js.Invoke(original.Value, receiver, args); var request = js.Hold(js.Get(session, "request"));
            js.Set(session, "request", js.Function((connection, input) =>
            {
                var headers = input.Length > 0 && js.IsObject(input[0]) ? input[0] : js.Object();
                var source = new Uri(authority, js.OptionalString(js.Get(headers, ":path")) ?? "/").AbsoluteUri;
                var mapped = policy.Map(source);
                if (mapped == source) return js.Invoke(request.Value, connection, input);
                var target = new Uri(mapped); var copy = js.Clone(headers);
                js.Set(copy, ":authority", js.Text(target.Authority)); js.Set(copy, ":scheme", js.Text(target.Scheme)); js.Set(copy, ":path", js.Text(target.PathAndQuery));
                return js.Invoke(request.Value, connection, new[] { copy }.Concat(input.Skip(1)).ToArray());
            }, request));
            return session;
        }, original));
    }

    private void AttachSession(nint session)
    {
        if (js.Has(session, "__codex2apiRouting")) return;
        js.Set(session, "__codex2apiRouting", js.Bool(true));
        var webRequest = js.Get(session, "webRequest"); var register = js.Hold(js.Get(webRequest, "onBeforeRequest"));
        var owner = js.Hold(webRequest);
        var wrapper = js.Function((_, args) =>
        {
            var filter = args.Length > 0 && js.IsObject(args[0]) && js.Type(args[0]) != 7 ? args[0] : js.Undefined;
            var listener = args.LastOrDefault(value => js.Type(value) == 7);
            var filterRef = js.IsObject(filter) ? js.Hold(filter) : null; var listenerRef = listener != 0 ? js.Hold(listener) : null;
            var all = js.Object(); js.Set(all, "urls", js.Array([js.Text("<all_urls>")]));
            var callback = js.Function((_, input) =>
            {
                var details = input[0]; var complete = js.Hold(input[1]); var url = js.String(js.Get(details, "url"));
                var finish = js.Function((_, response) =>
                {
                    var result = response.Length > 0 && js.IsObject(response[0]) ? response[0] : js.Object();
                    if (js.Boolean(js.Get(result, "cancel"))) return js.Invoke(complete.Value, js.Undefined, result);
                    var originalUrl = js.OptionalString(js.Get(result, "redirectURL")) ?? url; var target = policy.Map(originalUrl);
                    if (target != originalUrl) { result = js.Clone(result); js.Set(result, "redirectURL", js.Text(target)); }
                    return js.Invoke(complete.Value, js.Undefined, result);
                }, complete);
                var matches = filterRef is null || !js.Has(filterRef.Value, "urls") || js.Items(js.Get(filterRef.Value, "urls")).Select(js.String)
                    .Any(pattern => pattern == "<all_urls>" || Regex.IsMatch(url, "^" + Regex.Escape(pattern).Replace("\\*", ".*") + "$", RegexOptions.CultureInvariant));
                return listenerRef is not null && matches ? js.Invoke(listenerRef.Value, js.Undefined, details, finish) : js.Invoke(finish, js.Undefined);
            }, new IDisposable?[] { filterRef, listenerRef }.OfType<IDisposable>().ToArray());
            return js.Invoke(register.Value, owner.Value, all, callback);
        }, register, owner);
        js.Set(webRequest, "onBeforeRequest", wrapper); js.Invoke(wrapper, webRequest);
        WrapFetch(session, "fetch");
    }

    private void InstallNativeTransport()
    {
        var module = js.Require("node:child_process"); var spawn = js.Hold(js.Get(module, "spawn"));
        js.Set(module, "spawn", js.Function((receiver, args) =>
        {
            var isServer = args.Length > 1 && js.IsArray(args[1]) && js.Items(args[1]).Any(value => js.String(value) == "app-server");
            JsonObject? saved = null;
            if (isServer)
            {
                var options = args.Length > 2 ? args[2] : js.Undefined;
                var env = js.Get(options, "env");
                var environment = js.IsObject(env) ? js.Keys(env).Where(key => js.Type(js.Get(env, key)) is not (0 or 1)).ToDictionary(key => key, key => js.String(js.Get(env, key))) : null;
                var configured = NativeTransport.Configure(js.String(args[0]), js.Items(args[1]).Select(js.String).ToArray(), js.OptionalString(js.Get(options, "cwd")), environment, policy);
                args[1] = js.Array(configured.Arguments.Select(js.Text)); saved = configured.Saved;
            }
            var child = js.Invoke(spawn.Value, receiver, args);
            if (!isServer) return child;
            var stdin = js.Get(child, "stdin"); if (!js.IsObject(stdin)) return child;
            var write = js.Hold(js.Get(stdin, "write"));
            js.Set(stdin, "write", js.Function((stream, input) =>
            {
                if (input.Length > 0)
                {
                    var text = js.String(input[0]);
                    try
                    {
                        if (JsonNode.Parse(text) is JsonObject request && request["params"] is JsonObject parameters)
                        {
                            var changed = false;
                            if (request["method"]?.GetValue<string>() == "account/login/start") { parameters["useHostedLoginSuccessPage"] = false; changed = true; }
                            if (parameters.ContainsKey("config") || parameters.ContainsKey("modelProvider"))
                            {
                                parameters["config"] = policy.NativeConfig(saved!, parameters["config"] as JsonObject); changed = true;
                            }
                            if (changed) input[0] = js.Text(request.ToJsonString() + "\n");
                        }
                    }
                    catch (System.Text.Json.JsonException) { }
                }
                return js.Invoke(write.Value, stream, input);
            }, write));
            return child;
        }, spawn));
    }
}
