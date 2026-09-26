using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;

namespace DesktopProxy.NativeHook;

internal static unsafe class NativeEntry
{
    internal static string StatusPath = "";
    internal static string Server = "";
    internal static string Archive = "";
    internal static string ComponentFile = "";
    internal static string? Failure;
    [UnmanagedCallersOnly(EntryPoint = "InitializeHook", CallConvs = [typeof(CallConvStdcall)])]
    internal static uint Initialize(nint parameter)
    {
        try
        {
            StatusPath = Marshal.PtrToStringUni(parameter) ?? throw new InvalidOperationException("Missing hook status path.");
            Server = Environment.GetEnvironmentVariable("CODEX2API_HOOK_SERVER") ?? "";
            Archive = Environment.GetEnvironmentVariable("CODEX2API_HOOK_ARCHIVE") ?? "";
            ComponentFile = Environment.GetEnvironmentVariable("CODEX2API_HOOK_FILE") ?? "";
            IatHook.Install();
            File.WriteAllText(StatusPath, "native-hook-loaded");
            return 0;
        }
        catch (Exception e)
        {
            if (StatusPath.Length > 0) File.WriteAllText(StatusPath, "ERROR: " + e.Message);
            return 1;
        }
    }

    internal static void NodeModuleLoaded(nint env)
    {
        if (Failure is not null) throw new InvalidOperationException(Failure);
        NetworkHook.Install(env);
        if (StatusPath.Length > 0) File.WriteAllText(StatusPath, "routing-installed");
    }

    [UnmanagedCallersOnly(EntryPoint = "napi_register_module_v1", CallConvs = [typeof(CallConvCdecl)])]
    internal static nint RegisterWorker(nint env, nint exports)
    {
        try { NetworkHook.Install(env); }
        catch (Exception e) { Api.napi_throw_error(env, null, e.Message); }
        return exports;
    }
}
