using System.Collections.Concurrent;
using System.ComponentModel;
using System.Reflection.PortableExecutable;
using System.Runtime.InteropServices;

namespace DesktopProxy.NativeHook;

internal static class HttpCompatibility
{
    // These two instruction sequences were checked against the installed native
    // reader and the pinned parse_backend_url/apply_workspace_routing sources.
    // Only the scheme predicate changes. Host, credentials, path, routing owner,
    // requirements and provider/session checks continue in the original code.
    private static readonly byte[][] Signatures = [
        Convert.FromHexString("4183F9057514B868747470413307410FB64F0483F17309C1743D66C746080115"),
        Convert.FromHexString("4183F9057518B868747470413307410FB64F0483F17309C10F840001000041B82000000031D2")
    ];
    private static readonly ConcurrentDictionary<string, Lazy<Patch[]>> Plans = new(StringComparer.OrdinalIgnoreCase);
    internal sealed record Patch(int Rva, byte[] Original, int AcceptedRva, int RejectedRva);

    internal static Patch[] Inspect(string executable)
    {
        var file = new FileInfo(executable);
        var key = file.FullName + "|" + file.Length + "|" + file.LastWriteTimeUtc.Ticks;
        return Plans.GetOrAdd(key, _ => new Lazy<Patch[]>(() => ReadPlan(executable))).Value;
    }

    private static Patch[] ReadPlan(string executable)
    {
        using var file = File.OpenRead(executable); using var pe = new PEReader(file);
        if (pe.PEHeaders.CoffHeader.Machine != Machine.Amd64) throw new InvalidOperationException("当前客户端架构尚不支持此服务地址。");
        var section = pe.PEHeaders.SectionHeaders.Single(s => s.Name == ".text");
        var code = pe.GetSectionData(section.VirtualAddress).GetContent().AsSpan();
        var patches = new List<Patch>();
        foreach (var signature in Signatures)
        {
            var offset = code.IndexOf(signature);
            if (offset < 0 || code[(offset + signature.Length)..].IndexOf(signature) >= 0)
                throw new InvalidOperationException("客户端地址处理入口已变化，请更新启动器。");
            var longBranch = signature[24] == 0x0f;
            var length = longBranch ? 30 : 26;
            var rva = section.VirtualAddress + offset;
            var accepted = rva + length + (longBranch ? BitConverter.ToInt32(signature, 26) : (sbyte)signature[25]);
            var rejected = rva + 6 + (sbyte)signature[5];
            if (rejected != rva + length || accepted <= rejected || accepted >= section.VirtualAddress + section.VirtualSize)
                throw new InvalidOperationException("客户端地址处理结构无法识别。");
            patches.Add(new(rva, signature[..length], accepted, rejected));
        }
        return patches.ToArray();
    }

    internal static void Apply(nint process, string executable)
    {
        var patches = Inspect(executable);
        if (NtQueryInformationProcess(process, 0, out var info, Marshal.SizeOf<ProcessBasicInformation>(), out _) != 0)
            throw new InvalidOperationException("无法读取客户端进程信息。");
        var imageBytes = Read(process, info.Peb + 0x10, 8);
        var image = (nint)BitConverter.ToInt64(imageBytes);
        foreach (var patch in patches)
        {
            var address = image + patch.Rva;
            if (!Read(process, address, patch.Original.Length).AsSpan().SequenceEqual(patch.Original))
                throw new InvalidOperationException("客户端启动内容与安装文件不一致。");
            var replacement = Predicate(image + patch.AcceptedRva, image + patch.RejectedRva);
            var allocation = VirtualAllocEx(process, 0, (nuint)replacement.Length, 0x3000, 4);
            if (allocation == 0) throw new Win32Exception();
            Write(process, allocation, replacement);
            if (!VirtualProtectEx(process, allocation, (nuint)replacement.Length, 0x20, out _)) throw new Win32Exception();
            var jump = AbsoluteJump(allocation);
            var site = Enumerable.Repeat((byte)0x90, patch.Original.Length).ToArray(); jump.CopyTo(site, 0);
            if (!VirtualProtectEx(process, address, (nuint)site.Length, 0x40, out var protection)) throw new Win32Exception();
            try { Write(process, address, site); }
            finally { VirtualProtectEx(process, address, (nuint)site.Length, protection, out _); }
            if (!FlushInstructionCache(process, 0, 0)) throw new Win32Exception();
        }
    }

    internal static bool Verify(nint process, string executable)
    {
        if (NtQueryInformationProcess(process, 0, out var info, Marshal.SizeOf<ProcessBasicInformation>(), out _) != 0) return false;
        var image = (nint)BitConverter.ToInt64(Read(process, info.Peb + 0x10, 8));
        foreach (var patch in Inspect(executable))
        {
            var site = Read(process, image + patch.Rva, 14);
            if (!site.AsSpan(0, 6).SequenceEqual(Convert.FromHexString("FF2500000000"))) return false;
            var target = (nint)BitConverter.ToInt64(site, 6);
            var expected = Predicate(image + patch.AcceptedRva, image + patch.RejectedRva);
            if (!Read(process, target, expected.Length).AsSpan().SequenceEqual(expected)) return false;
        }
        return true;
    }

    internal static byte[] Predicate(nint accepted, nint rejected)
    {
        // r9 = parsed scheme length; r15 = parsed URL bytes. Reject every
        // scheme except literal http/https, then return to the original checks.
        var code = new List<byte>(); var rejects = new List<int>(); var accepts = new List<int>();
        void Emit(string hex) => code.AddRange(Convert.FromHexString(hex));
        void Branch(List<int> targets, byte opcode) { code.Add(opcode); targets.Add(code.Count); code.Add(0); }
        Emit("4183F904"); code.Add(0x74); var prefix = code.Count; code.Add(0);
        Emit("4183F905"); Branch(rejects, 0x75);
        code[prefix] = checked((byte)(code.Count - prefix - 1));
        Emit("41813F68747470"); Branch(rejects, 0x75);
        Emit("4183F904"); Branch(accepts, 0x74);
        Emit("41807F0473"); Branch(rejects, 0x75);
        var yes = code.Count; Emit("31C031C9"); code.AddRange(AbsoluteJump(accepted));
        var no = code.Count; code.AddRange(AbsoluteJump(rejected));
        foreach (var branch in accepts) code[branch] = checked((byte)(yes - branch - 1));
        foreach (var branch in rejects) code[branch] = checked((byte)(no - branch - 1));
        return code.ToArray();
    }

    private static byte[] AbsoluteJump(nint address) => Convert.FromHexString("FF2500000000").Concat(BitConverter.GetBytes((long)address)).ToArray();
    private static byte[] Read(nint process, nint address, int count)
    {
        var data = new byte[count];
        if (!ReadProcessMemory(process, address, data, (nuint)count, out var read) || read != (nuint)count) throw new Win32Exception();
        return data;
    }
    private static void Write(nint process, nint address, byte[] bytes)
    {
        if (!WriteProcessMemory(process, address, bytes, (nuint)bytes.Length, out var written) || written != (nuint)bytes.Length) throw new Win32Exception();
    }

    [StructLayout(LayoutKind.Sequential)] private struct ProcessBasicInformation { public nint Reserved, Peb, Reserved2, Reserved3, Pid, ParentPid; }
    [DllImport("ntdll.dll")] private static extern int NtQueryInformationProcess(nint process, int kind, out ProcessBasicInformation info, int size, out int returned);
    [DllImport("kernel32.dll", SetLastError = true)] private static extern nint VirtualAllocEx(nint process, nint address, nuint size, uint allocation, uint protection);
    [DllImport("kernel32.dll", SetLastError = true)] private static extern bool VirtualProtectEx(nint process, nint address, nuint size, uint protection, out uint previous);
    [DllImport("kernel32.dll", SetLastError = true)] private static extern bool ReadProcessMemory(nint process, nint address, byte[] bytes, nuint size, out nuint read);
    [DllImport("kernel32.dll", SetLastError = true)] private static extern bool WriteProcessMemory(nint process, nint address, byte[] bytes, nuint size, out nuint written);
    [DllImport("kernel32.dll", SetLastError = true)] private static extern bool FlushInstructionCache(nint process, nint address, nuint size);
}
