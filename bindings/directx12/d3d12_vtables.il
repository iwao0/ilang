// COM vtable layouts for D3D12 / IUnknown.
//
// Each enum lists the byte offset of every method of a given COM
// interface (full vtable, including inherited slots from IUnknown
// / ID3D12Object / ID3D12DeviceChild / ID3D12CommandList).  Use
// at the call site to replace bare integer offsets:
//
//     // before                            // after
//     readU64(vtbl, 64)        →           readU64(vtbl, ID3D12DeviceVtbl.CreateCommandQueue as i64)
//     readU64(vtbl, 16)        →           readU64(vtbl, IUnknownVtbl.Release as i64)
//
// Source of truth: Windows 10 SDK `d3d12.h` (FAR pointer-sized x64).
// Each method occupies 8 bytes (function pointer slot). Inherited
// slots are repeated in every derived interface so any enum has
// the absolute offset of every method callable through that vtable.

/// Universal IUnknown vtable — every COM interface starts with these
/// three slots. Pull `Release` / `AddRef` from here when you don't
/// care which interface you have.
pub enum IUnknownVtbl: u32 {
    QueryInterface = 0
    AddRef         = 8
    Release        = 16
}

/// ID3D12Object — IUnknown + 4 debug/naming methods. Most D3D12
/// interfaces inherit from this.
pub enum ID3D12ObjectVtbl: u32 {
    QueryInterface          = 0
    AddRef                  = 8
    Release                 = 16
    GetPrivateData          = 24
    SetPrivateData          = 32
    SetPrivateDataInterface = 40
    SetName                 = 48
}

/// ID3D12Device — main GPU device.  Direct child of ID3D12Object.
pub enum ID3D12DeviceVtbl: u32 {
    // IUnknown
    QueryInterface                   = 0
    AddRef                           = 8
    Release                          = 16
    // ID3D12Object
    GetPrivateData                   = 24
    SetPrivateData                   = 32
    SetPrivateDataInterface          = 40
    SetName                          = 48
    // ID3D12Device
    GetNodeCount                     = 56
    CreateCommandQueue               = 64
    CreateCommandAllocator           = 72
    CreateGraphicsPipelineState      = 80
    CreateComputePipelineState       = 88
    CreateCommandList                = 96
    CheckFeatureSupport              = 104
    CreateDescriptorHeap             = 112
    GetDescriptorHandleIncrementSize = 120
    CreateRootSignature              = 128
    CreateConstantBufferView         = 136
    CreateShaderResourceView         = 144
    CreateUnorderedAccessView        = 152
    CreateRenderTargetView           = 160
    CreateDepthStencilView           = 168
    CreateSampler                    = 176
    CopyDescriptors                  = 184
    CopyDescriptorsSimple            = 192
    GetResourceAllocationInfo        = 200
    GetCustomHeapProperties          = 208
    CreateCommittedResource          = 216
    CreateHeap                       = 224
    CreatePlacedResource             = 232
    CreateReservedResource           = 240
    CreateSharedHandle               = 248
    OpenSharedHandle                 = 256
    OpenSharedHandleByName           = 264
    MakeResident                     = 272
    Evict                            = 280
    CreateFence                      = 288
    GetDeviceRemovedReason           = 296
    GetCopyableFootprints            = 304
    CreateQueryHeap                  = 312
    SetStablePowerState              = 320
    CreateCommandSignature           = 328
    GetResourceTiling                = 336
    GetAdapterLuid                   = 344
}

/// ID3D12CommandQueue — IUnknown → ID3D12Object → ID3D12DeviceChild
/// → ID3D12Pageable → ID3D12CommandQueue.
pub enum ID3D12CommandQueueVtbl: u32 {
    // IUnknown
    QueryInterface          = 0
    AddRef                  = 8
    Release                 = 16
    // ID3D12Object
    GetPrivateData          = 24
    SetPrivateData          = 32
    SetPrivateDataInterface = 40
    SetName                 = 48
    // ID3D12DeviceChild
    GetDevice               = 56
    // ID3D12Pageable: no new methods
    // ID3D12CommandQueue
    UpdateTileMappings      = 64
    CopyTileMappings        = 72
    ExecuteCommandLists     = 80
    SetMarker               = 88
    BeginEvent              = 96
    EndEvent                = 104
    Signal                  = 112
    Wait                    = 120
    GetTimestampFrequency   = 128
    GetClockCalibration     = 136
    GetDesc                 = 144
}

/// ID3D12CommandAllocator — adds a single `Reset` method to the
/// standard pageable-device-child chain.
pub enum ID3D12CommandAllocatorVtbl: u32 {
    QueryInterface          = 0
    AddRef                  = 8
    Release                 = 16
    GetPrivateData          = 24
    SetPrivateData          = 32
    SetPrivateDataInterface = 40
    SetName                 = 48
    GetDevice               = 56
    Reset                   = 64
}

/// ID3D12Fence — synchronisation primitive.
pub enum ID3D12FenceVtbl: u32 {
    QueryInterface          = 0
    AddRef                  = 8
    Release                 = 16
    GetPrivateData          = 24
    SetPrivateData          = 32
    SetPrivateDataInterface = 40
    SetName                 = 48
    GetDevice               = 56
    GetCompletedValue       = 64
    SetEventOnCompletion    = 72
    Signal                  = 80
}

/// ID3D12Resource — buffers and textures.
pub enum ID3D12ResourceVtbl: u32 {
    QueryInterface          = 0
    AddRef                  = 8
    Release                 = 16
    GetPrivateData          = 24
    SetPrivateData          = 32
    SetPrivateDataInterface = 40
    SetName                 = 48
    GetDevice               = 56
    Map                     = 64
    Unmap                   = 72
    GetDesc                 = 80
    GetGPUVirtualAddress    = 88
    WriteToSubresource      = 96
    ReadFromSubresource     = 104
    GetHeapProperties       = 112
}

/// ID3D12DescriptorHeap — RTV / DSV / CBV-SRV-UAV / Sampler heaps.
pub enum ID3D12DescriptorHeapVtbl: u32 {
    QueryInterface              = 0
    AddRef                      = 8
    Release                     = 16
    GetPrivateData              = 24
    SetPrivateData              = 32
    SetPrivateDataInterface     = 40
    SetName                     = 48
    GetDevice                   = 56
    GetDesc                     = 64
    GetCPUDescriptorHandleForHeapStart = 72
    GetGPUDescriptorHandleForHeapStart = 80
}

/// ID3D12GraphicsCommandList — recording surface for graphics
/// command lists. Most methods you call after creating a list
/// live here.
pub enum ID3D12GraphicsCommandListVtbl: u32 {
    // IUnknown / ID3D12Object / ID3D12DeviceChild / ID3D12CommandList
    QueryInterface          = 0
    AddRef                  = 8
    Release                 = 16
    GetPrivateData          = 24
    SetPrivateData          = 32
    SetPrivateDataInterface = 40
    SetName                 = 48
    GetDevice               = 56
    GetType                 = 64
    // ID3D12GraphicsCommandList
    Close                   = 72
    Reset                   = 80
    ClearState              = 88
    DrawInstanced           = 96
    DrawIndexedInstanced    = 104
    Dispatch                = 112
    CopyBufferRegion        = 120
    CopyTextureRegion       = 128
    CopyResource            = 136
    CopyTiles               = 144
    ResolveSubresource      = 152
    IASetPrimitiveTopology  = 160
    RSSetViewports          = 168
    RSSetScissorRects       = 176
    OMSetBlendFactor        = 184
    OMSetStencilRef         = 192
    SetPipelineState        = 200
    ResourceBarrier         = 208
    ExecuteBundle           = 216
    SetDescriptorHeaps      = 224
    SetComputeRootSignature        = 232
    SetGraphicsRootSignature       = 240
    SetComputeRootDescriptorTable  = 248
    SetGraphicsRootDescriptorTable = 256
    SetComputeRoot32BitConstant    = 264
    SetGraphicsRoot32BitConstant   = 272
    SetComputeRoot32BitConstants   = 280
    SetGraphicsRoot32BitConstants  = 288
    SetComputeRootConstantBufferView   = 296
    SetGraphicsRootConstantBufferView  = 304
    SetComputeRootShaderResourceView   = 312
    SetGraphicsRootShaderResourceView  = 320
    SetComputeRootUnorderedAccessView  = 328
    SetGraphicsRootUnorderedAccessView = 336
    IASetIndexBuffer        = 344
    IASetVertexBuffers      = 352
    SOSetTargets            = 360
    OMSetRenderTargets      = 368
    ClearDepthStencilView   = 376
    ClearRenderTargetView   = 384
    ClearUnorderedAccessViewUint  = 392
    ClearUnorderedAccessViewFloat = 400
    DiscardResource         = 408
    BeginQuery              = 416
    EndQuery                = 424
    ResolveQueryData        = 432
    SetPredication          = 440
    SetMarker               = 448
    BeginEvent              = 456
    EndEvent                = 464
    ExecuteIndirect         = 472
}

/// ID3D12RootSignature — no extra methods beyond ID3D12DeviceChild.
pub enum ID3D12RootSignatureVtbl: u32 {
    QueryInterface          = 0
    AddRef                  = 8
    Release                 = 16
    GetPrivateData          = 24
    SetPrivateData          = 32
    SetPrivateDataInterface = 40
    SetName                 = 48
    GetDevice               = 56
}

/// ID3D12PipelineState — adds `GetCachedBlob` for PSO caching.
pub enum ID3D12PipelineStateVtbl: u32 {
    QueryInterface          = 0
    AddRef                  = 8
    Release                 = 16
    GetPrivateData          = 24
    SetPrivateData          = 32
    SetPrivateDataInterface = 40
    SetName                 = 48
    GetDevice               = 56
    GetCachedBlob           = 64
}

/// ID3D12Debug — flat: just `EnableDebugLayer` after IUnknown.
pub enum ID3D12DebugVtbl: u32 {
    QueryInterface     = 0
    AddRef             = 8
    Release            = 16
    EnableDebugLayer   = 24
}
