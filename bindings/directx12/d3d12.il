// d3d12.dll — Direct3D 12 core API.
//
// D3D12 is COM-based: most work happens via vtable calls on
// ID3D12Device, ID3D12CommandQueue, etc. The flat entry points
// listed here bootstrap the device factory, then the rest of the
// API is reached via the device's `QueryInterface` chain.
//
// Required Windows version: Windows 10 1507+ for D3D12, with
// progressively newer features in 1607 / 1709 / 1903 / Win11.
//
// Linking: d3d12.lib (Windows 10 SDK).  At runtime the DLL is
// d3d12.dll (forwarded transparently).

// Re-use the DXGI sample-descriptor struct from the windows
// binding rather than duplicating it here — both modules end up
// referenced from the same example via their parent `ilang.toml`,
// and keeping a single canonical type stops the type checker
// from rejecting cross-binding struct literals.
use dxgi { DXGI_SAMPLE_DESC }

// ─── Feature-level identifiers (D3D_FEATURE_LEVEL_*) ─────────────
pub enum D3DFeatureLevel: u32 {
    /// 9.1 — Windows Phone fallback only.
    level_9_1   = 0x9100
    level_9_2   = 0x9200
    level_9_3   = 0x9300
    level_10_0  = 0xa000
    level_10_1  = 0xa100
    level_11_0  = 0xb000
    level_11_1  = 0xb100
    level_12_0  = 0xc000
    level_12_1  = 0xc100
    level_12_2  = 0xc200    // DirectX 12 Ultimate baseline (RT, Mesh, VRS)
}

// ─── Device removal reasons (D3D12_DEVICE_REMOVED_*) ─────────────
pub enum D3D12DeviceRemovedReason: i32 {
    hung           = -2005270523    // DXGI_ERROR_DEVICE_HUNG
    removed        = -2005270523
    reset          = -2005270521
    driverInternal = -2005270522
}

// ─── Heap types (D3D12_HEAP_TYPE) ────────────────────────────────
pub enum D3D12HeapType: u32 {
    default  = 1    // GPU-local — fastest for the GPU
    upload   = 2    // CPU-write, GPU-read (staging)
    readback = 3    // GPU-write, CPU-read
    custom   = 4    // explicit CPU/GPU page properties
}

// ─── CPU page property for custom heaps ──────────────────────────
pub enum D3D12CpuPageProperty: u32 {
    unknown        = 0
    notAvailable   = 1
    writeCombine   = 2
    writeBack      = 3
}

// ─── Memory pool ─────────────────────────────────────────────────
pub enum D3D12MemoryPool: u32 {
    unknown = 0
    l0      = 1    // system memory (UMA) or PCIe (discrete)
    l1      = 2    // local GPU memory (discrete only)
}

// ─── Heap creation flags (D3D12_HEAP_FLAGS) ──────────────────────
@flags
pub enum D3D12HeapFlags: u32 {
    none                          = 0x0
    shared                        = 0x1
    denyBuffers                   = 0x4
    allowDisplay                  = 0x8
    sharedCrossAdapter            = 0x20
    denyRtDsTextures              = 0x40
    denyNonRtDsTextures           = 0x80
    hardwareProtected             = 0x100
    allowWriteWatch               = 0x200
    allowShaderAtomics            = 0x400
    createNotResident             = 0x800
    createNotZeroed               = 0x1000
    allowAllBuffersAndTextures    = 0x0
    allowOnlyBuffers              = 0xc0
    allowOnlyNonRtDsTextures      = 0x44
    allowOnlyRtDsTextures         = 0x84
}

// ─── Resource dimension (D3D12_RESOURCE_DIMENSION) ───────────────
pub enum D3D12ResourceDimension: u32 {
    unknown   = 0
    buffer    = 1
    texture1d = 2
    texture2d = 3
    texture3d = 4
}

// ─── Texture layout ──────────────────────────────────────────────
pub enum D3D12TextureLayout: u32 {
    unknown                = 0
    rowMajor               = 1
    undefinedSwizzle64kb   = 2
    standardSwizzle64kb    = 3
}

// ─── Resource flags (D3D12_RESOURCE_FLAGS) ───────────────────────
@flags
pub enum D3D12ResourceFlags: u32 {
    none                       = 0x0
    allowRenderTarget          = 0x1
    allowDepthStencil          = 0x2
    allowUnorderedAccess       = 0x4
    denyShaderResource         = 0x8
    allowCrossAdapter          = 0x10
    allowSimultaneousAccess    = 0x20
    videoDecodeReferenceOnly   = 0x40
    videoEncodeReferenceOnly   = 0x80
    raytracingAccelerationStructure = 0x100
}

// ─── Resource state bitfield (D3D12_RESOURCE_STATES) ─────────────
@flags
pub enum D3D12ResourceStates: u32 {
    common                          = 0x0
    vertexAndConstantBuffer         = 0x1
    indexBuffer                     = 0x2
    renderTarget                    = 0x4
    unorderedAccess                 = 0x8
    depthWrite                      = 0x10
    depthRead                       = 0x20
    nonPixelShaderResource          = 0x40
    pixelShaderResource             = 0x80
    streamOut                       = 0x100
    indirectArgument                = 0x200
    copyDest                        = 0x400
    copySource                      = 0x800
    resolveDest                     = 0x1000
    resolveSource                   = 0x2000
    raytracingAccelerationStructure = 0x400000
    shadingRateSource               = 0x1000000
    /// PRESENT == COMMON == 0.
    present                         = 0x0
    genericRead                     = 0xac3   // VB | IB | CB | SRV[nonPS] | SRV[PS] | INDIRECT | COPY_SRC
    allShaderResource               = 0xc0
}

// ─── Descriptor heap type ────────────────────────────────────────
pub enum D3D12DescriptorHeapType: u32 {
    cbvSrvUav     = 0
    sampler       = 1
    rtv           = 2
    dsv           = 3
}

@flags
pub enum D3D12DescriptorHeapFlags: u32 {
    none         = 0
    shaderVisible = 1
}

// ─── Command list type (D3D12_COMMAND_LIST_TYPE) ─────────────────
pub enum D3D12CommandListType: i32 {
    direct       = 0    // graphics + compute + copy
    bundle       = 1
    compute      = 2
    copy         = 3
    videoDecode  = 4
    videoProcess = 5
    videoEncode  = 6
}

@flags
pub enum D3D12CommandQueueFlags: u32 {
    none                = 0
    disableGpuTimeout   = 1
}

pub enum D3D12CommandQueuePriority: i32 {
    normal           = 0
    high             = 100
    globalRealtime   = 10000
}

@flags
pub enum D3D12FenceFlags: u32 {
    none                  = 0
    shared                = 1
    sharedCrossAdapter    = 2
    nonMonitored          = 4
}

// ─── Primitive topology type (D3D12_PRIMITIVE_TOPOLOGY_TYPE) ─────
pub enum D3D12PrimitiveTopologyType: u32 {
    undefined = 0
    point     = 1
    line      = 2
    triangle  = 3
    patch     = 4
}

// ─── D3D_PRIMITIVE_TOPOLOGY (IASetPrimitiveTopology) ─────────────
pub enum D3DPrimitiveTopology: u32 {
    undefined        = 0
    pointList        = 1
    lineList         = 2
    lineStrip        = 3
    triangleList     = 4
    triangleStrip    = 5
    lineListAdj      = 10
    lineStripAdj     = 11
    triangleListAdj  = 12
    triangleStripAdj = 13
}

// ─── Fill mode ───────────────────────────────────────────────────
pub enum D3D12FillMode: u32 {
    wireframe = 2
    solid     = 3
}

pub enum D3D12CullMode: u32 {
    none  = 1
    front = 2
    back  = 3
}

// ─── Comparison func ─────────────────────────────────────────────
pub enum D3D12ComparisonFunc: u32 {
    none         = 0
    never        = 1
    less         = 2
    equal        = 3
    lessEqual    = 4
    greater      = 5
    notEqual     = 6
    greaterEqual = 7
    always       = 8
}

// ─── Blend factor ────────────────────────────────────────────────
pub enum D3D12Blend: u32 {
    zero            = 1
    one             = 2
    srcColor        = 3
    invSrcColor     = 4
    srcAlpha        = 5
    invSrcAlpha     = 6
    destAlpha       = 7
    invDestAlpha    = 8
    destColor       = 9
    invDestColor    = 10
    srcAlphaSat     = 11
    blendFactor     = 14
    invBlendFactor  = 15
    src1Color       = 16
    invSrc1Color    = 17
    src1Alpha       = 18
    invSrc1Alpha    = 19
}

pub enum D3D12BlendOp: u32 {
    add         = 1
    subtract    = 2
    revSubtract = 3
    min         = 4
    max         = 5
}

// ─── Texture filter ──────────────────────────────────────────────
pub enum D3D12Filter: u32 {
    minMagMipPoint                  = 0x0
    minMagPointMipLinear            = 0x1
    minPointMagLinearMipPoint       = 0x4
    minPointMagMipLinear            = 0x5
    minLinearMagMipPoint            = 0x10
    minLinearMagPointMipLinear      = 0x11
    minMagLinearMipPoint            = 0x14
    minMagMipLinear                 = 0x15
    anisotropic                     = 0x55
    comparisonMinMagMipPoint        = 0x80
    comparisonMinMagMipLinear       = 0x95
    comparisonAnisotropic           = 0xd5
}

// ─── Texture address mode ────────────────────────────────────────
pub enum D3D12TextureAddressMode: u32 {
    wrap        = 1
    mirror      = 2
    clamp       = 3
    border      = 4
    mirrorOnce  = 5
}

// ─── Resource barrier ────────────────────────────────────────────
pub enum D3D12ResourceBarrierType: u32 {
    transition = 0
    aliasing   = 1
    uav        = 2
}

@flags
pub enum D3D12ResourceBarrierFlags: u32 {
    none      = 0
    beginOnly = 1
    endOnly   = 2
}

// ─── Resource barrier "all subresources" sentinel ────────────────
//   D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES = 0xFFFFFFFF

// ─── HRESULT codes specific to D3D12 ─────────────────────────────
pub enum D3D12Result: i32 {
    ok                          = 0
    adapterNotFound             = -2005270512   // 0x887E0001
    driverVersionMismatch       = -2005270511   // 0x887E0002
    invalidArg                  = -2147024809   // E_INVALIDARG
    outOfMemory                 = -2147024882   // E_OUTOFMEMORY
    notImpl                     = -2147467263   // E_NOTIMPL
    fail                        = -2147467259   // E_FAIL
    deviceRemoved               = -2005270523
    deviceReset                 = -2005270521
    notCurrentlyAvailable       = -2005270527
}

// ─── ID3D12Object query flags (debug naming, SetName) ────────────

@extern(C, "d3d12") {
    // ── COM interface handle types ────────────────────────────────
    // All ID3D12* interfaces are opaque pointers; calls go through
    // their COM vtables. The bindings expose the handle types so
    // user code can hold typed pointers between flat-entry-point
    // calls and the vtable invocations they bootstrap.

    // ── GUID (locally declared; layout-compatible with ole32's GUID) ─

    /// 16-byte interface identifier. Used as the `riid` argument to
    /// D3D12 / DXGI factory calls.
    pub struct GUID {
        Data1:   u32
        Data2:   u16
        Data3:   u16
        Data4lo: u32
        Data4hi: u32
    }

    pub struct ID3D12Object {}
    pub struct ID3D12DeviceChild {}
    pub struct ID3D12Device {}
    pub struct ID3D12Device1 {}
    pub struct ID3D12Device2 {}
    pub struct ID3D12Device5 {}    // ray tracing
    pub struct ID3D12Device7 {}
    pub struct ID3D12CommandQueue {}
    pub struct ID3D12CommandList {}
    pub struct ID3D12GraphicsCommandList {}
    pub struct ID3D12GraphicsCommandList4 {}    // ray tracing
    pub struct ID3D12CommandAllocator {}
    pub struct ID3D12Resource {}
    pub struct ID3D12Heap {}
    pub struct ID3D12DescriptorHeap {}
    pub struct ID3D12Fence {}
    pub struct ID3D12Fence1 {}
    pub struct ID3D12RootSignature {}
    pub struct ID3D12RootSignatureDeserializer {}
    pub struct ID3D12PipelineState {}
    pub struct ID3D12PipelineLibrary {}
    pub struct ID3D12StateObject {}    // ray tracing pipeline
    pub struct ID3D12Debug {}
    pub struct ID3D12Debug1 {}
    pub struct ID3D12Debug3 {}
    pub struct ID3D12DebugDevice {}
    pub struct ID3D12InfoQueue {}
    pub struct ID3D12QueryHeap {}

    // ── D3D12_HEAP_PROPERTIES — 20 bytes ─────────────────────────

    /// Describes where (and on which physical heap) a resource lives.
    /// For Type != custom, CPUPageProperty and MemoryPoolPreference
    /// must be UNKNOWN (0).
    pub struct D3D12_HEAP_PROPERTIES {
        Type:                 u32       // D3D12HeapType.*
        CPUPageProperty:      u32       // D3D12CpuPageProperty.*
        MemoryPoolPreference: u32       // D3D12MemoryPool.*
        CreationNodeMask:     u32       // 1 for single-GPU
        VisibleNodeMask:      u32       // 1 for single-GPU
    }

    // ── D3D12_HEAP_DESC — 48 bytes ───────────────────────────────

    pub struct D3D12_HEAP_DESC {
        SizeInBytes:     u64
        Properties:      D3D12_HEAP_PROPERTIES
        // 4 bytes implicit padding before Alignment (u64-aligned)
        Alignment:       u64
        Flags:           u32           // D3D12HeapFlags.*
        _pad:            u32
    }

    // ── D3D12_RESOURCE_DESC — 72 bytes ───────────────────────────

    /// Describes a texture or buffer. `Layout` controls tiling;
    /// `Flags` carries render-target / depth-stencil / UAV bits.
    pub struct D3D12_RESOURCE_DESC {
        Dimension:         u32       // D3D12ResourceDimension.*
        // 4 bytes implicit padding before Alignment (u64-aligned)
        Alignment:         u64       // 0 = let runtime pick; or 4 KB / 64 KB / 4 MB
        Width:             u64       // for buffers: size in bytes
        Height:            u32
        DepthOrArraySize:  u16
        MipLevels:         u16
        Format:            u32       // DXGI_FORMAT
        SampleDesc:        DXGI_SAMPLE_DESC
        Layout:            u32       // D3D12TextureLayout.*
        Flags:             u32       // D3D12ResourceFlags.*
    }

    // ── D3D12_RESOURCE_ALLOCATION_INFO — 16 bytes ────────────────

    pub struct D3D12_RESOURCE_ALLOCATION_INFO {
        SizeInBytes: u64
        Alignment:   u64
    }

    // ── D3D12_COMMAND_QUEUE_DESC — 16 bytes ──────────────────────

    pub struct D3D12_COMMAND_QUEUE_DESC {
        Type:     i32       // D3D12CommandListType.*
        Priority: i32       // D3D12CommandQueuePriority.*
        Flags:    u32       // D3D12CommandQueueFlags.*
        NodeMask: u32
    }

    // ── D3D12_DESCRIPTOR_HEAP_DESC — 16 bytes ────────────────────

    pub struct D3D12_DESCRIPTOR_HEAP_DESC {
        Type:           u32     // D3D12DescriptorHeapType.*
        NumDescriptors: u32
        Flags:          u32     // D3D12DescriptorHeapFlags.*
        NodeMask:       u32
    }

    // ── Descriptor handles (raw addresses, opaque) ───────────────

    /// 8 bytes on x64 — a SIZE_T pointing into a CPU-visible descriptor heap.
    pub struct D3D12_CPU_DESCRIPTOR_HANDLE {
        ptr: u64
    }

    /// 8 bytes — UINT64 GPU virtual address into a shader-visible heap.
    pub struct D3D12_GPU_DESCRIPTOR_HANDLE {
        ptr: u64
    }

    // ── D3D12_VIEWPORT — 24 bytes ────────────────────────────────

    pub struct D3D12_VIEWPORT {
        TopLeftX:  f32
        TopLeftY:  f32
        Width:     f32
        Height:    f32
        MinDepth:  f32
        MaxDepth:  f32
    }

    // ── D3D12_BOX — 24 bytes ─────────────────────────────────────

    pub struct D3D12_BOX {
        left:    u32
        top:     u32
        front:   u32
        right:   u32
        bottom:  u32
        back:    u32
    }

    // ── D3D12_RANGE — 16 bytes ───────────────────────────────────

    /// Used by Map / Unmap: a Begin..End range of bytes the CPU
    /// will read or write. Pass `0..0` to indicate "no CPU read".
    pub struct D3D12_RANGE {
        Begin: u64
        End:   u64
    }

    // ── D3D12_CLEAR_VALUE — 20 bytes ─────────────────────────────

    /// Colour (RGBA f32×4 = 16 bytes) OR depth-stencil (4 bytes).
    /// The union is represented as 4 × u32 — write floats via
    /// reinterpretation. `Format` selects the interpretation.
    pub struct D3D12_CLEAR_VALUE {
        Format: u32
        v0:     u32   // f32 R bits   OR   f32 Depth bits
        v1:     u32   // f32 G bits   OR   u8 Stencil + 24 padding
        v2:     u32   // f32 B bits
        v3:     u32   // f32 A bits
    }

    // ── D3D12_VERTEX_BUFFER_VIEW — 16 bytes ──────────────────────

    pub struct D3D12_VERTEX_BUFFER_VIEW {
        BufferLocation: u64       // ID3D12Resource::GetGPUVirtualAddress()
        SizeInBytes:    u32
        StrideInBytes:  u32
    }

    // ── D3D12_INDEX_BUFFER_VIEW — 16 bytes ───────────────────────

    pub struct D3D12_INDEX_BUFFER_VIEW {
        BufferLocation: u64
        SizeInBytes:    u32
        Format:         u32       // R16_UINT (57) or R32_UINT (42)
    }

    // ── D3D12_RESOURCE_BARRIER — 40 bytes ────────────────────────

    /// Union of three barrier types — only Transition is laid out
    /// here as the typical case.  Cast for aliasing / UAV barriers.
    pub struct D3D12_RESOURCE_BARRIER {
        Type:    u32          // D3D12ResourceBarrierType.*
        Flags:   u32          // D3D12ResourceBarrierFlags.*
        /// pResource for transitions / aliasing.
        pResource:       *ID3D12Resource
        Subresource:     u32  // D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES = 0xFFFFFFFF
        StateBefore:     u32  // D3D12ResourceStates.*
        StateAfter:      u32  // D3D12ResourceStates.*
        // For aliasing barriers, reuse pResource as pResourceBefore
        // and Subresource as pResourceAfter (rebind via cast).
        _pad:            u32
    }

    // ── Flat entry points ─────────────────────────────────────────

    /// Creates a Direct3D 12 device on `pAdapter` (NULL = default).
    /// `MinimumFeatureLevel`: D3DFeatureLevel.*.
    /// `riid`: typically IID_ID3D12Device.
    /// `ppDevice`: receives the device interface pointer.
    /// Returns S_OK or a DXGI / D3D12 error HRESULT.
    @lib pub fn D3D12CreateDevice(
        pAdapter:            *void,        // *IUnknown / *IDXGIAdapter
        MinimumFeatureLevel: u32,
        riid:                *const GUID,
        ppDevice:            **void
    ): i32

    /// Returns the debug interface (ID3D12Debug). Enables the
    /// debug layer for subsequently-created devices.
    /// Must be called BEFORE D3D12CreateDevice for layer to attach.
    /// Requires the "Graphics Tools" optional Windows feature.
    @lib pub fn D3D12GetDebugInterface(
        riid:     *const GUID,
        ppvDebug: **void
    ): i32

    /// Returns an interface (typically IDXGIDebug1) for retrieving
    /// live-object reports.  ProcessLuid: 0 for current process.
    @lib pub fn D3D12GetInterface(
        rclsid: *const void,
        riid:   *const void,
        ppvDebug: **void
    ): i32

    /// Serialises a root signature blob.  `pRootSignature`: pointer
    /// to a D3D12_ROOT_SIGNATURE_DESC.  `Version`: D3D_ROOT_SIGNATURE_VERSION
    /// (1 or 1.1 = 2).  `ppBlob`: receives the serialised ID3DBlob.
    /// `ppErrorBlob`: optional, NULL or receives a UTF-8 error string blob.
    @lib pub fn D3D12SerializeRootSignature(
        pRootSignature: *const void,
        Version:        u32,
        ppBlob:         **void,
        ppErrorBlob:    **void
    ): i32

    @lib pub fn D3D12SerializeVersionedRootSignature(
        pRootSignature: *const void,
        ppBlob:         **void,
        ppErrorBlob:    **void
    ): i32

    @lib pub fn D3D12CreateRootSignatureDeserializer(
        pSrcData:                 *const void,
        SrcDataSizeInBytes:       u64,
        pRootSignatureDeserializerInterface: *const void,
        ppRootSignatureDeserializer: **void
    ): i32

    @lib pub fn D3D12CreateVersionedRootSignatureDeserializer(
        pSrcData:                 *const void,
        SrcDataSizeInBytes:       u64,
        pRootSignatureDeserializerInterface: *const void,
        ppRootSignatureDeserializer: **void
    ): i32

    /// Opt-in to experimental D3D12 features.  Must be called before
    /// any device creation.  `NumFeatures` / `pIIDs`: array of GUIDs
    /// identifying the experimental features to enable.
    @lib pub fn D3D12EnableExperimentalFeatures(
        NumFeatures:           u32,
        pIIDs:                 *const void,
        pConfigurationStructs: *void,
        pConfigurationStructSizes: *u32
    ): i32

    // ─── Pipeline / barrier / root-sig structs ──────────────────────
    //
    // The structs below describe a complete graphics PSO and the
    // surrounding state objects (rasterizer / blend / depth-stencil
    // / input layout). Most fields are integer enums so the bindings
    // stay flat; enum constants live in the `D3D12*` enum families
    // above plus the new ones in this block.

    /// LONG[4] scissor / clear rectangle. Same layout as WinUser's
    /// `RECT`.
    pub struct D3D12_RECT {
        left:   i32
        top:    i32
        right:  i32
        bottom: i32
    }

    /// Bytecode pointer + length pair used everywhere a compiled
    /// shader is consumed.
    pub struct D3D12_SHADER_BYTECODE {
        pShaderBytecode: *const void
        BytecodeLength:  u64
    }

    /// Two-pointer pair for a cached PSO blob. Pass `{ NULL, 0 }` to
    /// disable PSO caching.
    pub struct D3D12_CACHED_PIPELINE_STATE {
        pCachedBlob:           *const void
        CachedBlobSizeInBytes: u64
    }

    /// Stream-output declaration entries — unused by hello-triangle.
    /// Pass `{ NULL, 0, NULL, 0, 0 }` to disable. `_pad0` keeps
    /// `pBufferStrides` 8-byte aligned to match MSVC's natural
    /// padding (struct size = 32 bytes).
    pub struct D3D12_STREAM_OUTPUT_DESC {
        pSODeclaration:   *const void
        NumEntries:       u32
        _pad0:            u32
        pBufferStrides:   *const u32
        NumStrides:       u32
        RasterizedStream: u32
    }

    /// `D3D12_RENDER_TARGET_BLEND_DESC` — 32 bytes per render target
    /// in the BLEND_DESC array.
    pub struct D3D12_RENDER_TARGET_BLEND_DESC {
        BlendEnable:           i32       // BOOL
        LogicOpEnable:         i32       // BOOL
        SrcBlend:              u32       // D3D12Blend
        DestBlend:             u32       // D3D12Blend
        BlendOp:               u32       // D3D12BlendOp
        SrcBlendAlpha:         u32       // D3D12Blend
        DestBlendAlpha:        u32       // D3D12Blend
        BlendOpAlpha:          u32       // D3D12BlendOp
        LogicOp:               u32       // D3D12LogicOp
        RenderTargetWriteMask: u8        // mask of D3D12ColorWriteEnable
        _pad0:                 u8
        _pad1:                 u16
    }

    /// Per-RT blend state plus two global toggles. The RT array is
    /// always 8 entries; entries beyond `NumRenderTargets` (in the
    /// PSO desc) are ignored.
    pub struct D3D12_BLEND_DESC {
        AlphaToCoverageEnable:  i32
        IndependentBlendEnable: i32
        RenderTarget0: D3D12_RENDER_TARGET_BLEND_DESC
        RenderTarget1: D3D12_RENDER_TARGET_BLEND_DESC
        RenderTarget2: D3D12_RENDER_TARGET_BLEND_DESC
        RenderTarget3: D3D12_RENDER_TARGET_BLEND_DESC
        RenderTarget4: D3D12_RENDER_TARGET_BLEND_DESC
        RenderTarget5: D3D12_RENDER_TARGET_BLEND_DESC
        RenderTarget6: D3D12_RENDER_TARGET_BLEND_DESC
        RenderTarget7: D3D12_RENDER_TARGET_BLEND_DESC
    }

    /// Rasterizer state — fill mode, cull mode, depth-bias bits.
    pub struct D3D12_RASTERIZER_DESC {
        FillMode:              u32       // D3D12FillMode
        CullMode:              u32       // D3D12CullMode
        FrontCounterClockwise: i32
        DepthBias:             i32
        DepthBiasClamp:        f32
        SlopeScaledDepthBias:  f32
        DepthClipEnable:       i32
        MultisampleEnable:     i32
        AntialiasedLineEnable: i32
        ForcedSampleCount:     u32
        ConservativeRaster:    u32
    }

    /// Per-face stencil ops.
    pub struct D3D12_DEPTH_STENCILOP_DESC {
        StencilFailOp:      u32   // D3D12StencilOp
        StencilDepthFailOp: u32
        StencilPassOp:      u32
        StencilFunc:        u32   // D3D12ComparisonFunc
    }

    /// Depth + stencil test configuration.
    pub struct D3D12_DEPTH_STENCIL_DESC {
        DepthEnable:      i32
        DepthWriteMask:   u32       // D3D12DepthWriteMask
        DepthFunc:        u32       // D3D12ComparisonFunc
        StencilEnable:    i32
        StencilReadMask:  u8
        StencilWriteMask: u8
        _pad0:            u16
        FrontFace:        D3D12_DEPTH_STENCILOP_DESC
        BackFace:         D3D12_DEPTH_STENCILOP_DESC
    }

    /// One vertex-buffer slot's element description.
    pub struct D3D12_INPUT_ELEMENT_DESC {
        SemanticName:         *const char
        SemanticIndex:        u32
        Format:               u32       // DXGI_FORMAT
        InputSlot:            u32
        AlignedByteOffset:    u32
        InputSlotClass:       u32       // D3D12InputClassification
        InstanceDataStepRate: u32
    }

    /// Input layout — pointer + count over `D3D12_INPUT_ELEMENT_DESC`.
    pub struct D3D12_INPUT_LAYOUT_DESC {
        pInputElementDescs: *const D3D12_INPUT_ELEMENT_DESC
        NumElements:        u32
        _pad0:              u32
    }

    /// `D3D12_GRAPHICS_PIPELINE_STATE_DESC` — everything a PSO needs.
    /// Layout follows the SDK's exact field order; sizes are MSVC
    /// x64 aligned (8-byte align on pointers / u64). The `RTVFormats`
    /// array is flattened to 8 u32 slots.
    pub struct D3D12_GRAPHICS_PIPELINE_STATE_DESC {
        pRootSignature:        i64                 // ID3D12RootSignature*
        VS:                    D3D12_SHADER_BYTECODE
        PS:                    D3D12_SHADER_BYTECODE
        DS:                    D3D12_SHADER_BYTECODE
        HS:                    D3D12_SHADER_BYTECODE
        GS:                    D3D12_SHADER_BYTECODE
        StreamOutput:          D3D12_STREAM_OUTPUT_DESC
        BlendState:            D3D12_BLEND_DESC
        SampleMask:            u32
        RasterizerState:       D3D12_RASTERIZER_DESC
        DepthStencilState:     D3D12_DEPTH_STENCIL_DESC
        InputLayout:           D3D12_INPUT_LAYOUT_DESC
        IBStripCutValue:       u32                 // D3D12IndexBufferStripCutValue
        PrimitiveTopologyType: u32                 // D3D12PrimitiveTopologyType
        NumRenderTargets:      u32
        RTVFormat0:            u32
        RTVFormat1:            u32
        RTVFormat2:            u32
        RTVFormat3:            u32
        RTVFormat4:            u32
        RTVFormat5:            u32
        RTVFormat6:            u32
        RTVFormat7:            u32
        DSVFormat:             u32
        SampleDesc:            DXGI_SAMPLE_DESC
        NodeMask:              u32
        CachedPSO:             D3D12_CACHED_PIPELINE_STATE
        Flags:                 u32                 // D3D12PipelineStateFlags
        _pad0:                 u32
    }

    /// `D3D12_ROOT_SIGNATURE_DESC` — simplest possible form
    /// (no parameters / no static samplers). For an empty root
    /// signature use `{ 0, NULL, 0, NULL,
    /// D3D12RootSignatureFlags.allowInputAssemblerInputLayout }`.
    pub struct D3D12_ROOT_SIGNATURE_DESC {
        NumParameters:     u32
        _pad0:             u32
        pParameters:       *const void
        NumStaticSamplers: u32
        _pad1:             u32
        pStaticSamplers:   *const void
        Flags:             u32       // D3D12RootSignatureFlags
        _pad2:             u32
    }
}

// ─── Additional enum families for the structs above ────────────────

pub enum D3D12LogicOp: u32 {
    clear = 0
    set = 1
    copy = 2
    copyInverted = 3
    noop = 4
    invert = 5
    and = 6
    nand = 7
    or = 8
    nor = 9
    xor = 10
    equiv = 11
    andReverse = 12
    andInverted = 13
    orReverse = 14
    orInverted = 15
}

pub enum D3D12StencilOp: u32 {
    keep = 1
    zero = 2
    replace = 3
    incrSat = 4
    decrSat = 5
    invert = 6
    incr = 7
    decr = 8
}

pub enum D3D12DepthWriteMask: u32 {
    zero = 0
    all = 1
}

pub enum D3D12ColorWriteEnable: u8 {
    red = 1
    green = 2
    blue = 4
    alpha = 8
    all = 15
}

pub enum D3D12InputClassification: u32 {
    perVertexData = 0
    perInstanceData = 1
}

pub enum D3D12IndexBufferStripCutValue: u32 {
    disabled = 0
    cut0xFFFF = 1
    cut0xFFFFFFFF = 2
}

pub enum D3D12PipelineStateFlags: u32 {
    none = 0
    toolDebug = 1
}

pub enum D3D12RootSignatureFlags: u32 {
    none = 0
    allowInputAssemblerInputLayout = 1
    denyVertexShaderRootAccess = 2
    denyHullShaderRootAccess = 4
    denyDomainShaderRootAccess = 8
    denyGeometryShaderRootAccess = 16
    denyPixelShaderRootAccess = 32
    allowStreamOutput = 64
}

pub enum D3D12RootSignatureVersion: u32 {
    version_1_0 = 1
    version_1_1 = 2
}

/// Heap creation flags (D3D12_HEAP_FLAG_*). Hello-triangle uses
/// `none` for the upload heap.
pub enum D3D12HeapMiscFlags: u32 {
    none = 0
    shared = 1
    denyBuffers = 4
    allowDisplay = 8
    sharedCrossAdapter = 32
    denyRtDsTextures = 64
    denyNonRtDsTextures = 128
    hardwareProtected = 256
    allowWriteWatch = 512
    allowShaderAtomics = 1024
    createNotResident = 2048
    createNotZeroed = 4096
    allowAllBuffersAndTextures = 0
    allowOnlyBuffers = 192
    allowOnlyNonRtDsTextures = 68
    allowOnlyRtDsTextures = 132
}

pub enum D3DPrimitiveTopologyValue: u32 {
    triangleList = 4
    triangleStrip = 5
    lineList = 2
    lineStrip = 3
    pointList = 1
}

