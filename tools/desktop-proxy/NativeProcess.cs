using System.ComponentModel;
using System.Diagnostics;
using System.Reflection.PortableExecutable;
using System.Runtime.InteropServices;
using System.Text;

namespace DesktopProxy;

internal static class NativeProcess
{
    public static int Start(ProcessStartInfo start, string hookFile, string statusFile, bool probeOnly = false, Func<bool>? cancelled = null)
    {
        var environment = string.Join('\0', start.Environment.Where(pair => pair.Value is not null)
            .OrderBy(pair => pair.Key, StringComparer.OrdinalIgnoreCase).Select(pair => pair.Key + "=" + pair.Value)) + "\0\0";
        var block = Marshal.StringToHGlobalUni(environment);
        var si = new StartupInfo { cb = Marshal.SizeOf<StartupInfo>() };
        ProcessInfo info = default;
        var success = false;
        try
        {
            if (cancelled?.Invoke() == true) throw new OperationCanceledException();
            var command = new StringBuilder(PackagedApplication.QuoteArgument(start.FileName) + " " + string.Join(" ", start.ArgumentList.Select(PackagedApplication.QuoteArgument)));
            if (!CreateProcessW(start.FileName, command, 0, 0, false, 0x404, block, start.WorkingDirectory, ref si, out info)) throw new Win32Exception();
            var libraryPath = RemoteString(info.process, hookFile);
            nint statusPath = 0;
            try
            {
                var loader = GetProcAddress(GetModuleHandleW("kernel32.dll"), "LoadLibraryW");
                RunRemote(info.process, loader, libraryPath);
                using var process = Process.GetProcessById(info.pid);
                var module = process.Modules.Cast<ProcessModule>().SingleOrDefault(m => Path.GetFullPath(m.FileName).Equals(Path.GetFullPath(hookFile), StringComparison.OrdinalIgnoreCase))
                    ?? throw new InvalidOperationException("客户端启动组件加载失败。");
                statusPath = RemoteString(info.process, statusFile);
                var entry = module.BaseAddress + ExportRva(hookFile, "InitializeHook");
                if (RunRemote(info.process, entry, statusPath) != 0) throw new InvalidOperationException(File.Exists(statusFile) ? File.ReadAllText(statusFile) : "客户端初始化失败。");
                if (cancelled?.Invoke() == true) throw new OperationCanceledException();
                if (probeOnly) TerminateProcess(info.process, 0);
                else if (ResumeThread(info.thread) == uint.MaxValue) throw new Win32Exception();
                success = true;
                return info.pid;
            }
            finally
            {
                if (!success) TerminateProcess(info.process, 1);
                if (statusPath != 0) VirtualFreeEx(info.process, statusPath, 0, 0x8000);
                VirtualFreeEx(info.process, libraryPath, 0, 0x8000);
            }
        }
        finally
        {
            Marshal.FreeHGlobal(block);
            if (info.process != 0)
            {
                if (!success) TerminateProcess(info.process, 1);
                CloseHandle(info.thread); CloseHandle(info.process);
            }
        }
    }

    private static nint RemoteString(nint process, string value)
    {
        var data = Encoding.Unicode.GetBytes(value + '\0');
        var address = VirtualAllocEx(process, 0, (nuint)data.Length, 0x3000, 4);
        if (address == 0) throw new Win32Exception();
        if (!WriteProcessMemory(process, address, data, (nuint)data.Length, out var written) || written != (nuint)data.Length)
        {
            VirtualFreeEx(process, address, 0, 0x8000); throw new Win32Exception();
        }
        return address;
    }

    private static uint RunRemote(nint process, nint entry, nint argument)
    {
        var thread = CreateRemoteThread(process, 0, 0, entry, argument, 0, out _);
        if (thread == 0) throw new Win32Exception();
        try
        {
            if (WaitForSingleObject(thread, 15000) != 0) throw new TimeoutException("客户端启动组件加载超时。");
            if (!GetExitCodeThread(thread, out var code)) throw new Win32Exception();
            return code;
        }
        finally { CloseHandle(thread); }
    }

    internal static int ExportRva(string library, string name)
    {
        using var file = File.OpenRead(library);
        using var pe = new PEReader(file);
        var export = pe.GetSectionData(pe.PEHeaders.PEHeader!.ExportTableDirectory.RelativeVirtualAddress).GetReader();
        export.Offset = 20;
        export.ReadUInt32(); var count = export.ReadInt32();
        var functions = export.ReadInt32(); var names = export.ReadInt32(); var ordinals = export.ReadInt32();
        for (var index = 0; index < count; index++)
        {
            var pointer = pe.GetSectionData(names + index * 4).GetReader().ReadInt32();
            var reader = pe.GetSectionData(pointer).GetReader();
            var text = new StringBuilder();
            for (byte c; (c = reader.ReadByte()) != 0;) text.Append((char)c);
            if (text.ToString() != name) continue;
            var ordinal = pe.GetSectionData(ordinals + index * 2).GetReader().ReadUInt16();
            return pe.GetSectionData(functions + ordinal * 4).GetReader().ReadInt32();
        }
        throw new InvalidOperationException("启动组件不完整。");
    }

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)] private struct StartupInfo { public int cb; public nint reserved, desktop, title; public int x, y, w, h, xc, yc, fill, flags; public short show, cb2; public nint reserved2, stdin, stdout, stderr; }
    [StructLayout(LayoutKind.Sequential)] private struct ProcessInfo { public nint process, thread; public int pid, tid; }
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)] private static extern bool CreateProcessW(string application, StringBuilder commandLine, nint pa, nint ta, bool inherit, uint flags, nint environment, string cwd, ref StartupInfo si, out ProcessInfo pi);
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode)] private static extern nint GetModuleHandleW(string module);
    [DllImport("kernel32.dll", CharSet = CharSet.Ansi, ExactSpelling = true)] private static extern nint GetProcAddress(nint module, string name);
    [DllImport("kernel32.dll", SetLastError = true)] private static extern nint VirtualAllocEx(nint process, nint address, nuint size, uint allocation, uint protection);
    [DllImport("kernel32.dll", SetLastError = true)] private static extern bool VirtualFreeEx(nint process, nint address, nuint size, uint type);
    [DllImport("kernel32.dll", SetLastError = true)] private static extern bool WriteProcessMemory(nint process, nint address, byte[] data, nuint size, out nuint written);
    [DllImport("kernel32.dll", SetLastError = true)] private static extern nint CreateRemoteThread(nint process, nint attributes, nuint stackSize, nint entry, nint parameter, uint flags, out uint threadId);
    [DllImport("kernel32.dll")] private static extern uint WaitForSingleObject(nint handle, uint timeout);
    [DllImport("kernel32.dll", SetLastError = true)] private static extern bool GetExitCodeThread(nint thread, out uint code);
    [DllImport("kernel32.dll", SetLastError = true)] private static extern uint ResumeThread(nint thread);
    [DllImport("kernel32.dll")] private static extern bool TerminateProcess(nint process, uint code);
    [DllImport("kernel32.dll")] private static extern bool CloseHandle(nint handle);
}
