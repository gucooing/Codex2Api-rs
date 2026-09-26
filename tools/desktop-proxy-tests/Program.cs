using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.Json.Nodes;
using DesktopProxy;
using DesktopProxy.NativeHook;

if (args[0] == "--self-test")
{
    var block = Native.VirtualAlloc(0, 4096, 0x3000, 4);
    if (block == 0) throw new System.ComponentModel.Win32Exception();
    try
    {
        var yes = block + 512; var no = block + 528; var predicate = block + 128;
        Marshal.Copy(Convert.FromHexString("B801000000415FC3"), 0, yes, 8);
        Marshal.Copy(Convert.FromHexString("31C0415FC3"), 0, no, 5);
        var code = HttpCompatibility.Predicate(yes, no); Marshal.Copy(code, 0, predicate, code.Length);
        var entry = Convert.FromHexString("41574989CF4989D1FF2500000000").Concat(BitConverter.GetBytes((long)predicate)).ToArray();
        Marshal.Copy(entry, 0, block, entry.Length);
        if (!Native.VirtualProtect(block, 4096, 0x20, out _)) throw new System.ComponentModel.Win32Exception();
        var test = Marshal.GetDelegateForFunctionPointer<Native.Predicate>(block);
        foreach (var value in new[] { "http", "https", "ftp", "file", "httpx", "htts", "h", "", "httpsx" })
        {
            var bytes = Marshal.StringToHGlobalAnsi(value);
            try { if ((test(bytes, value.Length) != 0) != (value is "http" or "https")) throw new InvalidOperationException("Scheme predicate failed: " + value); }
            finally { Marshal.FreeHGlobal(bytes); }
        }
        if (HttpCompatibility.Inspect(args[1]).Length != 2) throw new InvalidOperationException("Native routing sites missing.");
        File.WriteAllText(args[2], "PASS: emitted native predicate accepts HTTP/HTTPS only; both installed workspace and inference validators identified.");
    }
    finally { Native.VirtualFree(block, 0, 0x8000); }
}
else if (args[0] == "--prepare")
{
    var process = Native.OpenProcess(0xc38, false, int.Parse(args[1]));
    if (process == 0) throw new System.ComponentModel.Win32Exception();
    try
    {
        var path = new StringBuilder(32768); var length = path.Capacity;
        if (!Native.QueryFullProcessImageNameW(process, 0, path, ref length) || !Path.GetFullPath(path.ToString()).Equals(Path.GetFullPath(args[2]), StringComparison.OrdinalIgnoreCase))
            throw new InvalidOperationException("Fixture executable mismatch.");
        HttpCompatibility.Apply(process, args[2]);
        if (Native.NtResumeProcess(process) != 0) throw new InvalidOperationException("Fixture resume failed.");
    }
    finally { Native.CloseHandle(process); }
}
else if (args[0] == "--verify")
{
    var process = Native.OpenProcess(0x410, false, int.Parse(args[1]));
    if (process == 0) throw new System.ComponentModel.Win32Exception();
    try { if (!HttpCompatibility.Verify(process, args[2])) throw new InvalidOperationException("HTTP compatibility was not applied to the running native process."); }
    finally { Native.CloseHandle(process); }
}
else if (args[0] == "--route-config")
{
    var input = JsonNode.Parse(File.ReadAllText(args[2]))!.AsObject();
    File.WriteAllText(args[3], new AddressPolicy(args[1]).NativeConfig(input).ToJsonString());
}
else if (args[0] == "--package")
{
    PackagedApplication.StartHost(args[1], Environment.ProcessPath!, ["--run", ..args.Skip(2)]);
}
else if (args[0] == "--run")
{
    var report = args[2];
    try
    {
        var start = new ProcessStartInfo(args[1]) { UseShellExecute = false, CreateNoWindow = true, RedirectStandardOutput = true, RedirectStandardError = true };
        foreach (var arg in args.Skip(3)) start.ArgumentList.Add(arg);
        // Test settings are passed through a file because package activation
        // starts with its registered environment rather than the test shell's.
        if (File.Exists(report + ".env.json"))
            foreach (var (key, value) in JsonNode.Parse(File.ReadAllText(report + ".env.json"))!.AsObject()) start.Environment[key] = value?.GetValue<string>();
        using var process = Process.Start(start)!;
        var stdout = process.StandardOutput.ReadToEndAsync(); var stderr = process.StandardError.ReadToEndAsync();
        await process.WaitForExitAsync();
        File.WriteAllText(report + ".stdout.txt", await stdout); File.WriteAllText(report + ".stderr.txt", await stderr);
        File.WriteAllText(report, process.ExitCode.ToString());
    }
    catch (Exception error) { File.WriteAllText(report, error.ToString()); }
}
else throw new ArgumentException("Unknown fixture operation.");

internal static class Native
{
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)] internal delegate int Predicate(nint value, nint length);
    [DllImport("kernel32.dll", SetLastError = true)] internal static extern nint VirtualAlloc(nint address, nuint size, uint allocation, uint protection);
    [DllImport("kernel32.dll", SetLastError = true)] internal static extern bool VirtualProtect(nint address, nuint size, uint protection, out uint previous);
    [DllImport("kernel32.dll")] internal static extern bool VirtualFree(nint address, nuint size, uint type);
    [DllImport("kernel32.dll", SetLastError = true)] internal static extern nint OpenProcess(uint access, bool inherit, int pid);
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)] internal static extern bool QueryFullProcessImageNameW(nint process, uint flags, StringBuilder name, ref int length);
    [DllImport("ntdll.dll")] internal static extern int NtResumeProcess(nint process);
    [DllImport("kernel32.dll")] internal static extern bool CloseHandle(nint handle);
}
