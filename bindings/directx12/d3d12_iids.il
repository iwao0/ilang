// SDK-published GUIDs for D3D12 COM interfaces.
//
// Wraps the standard IIDs so user code never has to retype the
// {189819f1-…} octets. Mirrors the C SDK convention of shipping
// pre-defined `IID_*` symbols in `dxguid.lib`.
//
// IID layout helper:
//   `xxxxxxxx-aaaa-bbbb-cccc-dddddddddddd`
//   Data1   = 0xxxxxxxxx              (u32)
//   Data2   = 0xaaaa                  (u16)
//   Data3   = 0xbbbb                  (u16)
//   Data4lo = little-endian u32 of the first 4 bytes of `cccc || dddd`
//   Data4hi = little-endian u32 of the last 4 bytes

use d3d12 { GUID }

/// IID_ID3D12Device = 189819f1-1db6-4b57-be54-1821339b85f7
pub const IID_ID3D12Device: GUID = GUID {
    Data1: 0x189819f1, Data2: 0x1db6, Data3: 0x4b57,
    Data4lo: 0x211854be, Data4hi: 0xf7859b33,
}

/// IID_ID3D12CommandQueue = 0ec870a6-5d7e-4c22-8cfc-5baae07616ed
pub const IID_ID3D12CommandQueue: GUID = GUID {
    Data1: 0x0ec870a6, Data2: 0x5d7e, Data3: 0x4c22,
    Data4lo: 0xaa5bfc8c, Data4hi: 0xed1676e0,
}

/// IID_ID3D12CommandAllocator = 6102dee4-af59-4b09-b999-b44d73f09b24
pub const IID_ID3D12CommandAllocator: GUID = GUID {
    Data1: 0x6102dee4, Data2: 0xaf59, Data3: 0x4b09,
    Data4lo: 0x4db499b9, Data4hi: 0x249bf073,
}

/// IID_ID3D12GraphicsCommandList = 5b160d0f-ac1b-4185-8ba8-b3ae42a5a455
pub const IID_ID3D12GraphicsCommandList: GUID = GUID {
    Data1: 0x5b160d0f, Data2: 0xac1b, Data3: 0x4185,
    Data4lo: 0xaeb3a88b, Data4hi: 0x55a4a542,
}

/// IID_ID3D12Debug = 344488b7-6846-474b-b989-f027448245e0
pub const IID_ID3D12Debug: GUID = GUID {
    Data1: 0x344488b7, Data2: 0x6846, Data3: 0x474b,
    Data4lo: 0x27f089b9, Data4hi: 0xe0458244,
}
