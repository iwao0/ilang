// COM vtable layouts as `pub struct` with fn-typed fields.
//
// Companion to `d3d12_vtables.il` (offset enums) — eventually
// the enums can be retired in favour of these typed structs.
//
// Each struct lists the COM vtable in slot order; readers get
// `vtbl.Method(args)` dispatch through CallRawIndirect instead
// of a manual `readU64 + cast as fn(...)` chain.
//
// Method fields whose signatures aren't yet needed are typed as
// `i64` so the slot is occupied (offset stays correct) without
// re-stating every D3D12 parameter list. Add a real `fn(...)`
// signature when a call site adopts the method.

use d3d12 {
    GUID,
    D3D12_COMMAND_QUEUE_DESC,
}

@extern(C) {
    /// Universal IUnknown vtable — every COM interface starts here.
    pub struct VtIUnknown {
        QueryInterface: fn(i64, *const GUID, **void): i32
        AddRef:         fn(i64): u32
        Release:        fn(i64): u32
    }

    /// ID3D12Device — IUnknown + ID3D12Object (4 slots) + device API.
    /// Only the methods this repo currently uses are typed; the
    /// rest hold the slot as i64 so future callers can fill in
    /// signatures without renumbering.
    pub struct VtID3D12Device {
        QueryInterface:          fn(i64, *const GUID, **void): i32
        AddRef:                  fn(i64): u32
        Release:                 fn(i64): u32
        GetPrivateData:          i64
        SetPrivateData:          i64
        SetPrivateDataInterface: i64
        SetName:                 i64
        GetNodeCount:            fn(i64): u32
        CreateCommandQueue:      fn(i64, *const D3D12_COMMAND_QUEUE_DESC, *const GUID, **void): i32
    }
}
