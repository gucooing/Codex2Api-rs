using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;

namespace DesktopProxy.NativeHook;

internal static unsafe class IatHook
{
    private static nint loadLibrary, loadLibraryEx, getProcAddress, createProcess, createAsUser, createWithToken;
    [ThreadStatic] private static nint registration;

    internal static void Install()
    {
        var kernel = GetModuleHandleW("kernel32.dll");
        loadLibrary = GetProcAddress(kernel, "LoadLibraryW");
        loadLibraryEx = GetProcAddress(kernel, "LoadLibraryExW");
        getProcAddress = GetProcAddress(kernel, "GetProcAddress");
        createProcess = GetProcAddress(kernel, "CreateProcessW");
        var security = NativeLibrary.Load("advapi32.dll");
        createAsUser = GetProcAddress(security, "CreateProcessAsUserW");
        createWithToken = GetProcAddress(security, "CreateProcessWithTokenW");
        Patch(GetModuleHandleW(null));
        var chrome = GetModuleHandleW("chrome.dll");
        if (chrome != 0) Patch(chrome);
    }

    // Replace imports in this process only. Client files and runtime switches
    // remain unchanged; DLL loads continue through their original OS functions.
    private static void Patch(nint module)
    {
        if (module == 0 || *(ushort*)module != 0x5a4d) return;
        var pe = module + *(int*)(module + 0x3c);
        if (*(uint*)pe != 0x4550 || *(ushort*)(pe + 24) != 0x20b) throw new InvalidOperationException("Unsupported native hook architecture.");
        var importsRva = *(uint*)(pe + 24 + 120);
        if (importsRva == 0) return;
        for (var descriptor = module + (int)importsRva; *(uint*)(descriptor + 12) != 0; descriptor += 20)
        {
            var thunk = (nint*)(module + *(int*)(descriptor + 16));
            var namesRva = *(int*)descriptor;
            var names = namesRva == 0 ? null : (nint*)(module + namesRva);
            for (var index = 0; thunk[index] != 0; index++)
            {
                var name = names != null && ((ulong)names[index] & 0x8000000000000000) == 0 ? Marshal.PtrToStringUTF8(module + (int)names[index] + 2) : null;
                nint replacement = name == "LoadLibraryW" || thunk[index] == loadLibrary ? (nint)(delegate* unmanaged[Stdcall]<char*, nint>)&LoadLibrary
                    : name == "LoadLibraryExW" || thunk[index] == loadLibraryEx ? (nint)(delegate* unmanaged[Stdcall]<char*, nint, uint, nint>)&LoadLibraryEx
                    : name == "GetProcAddress" || thunk[index] == getProcAddress ? (nint)(delegate* unmanaged[Stdcall]<nint, byte*, nint>)&Resolve
                    : name == "CreateProcessW" || thunk[index] == createProcess ? (nint)(delegate* unmanaged[Stdcall]<char*, char*, nint, nint, int, uint, nint, char*, nint, nint, int>)&CreateProcess
                    : name == "CreateProcessAsUserW" || thunk[index] == createAsUser ? (nint)(delegate* unmanaged[Stdcall]<nint, char*, char*, nint, nint, int, uint, nint, char*, nint, nint, int>)&CreateAsUser
                    : name == "CreateProcessWithTokenW" || thunk[index] == createWithToken ? (nint)(delegate* unmanaged[Stdcall]<nint, uint, char*, char*, uint, nint, char*, nint, nint, int>)&CreateWithToken
                    : 0;
                if (replacement == 0) continue;
                var slot = (nint)(thunk + index);
                if (!VirtualProtect(slot, (nuint)sizeof(nint), 4, out var protection)) throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());
                thunk[index] = replacement;
                if (!VirtualProtect(slot, (nuint)sizeof(nint), protection, out _)) throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());
            }
        }
    }

    private static void AfterLoad(char* path, nint module)
    {
        if (module == 0 || path == null) return;
        var name = new string(path);
        if (name.EndsWith("chrome.dll", StringComparison.OrdinalIgnoreCase)) Patch(module);
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvStdcall)])]
    private static nint LoadLibrary(char* path)
    {
        var module = ((delegate* unmanaged[Stdcall]<char*, nint>)loadLibrary)(path);
        try { AfterLoad(path, module); } catch (Exception e) { Report(e); }
        return module;
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvStdcall)])]
    private static nint LoadLibraryEx(char* path, nint file, uint flags)
    {
        var module = ((delegate* unmanaged[Stdcall]<char*, nint, uint, nint>)loadLibraryEx)(path, file, flags);
        try { AfterLoad(path, module); } catch (Exception e) { Report(e); }
        return module;
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvStdcall)])]
    private static nint Resolve(nint module, byte* name)
    {
        var function = ((delegate* unmanaged[Stdcall]<nint, byte*, nint>)getProcAddress)(module, name);
        if (function != 0 && (nuint)name > 65535 && Marshal.PtrToStringUTF8((nint)name) == "CreateProcessW")
            return (nint)(delegate* unmanaged[Stdcall]<char*, char*, nint, nint, int, uint, nint, char*, nint, nint, int>)&CreateProcess;
        if (function != 0 && (nuint)name > 65535 && Marshal.PtrToStringUTF8((nint)name) == "CreateProcessAsUserW")
            return (nint)(delegate* unmanaged[Stdcall]<nint, char*, char*, nint, nint, int, uint, nint, char*, nint, nint, int>)&CreateAsUser;
        if (function != 0 && (nuint)name > 65535 && Marshal.PtrToStringUTF8((nint)name) == "CreateProcessWithTokenW")
            return (nint)(delegate* unmanaged[Stdcall]<nint, uint, char*, char*, uint, nint, char*, nint, nint, int>)&CreateWithToken;
        if (function != 0 && (nuint)name > 65535 && Marshal.PtrToStringUTF8((nint)name) == "napi_register_module_v1")
        {
            try { Patch(module); } catch (Exception e) { Report(e); }
            registration = function;
            return (nint)(delegate* unmanaged[Cdecl]<nint, nint, nint>)&Register;
        }
        return function;
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static nint Register(nint env, nint exports)
    {
        var original = registration;
        var result = ((delegate* unmanaged[Cdecl]<nint, nint, nint>)original)(env, exports);
        try { NativeEntry.NodeModuleLoaded(env); } catch (Exception e) { Report(e); }
        return result;
    }

    private static void Report(Exception error)
    {
        Interlocked.CompareExchange(ref NativeEntry.Failure, error.Message, null);
        try { if (NativeEntry.StatusPath.Length > 0) File.WriteAllText(NativeEntry.StatusPath, "ERROR: " + NativeEntry.Failure); } catch { }
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvStdcall)])]
    private static int CreateProcess(char* application, char* command, nint processAttributes, nint threadAttributes, int inherit,
        uint flags, nint environment, char* directory, nint startup, nint information)
    {
        var original = (delegate* unmanaged[Stdcall]<char*, char*, nint, nint, int, uint, nint, char*, nint, nint, int>)createProcess;
        return StartNative(application, command, flags, information, actualFlags => original(application, command, processAttributes, threadAttributes, inherit, actualFlags, environment, directory, startup, information));
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvStdcall)])]
    private static int CreateAsUser(nint token, char* application, char* command, nint processAttributes, nint threadAttributes, int inherit,
        uint flags, nint environment, char* directory, nint startup, nint information)
    {
        var original = (delegate* unmanaged[Stdcall]<nint, char*, char*, nint, nint, int, uint, nint, char*, nint, nint, int>)createAsUser;
        return StartNative(application, command, flags, information, actualFlags => original(token, application, command, processAttributes, threadAttributes, inherit, actualFlags, environment, directory, startup, information));
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvStdcall)])]
    private static int CreateWithToken(nint token, uint logonFlags, char* application, char* command, uint flags, nint environment, char* directory, nint startup, nint information)
    {
        var original = (delegate* unmanaged[Stdcall]<nint, uint, char*, char*, uint, nint, char*, nint, nint, int>)createWithToken;
        return StartNative(application, command, flags, information, actualFlags => original(token, logonFlags, application, command, actualFlags, environment, directory, startup, information));
    }

    private static int StartNative(char* application, char* command, uint flags, nint information, Func<uint, int> start)
    {
        string? executable = null;
        try
        {
            var server = NativeEntry.Server;
            if (server?.StartsWith("http://", StringComparison.OrdinalIgnoreCase) == true)
            {
                var candidate = application != null ? new string(application) : command == null ? "" : new string(command).TrimStart();
                if (application == null && candidate.StartsWith('"')) candidate = candidate[1..candidate.IndexOf('"', 1)];
                else if (application == null) candidate = candidate.Split(' ', 2)[0];
                if (candidate.StartsWith("\\\\?\\", StringComparison.Ordinal) || candidate.StartsWith("\\??\\", StringComparison.Ordinal)) candidate = candidate[4..];
                var arguments = command == null ? [] : new string(command).Split([' ', '"'], StringSplitOptions.RemoveEmptyEntries);
                // Desktop can select an automatically updated runtime from its
                // own cache. Follow the executable it actually creates, then
                // validate both native instruction sites in that exact file.
                if (candidate.Length > 0 && Path.GetFileName(candidate).Equals("codex.exe", StringComparison.OrdinalIgnoreCase)
                    && arguments.Any(value => value is "app-server" or "app-server-daemon")) executable = Path.GetFullPath(candidate);
            }
            if (executable is null) return start(flags);
            HttpCompatibility.Inspect(executable);
        }
        catch (Exception e) { Report(e); SetLastError(50); return 0; }
        var result = start(flags | 4);
        if (result == 0) return 0;
        var process = *(nint*)information; var thread = *(nint*)(information + IntPtr.Size);
        try
        {
            HttpCompatibility.Apply(process, executable);
            if ((flags & 4) == 0 && ResumeThread(thread) == uint.MaxValue) throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());
            return 1;
        }
        catch (Exception e)
        {
            Report(e); TerminateProcess(process, 1); CloseHandle(thread); CloseHandle(process);
            new Span<byte>((void*)information, 24).Clear(); SetLastError(50); return 0;
        }
    }

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode)] private static extern nint GetModuleHandleW(string? module);
    [DllImport("kernel32.dll", CharSet = CharSet.Ansi, ExactSpelling = true)] private static extern nint GetProcAddress(nint module, string name);
    [DllImport("kernel32.dll", SetLastError = true)] private static extern bool VirtualProtect(nint address, nuint size, uint protection, out uint oldProtection);
    [DllImport("kernel32.dll", SetLastError = true)] private static extern uint ResumeThread(nint thread);
    [DllImport("kernel32.dll")] private static extern bool TerminateProcess(nint process, uint exitCode);
    [DllImport("kernel32.dll")] private static extern bool CloseHandle(nint handle);
    [DllImport("kernel32.dll")] private static extern void SetLastError(uint error);
}
