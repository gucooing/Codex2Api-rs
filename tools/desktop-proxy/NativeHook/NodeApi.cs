using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using System.Text;

namespace DesktopProxy.NativeHook;

internal sealed unsafe class Js(nint env)
{
    public nint Env { get; } = env;
    public nint Global { get { Check(Api.napi_get_global(Env, out var value)); return value; } }
    public nint Undefined { get { Check(Api.napi_get_undefined(Env, out var value)); return value; } }
    public int Type(nint value) { if (value == 0) return 0; Check(Api.napi_typeof(Env, value, out var type)); return type; }
    public bool IsObject(nint value) => Type(value) is 6 or 7;
    public nint Get(nint value, string key) { if (!IsObject(value)) return Undefined; Check(Api.napi_get_named_property(Env, value, key, out var result)); return result; }
    public void Set(nint value, string key, nint property) => Check(Api.napi_set_named_property(Env, value, key, property));
    public void Delete(nint value, string key) => Check(Api.napi_delete_property(Env, value, Text(key), out _));
    public bool Has(nint value, string key) { if (!IsObject(value)) return false; Check(Api.napi_has_named_property(Env, value, key, out var found)); return found; }
    public nint Text(string value) { var bytes = Encoding.UTF8.GetBytes(value); Check(Api.napi_create_string_utf8(Env, bytes, (nuint)bytes.Length, out var result)); return result; }
    public string String(nint value)
    {
        Check(Api.napi_coerce_to_string(Env, value, out var text));
        Check(Api.napi_get_value_string_utf8(Env, text, null, 0, out var length));
        var bytes = new byte[(int)length + 1];
        Check(Api.napi_get_value_string_utf8(Env, text, bytes, (nuint)bytes.Length, out length));
        return Encoding.UTF8.GetString(bytes, 0, (int)length);
    }
    public string? OptionalString(nint value) => Type(value) is 0 or 1 ? null : String(value);
    public bool Boolean(nint value) { Check(Api.napi_coerce_to_bool(Env, value, out var converted)); Check(Api.napi_get_value_bool(Env, converted, out var result)); return result; }
    public nint Bool(bool value) { Check(Api.napi_get_boolean(Env, value, out var result)); return result; }
    public int Integer(nint value) { Check(Api.napi_get_value_int32(Env, value, out var result)); return result; }
    public nint Object() { Check(Api.napi_create_object(Env, out var result)); return result; }
    public nint Clone(nint value) => Call(Get(Global, "Object"), "assign", Object(), value);
    public nint Array(IEnumerable<nint> values)
    {
        var items = values.ToArray(); Check(Api.napi_create_array_with_length(Env, (nuint)items.Length, out var result));
        for (uint i = 0; i < items.Length; i++) Check(Api.napi_set_element(Env, result, i, items[i]));
        return result;
    }
    public bool IsArray(nint value) { Check(Api.napi_is_array(Env, value, out var result)); return result; }
    public nint[] Items(nint value)
    {
        Check(Api.napi_get_array_length(Env, value, out var length)); var items = new nint[length];
        for (uint i = 0; i < length; i++) Check(Api.napi_get_element(Env, value, i, out items[i]));
        return items;
    }
    public string[] Keys(nint value) { Check(Api.napi_get_property_names(Env, value, out var keys)); return Items(keys).Select(String).ToArray(); }
    public nint Call(nint receiver, string name, params nint[] arguments) => Invoke(Get(receiver, name), receiver, arguments);
    public nint Invoke(nint function, nint receiver, params nint[] arguments)
    {
        Check(Api.napi_call_function(Env, receiver, function, (nuint)arguments.Length, arguments, out var result)); return result;
    }
    public nint New(nint constructor, params nint[] arguments)
    {
        Check(Api.napi_new_instance(Env, constructor, (nuint)arguments.Length, arguments, out var result)); return result;
    }
    public nint Require(string module)
    {
        var process = Get(Global, "process");
        var modules = Call(process, "getBuiltinModule", Text("module"));
        var require = Call(modules, "createRequire", Text(NativeEntry.Archive + "/package.json"));
        return Invoke(require, Undefined, Text(module));
    }
    public nint TryRequire(string module)
    {
        try { return Require(module); }
        catch { Api.napi_get_and_clear_last_exception(Env, out _); return Undefined; }
    }
    public Ref Hold(nint value) => new(this, value);
    public nint Function(Func<nint, nint[], nint> callback, params IDisposable[] owned)
    {
        var box = new Callback(this, callback, owned); var handle = GCHandle.Alloc(box); var data = GCHandle.ToIntPtr(handle);
        Check(Api.napi_create_function(Env, "", 0, (nint)(delegate* unmanaged[Cdecl]<nint, nint, nint>)&Dispatch, data, out var value));
        Check(Api.napi_add_finalizer(Env, value, data, (nint)(delegate* unmanaged[Cdecl]<nint, nint, nint, void>)&Finalize, 0, out _));
        return value;
    }
    public nint PreserveConstructor(nint wrapper, nint original)
    {
        Set(wrapper, "prototype", Get(original, "prototype")); Call(Get(Global, "Object"), "setPrototypeOf", wrapper, original); return wrapper;
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static nint Dispatch(nint env, nint info)
    {
        try
        {
            nuint count = 16; var args = new nint[16];
            Check(Api.napi_get_cb_info(env, info, ref count, args, out var receiver, out var data));
            var callback = (Callback)GCHandle.FromIntPtr(data).Target!;
            return callback.Body(receiver, args[..(int)count]);
        }
        catch (Exception error)
        {
            Api.napi_is_exception_pending(env, out var pending);
            if (!pending) Api.napi_throw_error(env, null, error.Message);
            return 0;
        }
    }
    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static void Finalize(nint env, nint data, nint hint)
    {
        var handle = GCHandle.FromIntPtr(data);
        if (handle.Target is Callback callback) foreach (var item in callback.Owned) item.Dispose();
        handle.Free();
    }
    private sealed record Callback(Js Js, Func<nint, nint[], nint> Body, IDisposable[] Owned);
    public sealed class Ref : IDisposable
    {
        private readonly Js js; private nint reference;
        internal Ref(Js js, nint value) { this.js = js; Check(Api.napi_create_reference(js.Env, value, 1, out reference)); }
        public nint Value { get { Check(Api.napi_get_reference_value(js.Env, reference, out var value)); return value; } }
        public void Dispose() { if (reference != 0) { Api.napi_delete_reference(js.Env, reference); reference = 0; } }
    }
    private static void Check(int status) { if (status != 0) throw new InvalidOperationException("Client runtime API failed (" + status + ")."); }
}

internal static class Api
{
    private const string Library = "chrome.dll";
    [DllImport(Library)] internal static extern int napi_get_global(nint env, out nint value);
    [DllImport(Library)] internal static extern int napi_get_undefined(nint env, out nint value);
    [DllImport(Library)] internal static extern int napi_typeof(nint env, nint value, out int type);
    [DllImport(Library)] internal static extern int napi_get_named_property(nint env, nint value, [MarshalAs(UnmanagedType.LPUTF8Str)] string key, out nint result);
    [DllImport(Library)] internal static extern int napi_set_named_property(nint env, nint value, [MarshalAs(UnmanagedType.LPUTF8Str)] string key, nint property);
    [DllImport(Library)] internal static extern int napi_has_named_property(nint env, nint value, [MarshalAs(UnmanagedType.LPUTF8Str)] string key, [MarshalAs(UnmanagedType.I1)] out bool found);
    [DllImport(Library)] internal static extern int napi_delete_property(nint env, nint value, nint key, [MarshalAs(UnmanagedType.I1)] out bool deleted);
    [DllImport(Library)] internal static extern int napi_get_property_names(nint env, nint value, out nint names);
    [DllImport(Library)] internal static extern int napi_create_string_utf8(nint env, byte[] value, nuint length, out nint result);
    [DllImport(Library)] internal static extern int napi_coerce_to_string(nint env, nint value, out nint result);
    [DllImport(Library)] internal static extern int napi_get_value_string_utf8(nint env, nint value, byte[]? buffer, nuint size, out nuint written);
    [DllImport(Library)] internal static extern int napi_coerce_to_bool(nint env, nint value, out nint result);
    [DllImport(Library)] internal static extern int napi_get_value_bool(nint env, nint value, [MarshalAs(UnmanagedType.I1)] out bool result);
    [DllImport(Library)] internal static extern int napi_get_boolean(nint env, [MarshalAs(UnmanagedType.I1)] bool value, out nint result);
    [DllImport(Library)] internal static extern int napi_get_value_int32(nint env, nint value, out int result);
    [DllImport(Library)] internal static extern int napi_create_object(nint env, out nint value);
    [DllImport(Library)] internal static extern int napi_create_array_with_length(nint env, nuint length, out nint value);
    [DllImport(Library)] internal static extern int napi_is_array(nint env, nint value, [MarshalAs(UnmanagedType.I1)] out bool result);
    [DllImport(Library)] internal static extern int napi_get_array_length(nint env, nint value, out uint length);
    [DllImport(Library)] internal static extern int napi_get_element(nint env, nint value, uint index, out nint result);
    [DllImport(Library)] internal static extern int napi_set_element(nint env, nint value, uint index, nint item);
    [DllImport(Library)] internal static extern int napi_call_function(nint env, nint receiver, nint function, nuint count, nint[] args, out nint result);
    [DllImport(Library)] internal static extern int napi_new_instance(nint env, nint constructor, nuint count, nint[] args, out nint result);
    [DllImport(Library)] internal static extern int napi_create_function(nint env, [MarshalAs(UnmanagedType.LPUTF8Str)] string name, nuint length, nint callback, nint data, out nint value);
    [DllImport(Library)] internal static extern int napi_get_cb_info(nint env, nint info, ref nuint count, [Out] nint[] arguments, out nint receiver, out nint data);
    [DllImport(Library)] internal static extern int napi_add_finalizer(nint env, nint value, nint data, nint finalize, nint hint, out nint reference);
    [DllImport(Library)] internal static extern int napi_create_reference(nint env, nint value, uint count, out nint reference);
    [DllImport(Library)] internal static extern int napi_get_reference_value(nint env, nint reference, out nint value);
    [DllImport(Library)] internal static extern int napi_delete_reference(nint env, nint reference);
    [DllImport(Library)] internal static extern int napi_is_exception_pending(nint env, [MarshalAs(UnmanagedType.I1)] out bool pending);
    [DllImport(Library)] internal static extern int napi_get_and_clear_last_exception(nint env, out nint error);
    [DllImport(Library)] internal static extern int napi_throw_error(nint env, [MarshalAs(UnmanagedType.LPUTF8Str)] string? code, [MarshalAs(UnmanagedType.LPUTF8Str)] string message);
}
