using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;

namespace DesktopProxy.NativeHook;

internal static unsafe class ProfileFileRouting
{
    private static string? sharedHome, privateHome;
    private static readonly Dictionary<string, nint> originals = new();

    internal static void Initialize()
    {
        sharedHome = Environment.GetEnvironmentVariable("CODEX2API_SHARED_HOME");
        privateHome = Environment.GetEnvironmentVariable("CODEX2API_PRIVATE_HOME");
        if (string.IsNullOrWhiteSpace(sharedHome) || string.IsNullOrWhiteSpace(privateHome)) { sharedHome = privateHome = null; return; }
        sharedHome = Path.GetFullPath(sharedHome).TrimEnd('\\'); privateHome = Path.GetFullPath(privateHome).TrimEnd('\\');
        foreach (var name in new[] { "CreateFileW", "GetFileAttributesW", "GetFileAttributesExW", "SetFileAttributesW", "DeleteFileW", "MoveFileExW", "ReplaceFileW", "FindFirstFileExW", "NtCreateFile", "NtOpenFile" })
            originals[name] = GetProcAddress(GetModuleHandleW(name.StartsWith("Nt") ? "ntdll.dll" : "kernel32.dll"), name);
    }

    internal static string Map(string path, string shared, string separate)
    {
        // NT object names are not filesystem-relative paths. In particular,
        // Rust opens child stdio through \??\pipe\...; normalizing that name
        // would turn it into <cwd>\pipe\... and make every child spawn fail.
        // Only resolve candidates for the two files we actually own. All other
        // names (including device, pipe and extended UNC names) stay byte-for-byte.
        var source = path;
        if (source.StartsWith("\\\\.\\", StringComparison.Ordinal)) return path;
        if (source.StartsWith("\\\\?\\", StringComparison.Ordinal) || source.StartsWith("\\??\\", StringComparison.Ordinal))
        {
            source = source[4..];
            if (source.StartsWith("UNC\\", StringComparison.OrdinalIgnoreCase)) source = "\\\\" + source[4..];
            else if (source.Length < 3 || !char.IsAsciiLetter(source[0]) || source[1] != ':' || source[2] != '\\') return path;
        }
        var name = Path.GetFileName(source);
        if (!name.Equals("auth.json", StringComparison.OrdinalIgnoreCase) && !name.Equals("config.toml", StringComparison.OrdinalIgnoreCase)) return path;
        var full = Path.GetFullPath(source);
        return string.Equals(Path.GetDirectoryName(full), shared.TrimEnd('\\'), StringComparison.OrdinalIgnoreCase)
            ? Path.Combine(separate, name) : path;
    }

    private sealed class MappedPath : IDisposable
    {
        internal char* Value;
        private nint allocation;
        internal MappedPath(char* path)
        {
            Value = path;
            if (path == null || sharedHome is null) return;
            var source = new string(path);
            if (source.StartsWith("\\\\.\\", StringComparison.Ordinal)) return;
            var target = Map(source, sharedHome, privateHome!);
            if (source.Equals(target, StringComparison.Ordinal)) return;
            allocation = Marshal.StringToHGlobalUni(target); Value = (char*)allocation;
        }
        public void Dispose() { if (allocation != 0) Marshal.FreeHGlobal(allocation); }
    }

    internal static nint Replacement(string? name)
    {
        if (sharedHome is null || name is null || !originals.ContainsKey(name)) return 0;
        return name switch
        {
            "CreateFileW" => (nint)(delegate* unmanaged[Stdcall]<char*, uint, uint, nint, uint, uint, nint, nint>)&CreateFile,
            "GetFileAttributesW" => (nint)(delegate* unmanaged[Stdcall]<char*, uint>)&Attributes,
            "GetFileAttributesExW" => (nint)(delegate* unmanaged[Stdcall]<char*, int, nint, int>)&AttributesEx,
            "SetFileAttributesW" => (nint)(delegate* unmanaged[Stdcall]<char*, uint, int>)&SetAttributes,
            "DeleteFileW" => (nint)(delegate* unmanaged[Stdcall]<char*, int>)&Delete,
            "MoveFileExW" => (nint)(delegate* unmanaged[Stdcall]<char*, char*, uint, int>)&Move,
            "ReplaceFileW" => (nint)(delegate* unmanaged[Stdcall]<char*, char*, char*, uint, nint, nint, int>)&Replace,
            "FindFirstFileExW" => (nint)(delegate* unmanaged[Stdcall]<char*, int, nint, int, nint, uint, nint>)&Find,
            "NtCreateFile" => (nint)(delegate* unmanaged[Stdcall]<nint, uint, ObjectAttributes*, nint, nint, uint, uint, uint, uint, nint, uint, int>)&NtCreate,
            "NtOpenFile" => (nint)(delegate* unmanaged[Stdcall]<nint, uint, ObjectAttributes*, nint, uint, uint, int>)&NtOpen,
            _ => 0
        };
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvStdcall)])]
    private static nint CreateFile(char* path, uint access, uint share, nint security, uint disposition, uint flags, nint template)
    {
        try { using var mapped = new MappedPath(path); return ((delegate* unmanaged[Stdcall]<char*, uint, uint, nint, uint, uint, nint, nint>)originals["CreateFileW"])(mapped.Value, access, share, security, disposition, flags, template); }
        catch { SetLastError(5); return -1; }
    }
    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvStdcall)])]
    private static uint Attributes(char* path)
    {
        try { using var mapped = new MappedPath(path); return ((delegate* unmanaged[Stdcall]<char*, uint>)originals["GetFileAttributesW"])(mapped.Value); }
        catch { SetLastError(5); return uint.MaxValue; }
    }
    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvStdcall)])]
    private static int AttributesEx(char* path, int level, nint data)
    {
        try { using var mapped = new MappedPath(path); return ((delegate* unmanaged[Stdcall]<char*, int, nint, int>)originals["GetFileAttributesExW"])(mapped.Value, level, data); }
        catch { SetLastError(5); return 0; }
    }
    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvStdcall)])]
    private static int SetAttributes(char* path, uint attributes)
    {
        try { using var mapped = new MappedPath(path); return ((delegate* unmanaged[Stdcall]<char*, uint, int>)originals["SetFileAttributesW"])(mapped.Value, attributes); }
        catch { SetLastError(5); return 0; }
    }
    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvStdcall)])]
    private static int Delete(char* path)
    {
        try { using var mapped = new MappedPath(path); return ((delegate* unmanaged[Stdcall]<char*, int>)originals["DeleteFileW"])(mapped.Value); }
        catch { SetLastError(5); return 0; }
    }
    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvStdcall)])]
    private static int Move(char* source, char* target, uint flags)
    {
        try { using var from = new MappedPath(source); using var to = new MappedPath(target); return ((delegate* unmanaged[Stdcall]<char*, char*, uint, int>)originals["MoveFileExW"])(from.Value, to.Value, flags); }
        catch { SetLastError(5); return 0; }
    }
    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvStdcall)])]
    private static int Replace(char* target, char* source, char* backup, uint flags, nint exclude, nint reserved)
    {
        try { using var to = new MappedPath(target); using var from = new MappedPath(source); using var save = new MappedPath(backup); return ((delegate* unmanaged[Stdcall]<char*, char*, char*, uint, nint, nint, int>)originals["ReplaceFileW"])(to.Value, from.Value, save.Value, flags, exclude, reserved); }
        catch { SetLastError(5); return 0; }
    }
    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvStdcall)])]
    private static nint Find(char* path, int level, nint data, int operation, nint filter, uint flags)
    {
        try { using var mapped = new MappedPath(path); return ((delegate* unmanaged[Stdcall]<char*, int, nint, int, nint, uint, nint>)originals["FindFirstFileExW"])(mapped.Value, level, data, operation, filter, flags); }
        catch { SetLastError(5); return -1; }
    }

    [StructLayout(LayoutKind.Sequential)] private struct UnicodeString { public ushort Length, MaximumLength; public char* Buffer; }
    [StructLayout(LayoutKind.Sequential)] private struct ObjectAttributes { public uint Length; public nint Root; public UnicodeString* Name; public uint Attributes; public nint Security, Quality; }

    private static string? MapObject(ObjectAttributes* attributes)
    {
        if (attributes == null || attributes->Name == null || attributes->Name->Buffer == null) return null;
        var source = new string(attributes->Name->Buffer, 0, attributes->Name->Length / 2);
        if (attributes->Root != 0)
        {
            var buffer = new char[32768];
            fixed (char* path = buffer)
            {
                var length = GetFinalPathNameByHandleW(attributes->Root, path, (uint)buffer.Length, 0);
                if (length == 0 || length >= buffer.Length) return null;
                source = Path.Combine(new string(path, 0, (int)length), source);
            }
        }
        if (!source.StartsWith("\\??\\", StringComparison.Ordinal) && !source.StartsWith("\\\\?\\", StringComparison.Ordinal)) return null;
        var target = Map(source, sharedHome!, privateHome!);
        if (source.Equals(target, StringComparison.Ordinal)) return null;
        return target.StartsWith("\\\\", StringComparison.Ordinal) ? "\\??\\UNC\\" + target[2..] : "\\??\\" + target;
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvStdcall)])]
    private static int NtCreate(nint handle, uint access, ObjectAttributes* attributes, nint status, nint allocation, uint fileAttributes, uint share, uint disposition, uint options, nint extended, uint length)
    {
        var original = (delegate* unmanaged[Stdcall]<nint, uint, ObjectAttributes*, nint, nint, uint, uint, uint, uint, nint, uint, int>)originals["NtCreateFile"];
        try
        {
            var target = MapObject(attributes);
            if (target is null) return original(handle, access, attributes, status, allocation, fileAttributes, share, disposition, options, extended, length);
            fixed (char* path = target)
            {
                var name = new UnicodeString { Length = checked((ushort)(target.Length * 2)), MaximumLength = checked((ushort)((target.Length + 1) * 2)), Buffer = path };
                var mapped = *attributes; mapped.Root = 0; mapped.Name = &name;
                return original(handle, access, &mapped, status, allocation, fileAttributes, share, disposition, options, extended, length);
            }
        }
        catch { return unchecked((int)0xc0000022); }
    }
    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvStdcall)])]
    private static int NtOpen(nint handle, uint access, ObjectAttributes* attributes, nint status, uint share, uint options)
    {
        var original = (delegate* unmanaged[Stdcall]<nint, uint, ObjectAttributes*, nint, uint, uint, int>)originals["NtOpenFile"];
        try
        {
            var target = MapObject(attributes);
            if (target is null) return original(handle, access, attributes, status, share, options);
            fixed (char* path = target)
            {
                var name = new UnicodeString { Length = checked((ushort)(target.Length * 2)), MaximumLength = checked((ushort)((target.Length + 1) * 2)), Buffer = path };
                var mapped = *attributes; mapped.Root = 0; mapped.Name = &name;
                return original(handle, access, &mapped, status, share, options);
            }
        }
        catch { return unchecked((int)0xc0000022); }
    }

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode)] private static extern nint GetModuleHandleW(string module);
    [DllImport("kernel32.dll", CharSet = CharSet.Ansi, ExactSpelling = true)] private static extern nint GetProcAddress(nint module, string name);
    [DllImport("kernel32.dll")] private static extern void SetLastError(uint error);
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode)] private static extern uint GetFinalPathNameByHandleW(nint handle, char* path, uint length, uint flags);
}
