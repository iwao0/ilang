// COM-style interface bindings — IUnknown / ID3D12Device etc.
//
// `@com interface I { ... }` declares a virtual contract whose
// dispatch goes through a raw COM vtable
// (`(*recv)[slot](recv, args...)`), not ilang's class registry.
// Slot numbers come from declaration order, concatenated with any
// parent interface, matching the C++ COM ABI verbatim. Receivers
// are 8-byte handles (no ARC header); lifetime is managed by
// `Release()`.
//
// `@com interface` lives inside `@extern(C) { ... }` so its method
// signatures can reference raw pointers and other C-only types
// under the same scope rule that `fn` / `struct` decls follow.
//
// Cast `*void`/`i64` ↔ interface to bring a COM pointer in or
// out of FFI scope, then dispatch with `device.Method(args)`.

use d3d12 {
    GUID,
    D3D12_COMMAND_QUEUE_DESC,
}

@extern(C) {
    /// Universal IUnknown vtable — every COM interface starts here.
    @com pub interface IUnknown {
        QueryInterface(riid: *const GUID, ppvObject: **void): i32
        AddRef(): u32
        Release(): u32
    }

    /// ID3D12Object — IUnknown + 4 debug/naming methods. Most
    /// D3D12 interfaces inherit from this.
    @com pub interface ID3D12Object : IUnknown {
        GetPrivateData(guid: *const GUID, pDataSize: *u32, pData: *void): i32
        SetPrivateData(guid: *const GUID, DataSize: u32, pData: *const void): i32
        SetPrivateDataInterface(guid: *const GUID, pData: i64): i32
        SetName(Name: *const u16): i32
    }

    /// ID3D12Device — main GPU device. Only the slots this repo
    /// currently calls are typed; the rest stay implicitly typed
    /// (the user can add them as needed without renumbering).
    @com pub interface ID3D12Device : ID3D12Object {
        GetNodeCount(): u32
        CreateCommandQueue(
            pDesc: *const D3D12_COMMAND_QUEUE_DESC,
            riid: *const GUID,
            ppCommandQueue: **void
        ): i32
    }
}
