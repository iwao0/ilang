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

/// IID_ID3D12Resource = 696442be-a72e-4059-bc79-5b5c98040fad
pub const IID_ID3D12Resource: GUID = GUID {
    Data1: 0x696442be, Data2: 0xa72e, Data3: 0x4059,
    Data4lo: 0x5c5b79bc, Data4hi: 0xad0f0498,
}

/// IID_ID3D12DescriptorHeap = 8efb471d-616c-4f49-90f7-127bb763fa51
pub const IID_ID3D12DescriptorHeap: GUID = GUID {
    Data1: 0x8efb471d, Data2: 0x616c, Data3: 0x4f49,
    Data4lo: 0x7b12f790, Data4hi: 0x51fa63b7,
}

/// IID_ID3D12RootSignature = c54a6b66-72df-4ee8-8be5-a946a1429214
pub const IID_ID3D12RootSignature: GUID = GUID {
    Data1: 0xc54a6b66, Data2: 0x72df, Data3: 0x4ee8,
    Data4lo: 0x46a9e58b, Data4hi: 0x14924241,
}

/// IID_ID3D12PipelineState = 765a30f3-f624-4c6f-a828-ace948622445
pub const IID_ID3D12PipelineState: GUID = GUID {
    Data1: 0x765a30f3, Data2: 0xf624, Data3: 0x4c6f,
    Data4lo: 0xe9ac28a8, Data4hi: 0x45246248,
}

/// IID_ID3D12Fence = 0a753dcf-c4d8-4b91-adf6-be5a60d95a76
pub const IID_ID3D12Fence: GUID = GUID {
    Data1: 0x0a753dcf, Data2: 0xc4d8, Data3: 0x4b91,
    Data4lo: 0x5abef6ad, Data4hi: 0x765ad960,
}

/// IID_IDXGIFactory4 = 1bc6ea02-ef36-464f-bf0c-21ca39e5168a
pub const IID_IDXGIFactory4: GUID = GUID {
    Data1: 0x1bc6ea02, Data2: 0xef36, Data3: 0x464f,
    Data4lo: 0xca210cbf, Data4hi: 0x8a16e539,
}

/// IID_IDXGISwapChain3 = 94d99bdb-f1f8-4ab0-b236-7da0170edab1
pub const IID_IDXGISwapChain3: GUID = GUID {
    Data1: 0x94d99bdb, Data2: 0xf1f8, Data3: 0x4ab0,
    Data4lo: 0xa07d3672, Data4hi: 0xb1da0e17,
}
