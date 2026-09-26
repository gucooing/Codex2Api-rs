using System.Runtime.InteropServices;
using System.Text;

namespace DesktopProxy;

internal static class PackagedApplication
{
    // Start this C# launcher with the registered package identity. It can then
    // create the original executable suspended with its own environment block.
    public static void StartHost(string appUserModelId, string executable, IEnumerable<string> arguments)
    {
        var instance = Activator.CreateInstance(Type.GetTypeFromCLSID(new Guid("168EB462-775F-42AE-9111-D714B2306C2E"))!)!;
        try
        {
            ((IDesktopAppXActivator)instance).ActivateWithOptions(appUserModelId, executable,
                string.Join(" ", arguments.Select(QuoteArgument)), 6 | 8, 0, out var process);
            if (process != 0) CloseHandle(process);
        }
        finally { Marshal.ReleaseComObject(instance); }
    }

    internal static string QuoteArgument(string value)
    {
        var result = new StringBuilder("\"");
        var slashes = 0;
        foreach (var c in value)
        {
            if (c == '\\') { slashes++; continue; }
            result.Append('\\', c == '"' ? slashes * 2 + 1 : slashes).Append(c);
            slashes = 0;
        }
        return result.Append('\\', slashes * 2).Append('"').ToString();
    }

    [ComImport, Guid("72E3A5B0-8FEA-485C-9F8B-822B16DBA17F"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    private interface IDesktopAppXActivator
    {
        void Activate([MarshalAs(UnmanagedType.LPWStr)] string app, [MarshalAs(UnmanagedType.LPWStr)] string executable,
            [MarshalAs(UnmanagedType.LPWStr)] string arguments, out nint process);
        void ActivateWithOptions([MarshalAs(UnmanagedType.LPWStr)] string app, [MarshalAs(UnmanagedType.LPWStr)] string executable,
            [MarshalAs(UnmanagedType.LPWStr)] string arguments, uint options, uint parentProcessId, out nint process);
    }
    [DllImport("kernel32.dll")] private static extern bool CloseHandle(nint handle);
}
