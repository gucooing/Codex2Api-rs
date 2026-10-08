using System.Runtime.InteropServices;

namespace DesktopProxy;

internal static class PackagedApplication
{
    // Start this C# launcher with the registered package identity. It can then
    // create the original executable suspended with its own environment block.
    public static void StartHost(string appUserModelId, string executable, IEnumerable<string> arguments)
    {
        var instance = Activator.CreateInstance(Type.GetTypeFromCLSID(new Guid("168EB462-775F-42AE-9111-D714B2306C2E"))!)!;
        nint process = 0;
        try
        {
            Activate(instance, appUserModelId, executable, string.Join(" ", arguments.Select(QuoteArgument)), out process);
        }
        catch (COMException error)
        {
            throw new InvalidOperationException($"无法以客户端的应用包身份启动（0x{error.HResult:X8}）。", error);
        }
        finally
        {
            if (process != 0) CloseHandle(process);
            Marshal.ReleaseComObject(instance);
        }
    }

    internal static void Activate(object instance, string app, string executable, string arguments, out nint process)
    {
        // Windows exposes different IIDs for the same first two activation
        // methods. Query capabilities; replacing the old IID breaks older hosts.
        const uint options = 6 | 8; // Nonpackaged executable/process tree, no error UI.
        if (instance is IDesktopAppXActivator2 current)
            current.ActivateWithOptions(app, executable, arguments, options, 0, out process);
        else if (instance is IDesktopAppXActivator1 previous)
            previous.ActivateWithOptions(app, executable, arguments, options, 0, out process);
        else
            throw new PlatformNotSupportedException("当前 Windows 不支持所需的应用包启动接口，无法启动商店客户端。");
    }

    internal static string QuoteArgument(string value) => CommandLine.Quote(value);

    [ComImport, Guid("72E3A5B0-8FEA-485C-9F8B-822B16DBA17F"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    internal interface IDesktopAppXActivator1
    {
        void Activate([MarshalAs(UnmanagedType.LPWStr)] string app, [MarshalAs(UnmanagedType.LPWStr)] string executable,
            [MarshalAs(UnmanagedType.LPWStr)] string arguments, out nint process);
        void ActivateWithOptions([MarshalAs(UnmanagedType.LPWStr)] string app, [MarshalAs(UnmanagedType.LPWStr)] string executable,
            [MarshalAs(UnmanagedType.LPWStr)] string arguments, uint options, uint parentProcessId, out nint process);
    }

    // Repeat the common vtable prefix instead of inheriting/casting to v1:
    // a v2-only Windows implementation can reject QueryInterface for v1.
    [ComImport, Guid("F158268A-D5A5-45CE-99CF-00D6C3F3FC0A"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    internal interface IDesktopAppXActivator2
    {
        void Activate([MarshalAs(UnmanagedType.LPWStr)] string app, [MarshalAs(UnmanagedType.LPWStr)] string executable,
            [MarshalAs(UnmanagedType.LPWStr)] string arguments, out nint process);
        void ActivateWithOptions([MarshalAs(UnmanagedType.LPWStr)] string app, [MarshalAs(UnmanagedType.LPWStr)] string executable,
            [MarshalAs(UnmanagedType.LPWStr)] string arguments, uint options, uint parentProcessId, out nint process);
    }
    [DllImport("kernel32.dll")] private static extern bool CloseHandle(nint handle);
}
