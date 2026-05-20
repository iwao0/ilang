/// d3d12.dll — Direct3D 12 core API.
///
/// D3D12 is COM-based: most work happens via vtable calls on
/// ID3D12Device, ID3D12CommandQueue, etc. The flat entry points
/// listed here bootstrap the device factory, then the rest of the
/// API is reached via the device's `QueryInterface` chain.
///
/// Required Windows version: Windows 10 1507+ for D3D12, with
/// progressively newer features in 1607 / 1709 / 1903 / Win11.
///
/// Linking: d3d12.lib (Windows 10 SDK).  At runtime the DLL is
/// d3d12.dll (forwarded transparently).

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

    // All of the named ID3D12* handles are declared as `@com pub
    // interface` in d3d12_com.il (re-exported via directx12.il).
    // Anything that needs to flow through this file just refers to
    // those interface names; no opaque placeholders required.

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

    // ── Descriptor-table / root-parameter layout ─────────────────

    /// `D3D12_DESCRIPTOR_RANGE` (1.0 root signatures).
    /// A run of `NumDescriptors` SRV/UAV/CBV/Sampler slots starting
    /// at `BaseShaderRegister` in space `RegisterSpace`. Use
    /// `D3D12_APPEND_ALIGNED_ELEMENT` (0xFFFFFFFF) for
    /// `OffsetInDescriptorsFromTableStart` to append to the
    /// preceding range.
    pub struct D3D12_DESCRIPTOR_RANGE {
        RangeType:                         u32  // D3D12DescriptorRangeType
        NumDescriptors:                    u32
        BaseShaderRegister:                u32
        RegisterSpace:                     u32
        OffsetInDescriptorsFromTableStart: u32
        _pad0:                             u32
    }

    /// `D3D12_DESCRIPTOR_RANGE1` (1.1 root signatures, adds Flags).
    pub struct D3D12_DESCRIPTOR_RANGE1 {
        RangeType:                         u32  // D3D12DescriptorRangeType
        NumDescriptors:                    u32
        BaseShaderRegister:                u32
        RegisterSpace:                     u32
        Flags:                             u32  // D3D12DescriptorRangeFlags
        OffsetInDescriptorsFromTableStart: u32
    }

    /// `D3D12_ROOT_DESCRIPTOR_TABLE` — array of descriptor ranges.
    pub struct D3D12_ROOT_DESCRIPTOR_TABLE {
        NumDescriptorRanges: u32
        _pad0:               u32
        pDescriptorRanges:   *const D3D12_DESCRIPTOR_RANGE
    }

    pub struct D3D12_ROOT_DESCRIPTOR_TABLE1 {
        NumDescriptorRanges: u32
        _pad0:               u32
        pDescriptorRanges:   *const D3D12_DESCRIPTOR_RANGE1
    }

    /// `D3D12_ROOT_CONSTANTS` — push-constants block.
    pub struct D3D12_ROOT_CONSTANTS {
        ShaderRegister: u32
        RegisterSpace:  u32
        Num32BitValues: u32
        _pad0:          u32
    }

    /// `D3D12_ROOT_DESCRIPTOR` (1.0) — single CBV/SRV/UAV descriptor.
    pub struct D3D12_ROOT_DESCRIPTOR {
        ShaderRegister: u32
        RegisterSpace:  u32
    }

    /// `D3D12_ROOT_DESCRIPTOR1` (1.1) — adds Flags.
    pub struct D3D12_ROOT_DESCRIPTOR1 {
        ShaderRegister: u32
        RegisterSpace:  u32
        Flags:          u32  // D3D12RootDescriptorFlags
        _pad0:          u32
    }

    /// `D3D12_ROOT_PARAMETER` (1.0) — discriminated union over
    /// DescriptorTable / Constants / Descriptor. The C union's
    /// payload area is 24 bytes (max of the three variant structs);
    /// write one of `tableNumRanges`/`tableRanges`,
    /// `constShaderRegister`/`Space`/`Num`/`pad`, or
    /// `descShaderRegister`/`Space` depending on `ParameterType`.
    pub struct D3D12_ROOT_PARAMETER {
        ParameterType:     u32        // D3D12RootParameterType
        _pad0:             u32
        // 24-byte union payload, flattened.
        u0:                u64
        u1:                u64
        u2:                u64
        ShaderVisibility:  u32        // D3D12ShaderVisibility
        _pad1:             u32
    }

    /// `D3D12_ROOT_PARAMETER1` (1.1 — descriptor flags variant).
    pub struct D3D12_ROOT_PARAMETER1 {
        ParameterType:     u32        // D3D12RootParameterType
        _pad0:             u32
        u0:                u64
        u1:                u64
        u2:                u64
        ShaderVisibility:  u32        // D3D12ShaderVisibility
        _pad1:             u32
    }

    /// `D3D12_STATIC_SAMPLER_DESC` — sampler embedded in root sig.
    pub struct D3D12_STATIC_SAMPLER_DESC {
        Filter:           u32         // D3D12Filter
        AddressU:         u32         // D3D12TextureAddressMode
        AddressV:         u32
        AddressW:         u32
        MipLODBias:       f32
        MaxAnisotropy:    u32
        ComparisonFunc:   u32         // D3D12ComparisonFunc
        BorderColor:      u32         // D3D12StaticBorderColor
        MinLOD:           f32
        MaxLOD:           f32
        ShaderRegister:   u32
        RegisterSpace:    u32
        ShaderVisibility: u32         // D3D12ShaderVisibility
        _pad0:            u32
    }

    /// `D3D12_ROOT_SIGNATURE_DESC1` (1.1 root signatures).
    pub struct D3D12_ROOT_SIGNATURE_DESC1 {
        NumParameters:     u32
        _pad0:             u32
        pParameters:       *const D3D12_ROOT_PARAMETER1
        NumStaticSamplers: u32
        _pad1:             u32
        pStaticSamplers:   *const D3D12_STATIC_SAMPLER_DESC
        Flags:             u32        // D3D12RootSignatureFlags
        _pad2:             u32
    }

    /// `D3D12_VERSIONED_ROOT_SIGNATURE_DESC` — tagged union over
    /// 1.0 / 1.1 root sig descriptors. Pass to
    /// `D3D12SerializeVersionedRootSignature`. Use only one of the
    /// `desc10` / `desc11` fields based on `Version`.
    pub struct D3D12_VERSIONED_ROOT_SIGNATURE_DESC {
        Version:    u32                          // D3D12RootSignatureVersion
        _pad0:      u32
        // 48-byte union payload — D3D12_ROOT_SIGNATURE_DESC1 size
        // is the max of the two variants. Flattened as 6 × u64 so
        // the call site can poke either layout via a typed pointer
        // cast on `&versioned.u0` (works for both variants since
        // their fields line up at offset 0).
        u0:         u64
        u1:         u64
        u2:         u64
        u3:         u64
        u4:         u64
        u5:         u64
    }

    // ── View descriptors (RTV / DSV / SRV / UAV / CBV / Sampler) ──

    /// `D3D12_CONSTANT_BUFFER_VIEW_DESC` — GPU VA + size for a CBV.
    pub struct D3D12_CONSTANT_BUFFER_VIEW_DESC {
        BufferLocation: u64           // GPU virtual address
        SizeInBytes:    u32           // multiple of 256 bytes
        _pad0:          u32
    }

    /// `D3D12_SHADER_RESOURCE_VIEW_DESC` — view descriptor.
    /// `ViewDimension` chooses which subview struct the trailing
    /// 16 bytes are interpreted as (Buffer / Tex1D / Tex2D / ...).
    /// `Shader4ComponentMapping` defaults to
    /// `D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING` (0x1688).
    pub struct D3D12_SHADER_RESOURCE_VIEW_DESC {
        Format:                  u32     // DXGI_FORMAT
        ViewDimension:           u32     // D3D12SrvDimension
        Shader4ComponentMapping: u32
        _pad0:                   u32
        u0:                      u64
        u1:                      u64
    }

    /// `D3D12_UNORDERED_ACCESS_VIEW_DESC`.
    pub struct D3D12_UNORDERED_ACCESS_VIEW_DESC {
        Format:        u32              // DXGI_FORMAT
        ViewDimension: u32              // D3D12UavDimension
        u0:            u64
        u1:            u64
    }

    /// `D3D12_RENDER_TARGET_VIEW_DESC`.
    pub struct D3D12_RENDER_TARGET_VIEW_DESC {
        Format:        u32              // DXGI_FORMAT
        ViewDimension: u32              // D3D12RtvDimension
        u0:            u64
        u1:            u64
    }

    /// `D3D12_DEPTH_STENCIL_VIEW_DESC`.
    pub struct D3D12_DEPTH_STENCIL_VIEW_DESC {
        Format:        u32              // DXGI_FORMAT
        ViewDimension: u32              // D3D12DsvDimension
        Flags:         u32              // D3D12DsvFlags
        _pad0:         u32
        u0:            u64
    }

    /// `D3D12_SAMPLER_DESC` — non-static (heap-resident) sampler.
    pub struct D3D12_SAMPLER_DESC {
        Filter:         u32             // D3D12Filter
        AddressU:       u32             // D3D12TextureAddressMode
        AddressV:       u32
        AddressW:       u32
        MipLODBias:     f32
        MaxAnisotropy:  u32
        ComparisonFunc: u32             // D3D12ComparisonFunc
        BorderColor:    f32[4]
        MinLOD:         f32
        MaxLOD:         f32
    }

    // ── Compute pipeline ─────────────────────────────────────────

    /// `D3D12_COMPUTE_PIPELINE_STATE_DESC`.
    pub struct D3D12_COMPUTE_PIPELINE_STATE_DESC {
        pRootSignature: i64                     // ID3D12RootSignature*
        CS:             D3D12_SHADER_BYTECODE
        NodeMask:       u32
        _pad0:          u32
        CachedPSO:      D3D12_CACHED_PIPELINE_STATE
        Flags:          u32                     // D3D12PipelineStateFlags
        _pad1:          u32
    }

    // ── Query heap + statistics ──────────────────────────────────

    /// `D3D12_QUERY_HEAP_DESC`.
    pub struct D3D12_QUERY_HEAP_DESC {
        Type:     u32                            // D3D12QueryHeapType
        Count:    u32
        NodeMask: u32
        _pad0:    u32
    }

    /// `D3D12_QUERY_DATA_PIPELINE_STATISTICS` — resolve target for
    /// pipeline-statistics queries (88 bytes).
    pub struct D3D12_QUERY_DATA_PIPELINE_STATISTICS {
        IAVertices:    u64
        IAPrimitives:  u64
        VSInvocations: u64
        GSInvocations: u64
        GSPrimitives:  u64
        CInvocations:  u64
        CPrimitives:   u64
        PSInvocations: u64
        HSInvocations: u64
        DSInvocations: u64
        CSInvocations: u64
    }

    // ── Texture copy / footprint ─────────────────────────────────

    /// `D3D12_SUBRESOURCE_FOOTPRINT` — texture sub-area layout.
    pub struct D3D12_SUBRESOURCE_FOOTPRINT {
        Format:   u32                            // DXGI_FORMAT
        Width:    u32
        Height:   u32
        Depth:    u32
        RowPitch: u32
        _pad0:    u32
    }

    /// `D3D12_PLACED_SUBRESOURCE_FOOTPRINT` — footprint placed at
    /// a fixed offset inside a buffer.
    pub struct D3D12_PLACED_SUBRESOURCE_FOOTPRINT {
        Offset:    u64
        Footprint: D3D12_SUBRESOURCE_FOOTPRINT
    }

    /// `D3D12_TEXTURE_COPY_LOCATION` — discriminated union over
    /// subresource-index / placed-footprint copy sources/dests.
    pub struct D3D12_TEXTURE_COPY_LOCATION {
        pResource: *ID3D12Resource
        Type:      u32                           // D3D12TextureCopyType
        _pad0:     u32
        // 64-byte union payload (PlacedFootprint variant is largest).
        u0:        u64
        u1:        u64
        u2:        u64
        u3:        u64
        u4:        u64
        u5:        u64
        u6:        u64
        u7:        u64
    }

    /// `D3D12_DISCARD_REGION` — sub-region hint for DiscardResource.
    pub struct D3D12_DISCARD_REGION {
        NumRects:         u32
        _pad0:            u32
        pRects:           *const D3D12_RECT
        FirstSubresource: u32
        NumSubresources:  u32
    }

    /// `D3D12_TILED_RESOURCE_COORDINATE`.
    pub struct D3D12_TILED_RESOURCE_COORDINATE {
        X:           u32
        Y:           u32
        Z:           u32
        Subresource: u32
    }

    // ── Feature-support payloads ─────────────────────────────────

    /// `D3D12_FEATURE_DATA_D3D12_OPTIONS` — D3D12_FEATURE.d3d12Options.
    pub struct D3D12_FEATURE_DATA_D3D12_OPTIONS {
        DoublePrecisionFloatShaderOps:           i32
        OutputMergerLogicOp:                     i32
        MinPrecisionSupport:                     u32
        TiledResourcesTier:                      u32
        ResourceBindingTier:                     u32
        PSSpecifiedStencilRefSupported:          i32
        TypedUAVLoadAdditionalFormats:           i32
        ROVsSupported:                           i32
        ConservativeRasterizationTier:           u32
        MaxGPUVirtualAddressBitsPerResource:     u32
        StandardSwizzle64KBSupported:            i32
        CrossNodeSharingTier:                    u32
        CrossAdapterRowMajorTextureSupported:    i32
        VPAndRTArrayIndexFromAnyShaderFeedingRasterizerSupportedWithoutGSEmulation: i32
        ResourceHeapTier:                        u32
    }

    /// `D3D12_FEATURE_DATA_D3D12_OPTIONS5` — D3D12_FEATURE.d3d12Options5.
    /// `RaytracingTier` matches `D3D12RaytracingTier`.
    pub struct D3D12_FEATURE_DATA_D3D12_OPTIONS5 {
        SRVOnlyTiledResourceTier3: i32
        RenderPassesTier:          u32
        RaytracingTier:            u32   // D3D12RaytracingTier
    }

    /// `D3D12_FEATURE_DATA_D3D12_OPTIONS6` — VRS tier query.
    pub struct D3D12_FEATURE_DATA_D3D12_OPTIONS6 {
        AdditionalShadingRatesSupported:                    i32
        PerPrimitiveShadingRateSupportedWithViewportIndexing: i32
        VariableShadingRateTier:                            u32  // D3D12VariableShadingRateTier
        ShadingRateImageTileSize:                           u32
        BackgroundProcessingSupported:                      i32
    }

    /// `D3D12_FEATURE_DATA_D3D12_OPTIONS7` — mesh shader tier query.
    pub struct D3D12_FEATURE_DATA_D3D12_OPTIONS7 {
        MeshShaderTier:        u32  // D3D12MeshShaderTier
        SamplerFeedbackTier:   u32
    }

    /// `D3D12_FEATURE_DATA_SHADER_MODEL` — caller writes its desired
    /// `HighestShaderModel` on input; runtime clamps and returns the
    /// supported value.
    pub struct D3D12_FEATURE_DATA_SHADER_MODEL {
        HighestShaderModel: u32
    }

    /// `D3D12_FEATURE_DATA_ROOT_SIGNATURE` — caller writes the desired
    /// `HighestVersion` on input.
    pub struct D3D12_FEATURE_DATA_ROOT_SIGNATURE {
        HighestVersion: u32   // D3D12RootSignatureVersion
    }

    /// `D3D12_FEATURE_DATA_ARCHITECTURE`.
    pub struct D3D12_FEATURE_DATA_ARCHITECTURE {
        NodeIndex:         u32
        TileBasedRenderer: i32
        UMA:               i32
        CacheCoherentUMA:  i32
    }

    /// `D3D12_FEATURE_DATA_ARCHITECTURE1` — adds `IsolatedMMU`.
    pub struct D3D12_FEATURE_DATA_ARCHITECTURE1 {
        NodeIndex:         u32
        TileBasedRenderer: i32
        UMA:               i32
        CacheCoherentUMA:  i32
        IsolatedMMU:       i32
    }

    /// `D3D12_FEATURE_DATA_GPU_VIRTUAL_ADDRESS_SUPPORT`.
    pub struct D3D12_FEATURE_DATA_GPU_VIRTUAL_ADDRESS_SUPPORT {
        MaxGPUVirtualAddressBitsPerResource: u32
        MaxGPUVirtualAddressBitsPerProcess:  u32
    }

    /// `D3D12_FEATURE_DATA_FEATURE_LEVELS`.
    pub struct D3D12_FEATURE_DATA_FEATURE_LEVELS {
        NumFeatureLevels:        u32
        _pad0:                   u32
        pFeatureLevelsRequested: *const u32
        MaxSupportedFeatureLevel: u32
        _pad1:                   u32
    }

    /// `D3D12_FEATURE_DATA_FORMAT_SUPPORT`.
    pub struct D3D12_FEATURE_DATA_FORMAT_SUPPORT {
        Format:    u32   // DXGI_FORMAT
        Support1:  u32   // D3D12FormatSupport1
        Support2:  u32   // D3D12FormatSupport2
    }

    /// `D3D12_FEATURE_DATA_MULTISAMPLE_QUALITY_LEVELS`.
    pub struct D3D12_FEATURE_DATA_MULTISAMPLE_QUALITY_LEVELS {
        Format:           u32
        SampleCount:      u32
        Flags:            u32
        NumQualityLevels: u32
    }

    // ── Subresource data (used by UpdateSubresources helper) ─────

    /// `D3D12_SUBRESOURCE_DATA` — CPU-side init data for a subresource.
    pub struct D3D12_SUBRESOURCE_DATA {
        pData:      *const void
        RowPitch:   i64
        SlicePitch: i64
    }

    // ── Enhanced barriers (Agility SDK / DirectX 12 Ultimate) ────

    /// `D3D12_GLOBAL_BARRIER` — coarse memory barrier covering every
    /// resource of compatible access bits.
    pub struct D3D12_GLOBAL_BARRIER {
        SyncBefore:   u32   // D3D12BarrierSync
        SyncAfter:    u32
        AccessBefore: u32   // D3D12BarrierAccess
        AccessAfter:  u32
    }

    /// `D3D12_BUFFER_BARRIER`.
    pub struct D3D12_BUFFER_BARRIER {
        SyncBefore:   u32
        SyncAfter:    u32
        AccessBefore: u32
        AccessAfter:  u32
        pResource:    *ID3D12Resource
        Offset:       u64
        Size:         u64
    }

    /// `D3D12_BARRIER_SUBRESOURCE_RANGE` — used inside texture barriers.
    pub struct D3D12_BARRIER_SUBRESOURCE_RANGE {
        IndexOrFirstMipLevel: u32
        NumMipLevels:         u32
        FirstArraySlice:      u32
        NumArraySlices:       u32
        FirstPlane:           u32
        NumPlanes:            u32
    }

    /// `D3D12_TEXTURE_BARRIER`.
    pub struct D3D12_TEXTURE_BARRIER {
        SyncBefore:    u32
        SyncAfter:     u32
        AccessBefore:  u32
        AccessAfter:   u32
        LayoutBefore:  u32   // D3D12BarrierLayout
        LayoutAfter:   u32
        pResource:     *ID3D12Resource
        Subresources:  D3D12_BARRIER_SUBRESOURCE_RANGE
        Flags:         u32   // D3D12TextureBarrierFlags
        _pad0:         u32
    }

    /// `D3D12_BARRIER_GROUP` — pass an array of these to
    /// `ID3D12GraphicsCommandList7::Barrier`. `Type` discriminates
    /// the union; only one of `pGlobalBarriers` / `pTextureBarriers`
    /// / `pBufferBarriers` is meaningful per group.
    pub struct D3D12_BARRIER_GROUP {
        Type:          u32   // D3D12BarrierType
        NumBarriers:   u32
        // union { GLOBAL_BARRIER*; TEXTURE_BARRIER*; BUFFER_BARRIER* }
        pBarriers:     *const void
    }

    // ── Render passes (List4) ────────────────────────────────────

    /// `D3D12_RENDER_PASS_BEGINNING_ACCESS_CLEAR_PARAMETERS`.
    pub struct D3D12_RENDER_PASS_BEGINNING_ACCESS_CLEAR_PARAMETERS {
        ClearValue: D3D12_CLEAR_VALUE
    }

    /// `D3D12_RENDER_PASS_BEGINNING_ACCESS_PRESERVE_LOCAL_PARAMETERS`
    /// — TBR-style preserve to local memory.
    pub struct D3D12_RENDER_PASS_BEGINNING_ACCESS_PRESERVE_LOCAL_PARAMETERS {
        AdditionalWidth:  u32
        AdditionalHeight: u32
    }

    /// `D3D12_RENDER_PASS_BEGINNING_ACCESS` — tagged union over
    /// CLEAR / PRESERVE_LOCAL parameter blocks. `Clear` variant is
    /// 24 bytes (D3D12_CLEAR_VALUE) — laid out flat so the call site
    /// can poke either form.
    pub struct D3D12_RENDER_PASS_BEGINNING_ACCESS {
        Type:    u32   // D3D12RenderPassBeginningAccessType
        _pad0:   u32
        u0:      u64
        u1:      u64
        u2:      u64
    }

    /// `D3D12_RENDER_PASS_ENDING_ACCESS_RESOLVE_SUBRESOURCE_PARAMETERS`.
    pub struct D3D12_RENDER_PASS_ENDING_ACCESS_RESOLVE_SUBRESOURCE_PARAMETERS {
        SrcSubresource:    u32
        DstSubresource:    u32
        DstX:              u32
        DstY:              u32
        SrcRect:           D3D12_RECT
    }

    /// `D3D12_RENDER_PASS_ENDING_ACCESS_RESOLVE_PARAMETERS`.
    pub struct D3D12_RENDER_PASS_ENDING_ACCESS_RESOLVE_PARAMETERS {
        pSrcResource:               *ID3D12Resource
        pDstResource:               *ID3D12Resource
        SubresourceCount:           u32
        _pad0:                      u32
        pSubresourceParameters:     *const D3D12_RENDER_PASS_ENDING_ACCESS_RESOLVE_SUBRESOURCE_PARAMETERS
        Format:                     u32
        ResolveMode:                u32   // D3D12ResolveMode
        PreserveResolveSource:      i32
        _pad1:                      u32
    }

    /// `D3D12_RENDER_PASS_ENDING_ACCESS` — tagged union over the
    /// RESOLVE / PRESERVE_LOCAL parameter blocks. Resolve variant is
    /// the larger; flatten as 6 × u64.
    pub struct D3D12_RENDER_PASS_ENDING_ACCESS {
        Type:    u32   // D3D12RenderPassEndingAccessType
        _pad0:   u32
        u0:      u64
        u1:      u64
        u2:      u64
        u3:      u64
        u4:      u64
        u5:      u64
    }

    /// `D3D12_RENDER_PASS_RENDER_TARGET_DESC`.
    pub struct D3D12_RENDER_PASS_RENDER_TARGET_DESC {
        cpuDescriptorPtr:  u64                                 // D3D12_CPU_DESCRIPTOR_HANDLE by value
        BeginningAccess:   D3D12_RENDER_PASS_BEGINNING_ACCESS
        EndingAccess:      D3D12_RENDER_PASS_ENDING_ACCESS
    }

    /// `D3D12_RENDER_PASS_DEPTH_STENCIL_DESC`.
    pub struct D3D12_RENDER_PASS_DEPTH_STENCIL_DESC {
        cpuDescriptorPtr:         u64
        DepthBeginningAccess:     D3D12_RENDER_PASS_BEGINNING_ACCESS
        StencilBeginningAccess:   D3D12_RENDER_PASS_BEGINNING_ACCESS
        DepthEndingAccess:        D3D12_RENDER_PASS_ENDING_ACCESS
        StencilEndingAccess:      D3D12_RENDER_PASS_ENDING_ACCESS
    }

    // ── Indirect drawing / command signatures ────────────────────

    /// `D3D12_INDIRECT_ARGUMENT_DESC` — one entry in a command
    /// signature. `Type` is the discriminator; only the matching
    /// payload fields are read. CONSTANT / VERTEX_BUFFER_VIEW /
    /// INDEX_BUFFER_VIEW / ROOT_*_VIEW carry small integer payloads
    /// flattened into `u0` / `u1`.
    pub struct D3D12_INDIRECT_ARGUMENT_DESC {
        Type:   u32   // D3D12IndirectArgumentType
        _pad0:  u32
        u0:     u64
        u1:     u64
    }

    /// `D3D12_COMMAND_SIGNATURE_DESC`.
    pub struct D3D12_COMMAND_SIGNATURE_DESC {
        ByteStride:        u32
        NumArgumentDescs:  u32
        pArgumentDescs:    *const D3D12_INDIRECT_ARGUMENT_DESC
        NodeMask:          u32
        _pad0:             u32
    }

    /// Built-in indirect-arg shapes — sizes for ByteStride pickers.
    pub struct D3D12_DRAW_ARGUMENTS {
        VertexCountPerInstance: u32
        InstanceCount:          u32
        StartVertexLocation:    u32
        StartInstanceLocation:  u32
    }

    pub struct D3D12_DRAW_INDEXED_ARGUMENTS {
        IndexCountPerInstance: u32
        InstanceCount:         u32
        StartIndexLocation:    u32
        BaseVertexLocation:    i32
        StartInstanceLocation: u32
    }

    pub struct D3D12_DISPATCH_ARGUMENTS {
        ThreadGroupCountX: u32
        ThreadGroupCountY: u32
        ThreadGroupCountZ: u32
    }

    // ── State object subobjects (DXR / collection support) ───────

    /// `D3D12_STATE_SUBOBJECT` — one row of a `STATE_OBJECT_DESC`.
    /// `pDesc` points at the per-`Type` payload struct.
    pub struct D3D12_STATE_SUBOBJECT {
        Type:  u32   // D3D12StateSubobjectType
        _pad0: u32
        pDesc: *const void
    }

    /// `D3D12_STATE_OBJECT_DESC`.
    pub struct D3D12_STATE_OBJECT_DESC {
        Type:         u32   // D3D12StateObjectType
        NumSubobjects: u32
        pSubobjects:  *const D3D12_STATE_SUBOBJECT
    }

    /// `D3D12_EXPORT_DESC` — name / rename / flags for a DXIL export.
    pub struct D3D12_EXPORT_DESC {
        Name:           *const u16
        ExportToRename: *const u16
        Flags:          u32   // D3D12ExportFlags
        _pad0:          u32
    }

    /// `D3D12_DXIL_LIBRARY_DESC`.
    pub struct D3D12_DXIL_LIBRARY_DESC {
        DXILLibrary:   D3D12_SHADER_BYTECODE
        NumExports:    u32
        _pad0:         u32
        pExports:      *D3D12_EXPORT_DESC
    }

    /// `D3D12_EXISTING_COLLECTION_DESC`.
    pub struct D3D12_EXISTING_COLLECTION_DESC {
        pExistingCollection: *ID3D12StateObject
        NumExports:          u32
        _pad0:               u32
        pExports:            *D3D12_EXPORT_DESC
    }

    /// `D3D12_DXIL_SUBOBJECT_TO_EXPORTS_ASSOCIATION`.
    pub struct D3D12_DXIL_SUBOBJECT_TO_EXPORTS_ASSOCIATION {
        SubobjectToAssociate: *const u16
        NumExports:           u32
        _pad0:                u32
        pExports:             *const *const u16
    }

    /// `D3D12_SUBOBJECT_TO_EXPORTS_ASSOCIATION` — non-DXIL variant.
    pub struct D3D12_SUBOBJECT_TO_EXPORTS_ASSOCIATION {
        pSubobjectToAssociate: *const D3D12_STATE_SUBOBJECT
        NumExports:            u32
        _pad0:                 u32
        pExports:              *const *const u16
    }

    /// `D3D12_HIT_GROUP_DESC` — bundles a hit-group's intersection /
    /// any-hit / closest-hit shaders.
    pub struct D3D12_HIT_GROUP_DESC {
        HitGroupExport:                *const u16
        Type:                          u32   // D3D12HitGroupType
        _pad0:                         u32
        AnyHitShaderImport:            *const u16
        ClosestHitShaderImport:        *const u16
        IntersectionShaderImport:      *const u16
    }

    /// `D3D12_RAYTRACING_SHADER_CONFIG`.
    pub struct D3D12_RAYTRACING_SHADER_CONFIG {
        MaxPayloadSizeInBytes:   u32
        MaxAttributeSizeInBytes: u32
    }

    /// `D3D12_RAYTRACING_PIPELINE_CONFIG`.
    pub struct D3D12_RAYTRACING_PIPELINE_CONFIG {
        MaxTraceRecursionDepth: u32
    }

    /// `D3D12_RAYTRACING_PIPELINE_CONFIG1` — adds Flags.
    pub struct D3D12_RAYTRACING_PIPELINE_CONFIG1 {
        MaxTraceRecursionDepth: u32
        Flags:                  u32   // D3D12RaytracingPipelineFlags
    }

    /// `D3D12_LOCAL_ROOT_SIGNATURE` / `_GLOBAL_ROOT_SIGNATURE` — both
    /// wrap an `ID3D12RootSignature*`. Use the same struct for either
    /// subobject `Type`; the discriminator is the subobject's Type
    /// field, not a tag on the payload.
    pub struct D3D12_ROOT_SIGNATURE_REFERENCE {
        pRootSignature: *ID3D12RootSignature
    }

    /// `D3D12_NODE_MASK` — used as a `STATE_SUBOBJECT_TYPE_NODE_MASK`
    /// payload.
    pub struct D3D12_NODE_MASK {
        NodeMask: u32
    }

    // ── Protected sessions (Agility SDK) ─────────────────────────

    /// `D3D12_PROTECTED_RESOURCE_SESSION_DESC`.
    pub struct D3D12_PROTECTED_RESOURCE_SESSION_DESC {
        NodeMask: u32
        Flags:    u32   // D3D12ProtectedResourceSessionFlags
    }

    // ── Sample position (List1) ──────────────────────────────────

    /// `D3D12_SAMPLE_POSITION` — programmable sample patterns.
    pub struct D3D12_SAMPLE_POSITION {
        X: i8
        Y: i8
    }

    // ── DRED (Device-Removed Extended Data) ──────────────────────

    /// `D3D12_AUTO_BREADCRUMB_NODE` (singly-linked list) — surfaced
    /// for completeness; consumers walk it via raw pointer arith.
    pub struct D3D12_AUTO_BREADCRUMB_NODE {
        pCommandListDebugNameA:      *const char
        pCommandListDebugNameW:      *const u16
        pCommandQueueDebugNameA:     *const char
        pCommandQueueDebugNameW:     *const u16
        pCommandList:                *void   // ID3D12GraphicsCommandList*
        pCommandQueue:               *void   // ID3D12CommandQueue*
        BreadcrumbCount:             u32
        _pad0:                       u32
        pLastBreadcrumbValue:        *const u32
        pCommandHistory:             *const u32   // D3D12_AUTO_BREADCRUMB_OP[]
        pNext:                       *const D3D12_AUTO_BREADCRUMB_NODE
    }

    /// `D3D12_DEVICE_REMOVED_EXTENDED_DATA` (DRED) — top-level
    /// payload for `ID3D12DeviceRemovedExtendedData::GetAutoBreadcrumbsOutput`.
    pub struct D3D12_DEVICE_REMOVED_EXTENDED_DATA {
        Flags:                      u32   // D3D12DredFlags
        _pad0:                      u32
        pHeadAutoBreadcrumbNode:    *const D3D12_AUTO_BREADCRUMB_NODE
    }

    /// `D3D12_DRED_ALLOCATION_NODE` — singly-linked.
    pub struct D3D12_DRED_ALLOCATION_NODE {
        ObjectNameA:    *const char
        ObjectNameW:    *const u16
        AllocationType: u32   // D3D12DredAllocationType
        _pad0:          u32
        pNext:          *const D3D12_DRED_ALLOCATION_NODE
    }

    /// `D3D12_DRED_PAGE_FAULT_OUTPUT`.
    pub struct D3D12_DRED_PAGE_FAULT_OUTPUT {
        PageFaultVA:                       u64
        pHeadExistingAllocationNode:       *const D3D12_DRED_ALLOCATION_NODE
        pHeadRecentFreedAllocationNode:    *const D3D12_DRED_ALLOCATION_NODE
    }

    // ── DirectX Raytracing (DXR) ─────────────────────────────────

    /// `D3D12_GPU_VIRTUAL_ADDRESS_AND_STRIDE` — used for AS build /
    /// shader table inputs.
    pub struct D3D12_GPU_VIRTUAL_ADDRESS_AND_STRIDE {
        StartAddress:  u64
        StrideInBytes: u64
    }

    /// `D3D12_GPU_VIRTUAL_ADDRESS_RANGE`.
    pub struct D3D12_GPU_VIRTUAL_ADDRESS_RANGE {
        StartAddress:  u64
        SizeInBytes:   u64
    }

    /// `D3D12_GPU_VIRTUAL_ADDRESS_RANGE_AND_STRIDE`.
    pub struct D3D12_GPU_VIRTUAL_ADDRESS_RANGE_AND_STRIDE {
        StartAddress:  u64
        SizeInBytes:   u64
        StrideInBytes: u64
    }

    /// `D3D12_RAYTRACING_AABB`.
    pub struct D3D12_RAYTRACING_AABB {
        MinX: f32
        MinY: f32
        MinZ: f32
        MaxX: f32
        MaxY: f32
        MaxZ: f32
    }

    /// `D3D12_RAYTRACING_GEOMETRY_TRIANGLES_DESC`.
    pub struct D3D12_RAYTRACING_GEOMETRY_TRIANGLES_DESC {
        Transform3x4:           u64    // GPU VA of 3x4 row-major matrix
        IndexFormat:            u32
        VertexFormat:           u32
        IndexCount:             u32
        VertexCount:            u32
        IndexBuffer:            u64
        VertexBuffer:           D3D12_GPU_VIRTUAL_ADDRESS_AND_STRIDE
    }

    /// `D3D12_RAYTRACING_GEOMETRY_AABBS_DESC`.
    pub struct D3D12_RAYTRACING_GEOMETRY_AABBS_DESC {
        AABBCount: u64
        AABBs:     D3D12_GPU_VIRTUAL_ADDRESS_AND_STRIDE
    }

    /// `D3D12_RAYTRACING_GEOMETRY_DESC` — tagged union over triangles /
    /// aabbs. Triangles variant is the larger; flatten as 7 × u64
    /// after the (Type, Flags, pad) header.
    pub struct D3D12_RAYTRACING_GEOMETRY_DESC {
        Type:    u32   // D3D12RaytracingGeometryType
        Flags:   u32   // D3D12RaytracingGeometryFlags
        u0:      u64
        u1:      u64
        u2:      u64
        u3:      u64
        u4:      u64
        u5:      u64
        u6:      u64
    }

    /// `D3D12_RAYTRACING_INSTANCE_DESC` — TLAS instance entry.
    /// `_pack0..23` encodes the 4×3 transform, instance id (24-bit),
    /// instance mask (8-bit), instance contribution to hit-group index
    /// (24-bit), flags (8-bit). See the SDK header for bit layout.
    pub struct D3D12_RAYTRACING_INSTANCE_DESC {
        Transform0_0: f32
        Transform0_1: f32
        Transform0_2: f32
        Transform0_3: f32
        Transform1_0: f32
        Transform1_1: f32
        Transform1_2: f32
        Transform1_3: f32
        Transform2_0: f32
        Transform2_1: f32
        Transform2_2: f32
        Transform2_3: f32
        InstanceIDAndMask:               u32   // 24-bit ID | 8-bit mask
        InstanceContributionAndFlags:    u32   // 24-bit contribution | 8-bit flags
        AccelerationStructure: u64
    }

    /// `D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_INPUTS`.
    pub struct D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_INPUTS {
        Type:          u32   // D3D12RaytracingAccelerationStructureType
        Flags:         u32   // D3D12RaytracingAccelerationStructureBuildFlags
        NumDescs:      u32
        DescsLayout:   u32   // D3D12ElementsLayout
        // union { InstanceDescs: u64; pGeometryDescs: *...; ppGeometryDescs: ** }
        u0:            u64
    }

    /// `D3D12_RAYTRACING_ACCELERATION_STRUCTURE_PREBUILD_INFO`.
    pub struct D3D12_RAYTRACING_ACCELERATION_STRUCTURE_PREBUILD_INFO {
        ResultDataMaxSizeInBytes:  u64
        ScratchDataSizeInBytes:    u64
        UpdateScratchDataSizeInBytes: u64
    }

    /// `D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_DESC` —
    /// the full argument bundle passed to BuildRaytracingAccelerationStructure.
    pub struct D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_DESC {
        DestAccelerationStructureData:   u64
        Inputs:                          D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_INPUTS
        SourceAccelerationStructureData: u64
        ScratchAccelerationStructureData: u64
    }

    /// `D3D12_RAYTRACING_ACCELERATION_STRUCTURE_POSTBUILD_INFO_DESC`.
    pub struct D3D12_RAYTRACING_ACCELERATION_STRUCTURE_POSTBUILD_INFO_DESC {
        DestBuffer: u64
        InfoType:   u32   // D3D12RaytracingAccelerationStructurePostbuildInfoType
        _pad0:      u32
    }

    /// `D3D12_DISPATCH_RAYS_DESC` — argument bundle for
    /// `DispatchRays` (and the SDF tables for shader binding).
    pub struct D3D12_DISPATCH_RAYS_DESC {
        RayGenerationShaderRecord: D3D12_GPU_VIRTUAL_ADDRESS_RANGE
        MissShaderTable:           D3D12_GPU_VIRTUAL_ADDRESS_RANGE_AND_STRIDE
        HitGroupTable:             D3D12_GPU_VIRTUAL_ADDRESS_RANGE_AND_STRIDE
        CallableShaderTable:       D3D12_GPU_VIRTUAL_ADDRESS_RANGE_AND_STRIDE
        Width:   u32
        Height:  u32
        Depth:   u32
        _pad0:   u32
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

// ─── Root signature / descriptor table enums ─────────────────────

/// D3D12_ROOT_PARAMETER_TYPE — discriminator on `D3D12_ROOT_PARAMETER`.
pub enum D3D12RootParameterType: u32 {
    descriptorTable = 0
    constants32Bit  = 1
    cbv             = 2
    srv             = 3
    uav             = 4
}

/// D3D12_DESCRIPTOR_RANGE_TYPE — flavour of a descriptor table range.
pub enum D3D12DescriptorRangeType: u32 {
    srv     = 0
    uav     = 1
    cbv     = 2
    sampler = 3
}

/// D3D12_SHADER_VISIBILITY — which stage a root entry is visible to.
pub enum D3D12ShaderVisibility: u32 {
    all          = 0
    vertex       = 1
    hull         = 2
    domain       = 3
    geometry     = 4
    pixel        = 5
    amplification = 6
    mesh         = 7
}

/// D3D12_DESCRIPTOR_RANGE_FLAGS (1.1 root signatures).
@flags
pub enum D3D12DescriptorRangeFlags: u32 {
    none                              = 0
    descriptorsVolatile               = 0x1
    dataVolatile                      = 0x2
    dataStaticWhileSetAtExecute       = 0x4
    dataStatic                        = 0x8
    descriptorsStaticKeepingBufferBounds = 0x10000
}

/// D3D12_ROOT_DESCRIPTOR_FLAGS (1.1 root signatures).
@flags
pub enum D3D12RootDescriptorFlags: u32 {
    none                              = 0
    dataVolatile                      = 0x2
    dataStaticWhileSetAtExecute       = 0x4
    dataStatic                        = 0x8
}

/// D3D12_STATIC_BORDER_COLOR — fixed border palette for static samplers.
pub enum D3D12StaticBorderColor: u32 {
    transparentBlack = 0
    opaqueBlack      = 1
    opaqueWhite      = 2
}

// ─── Query / counter enums ───────────────────────────────────────

pub enum D3D12QueryHeapType: u32 {
    occlusion          = 0
    timestamp          = 1
    pipelineStatistics = 2
    soStatistics       = 3
    videoDecodeStatistics = 4
    copyQueueTimestamp = 5
}

pub enum D3D12QueryType: u32 {
    occlusion          = 0
    binaryOcclusion    = 1
    timestamp          = 2
    pipelineStatistics = 3
    soStatisticsStream0 = 4
    soStatisticsStream1 = 5
    soStatisticsStream2 = 6
    soStatisticsStream3 = 7
    videoDecodeStatistics = 8
}

// ─── View dimension enums (RTV / DSV / SRV / UAV) ────────────────

pub enum D3D12RtvDimension: u32 {
    unknown      = 0
    buffer       = 1
    texture1d    = 2
    texture1dArray = 3
    texture2d    = 4
    texture2dArray = 5
    texture2dMs  = 6
    texture2dMsArray = 7
    texture3d    = 8
}

pub enum D3D12DsvDimension: u32 {
    unknown      = 0
    texture1d    = 1
    texture1dArray = 2
    texture2d    = 3
    texture2dArray = 4
    texture2dMs  = 5
    texture2dMsArray = 6
}

pub enum D3D12SrvDimension: u32 {
    unknown      = 0
    buffer       = 1
    texture1d    = 2
    texture1dArray = 3
    texture2d    = 4
    texture2dArray = 5
    texture2dMs  = 6
    texture2dMsArray = 7
    texture3d    = 8
    textureCube  = 9
    textureCubeArray = 10
    raytracingAccelerationStructure = 11
}

pub enum D3D12UavDimension: u32 {
    unknown      = 0
    buffer       = 1
    texture1d    = 2
    texture1dArray = 3
    texture2d    = 4
    texture2dArray = 5
    texture3d    = 8
}

/// D3D12_DSV_FLAGS — read-only depth / stencil on a DSV.
@flags
pub enum D3D12DsvFlags: u32 {
    none                = 0
    readOnlyDepth       = 1
    readOnlyStencil     = 2
}

/// D3D12_BUFFER_SRV_FLAGS / UAV_FLAGS — raw byte buffer marker.
@flags
pub enum D3D12BufferSrvFlags: u32 {
    none = 0
    raw  = 1
}

@flags
pub enum D3D12BufferUavFlags: u32 {
    none = 0
    raw  = 1
}

// ─── Texture copy / footprint enums ──────────────────────────────

pub enum D3D12TextureCopyType: u32 {
    subresourceIndex = 0
    placedFootprint  = 1
}

// ─── Feature queries ─────────────────────────────────────────────

/// D3D12_FEATURE — the discriminator for `ID3D12Device::CheckFeatureSupport`.
pub enum D3D12Feature: u32 {
    d3d12Options          = 0
    architecture          = 1
    featureLevels         = 2
    formatSupport         = 3
    multisampleQualityLevels = 4
    formatInfo            = 5
    gpuVirtualAddressSupport = 6
    shaderModel           = 7
    d3d12Options1         = 8
    rootSignature         = 12
    architecture1         = 16
    d3d12Options2         = 18
    shaderCache           = 19
    commandQueuePriority  = 20
    d3d12Options3         = 21
    existingHeaps         = 22
    d3d12Options4         = 23
    serialization         = 24
    crossNode             = 25
    d3d12Options5         = 27  // ray tracing tier lives here
    displayable           = 28
    d3d12Options6         = 30
    queryMetaCommand      = 31
    d3d12Options7         = 32
    protectedResourceSession = 33
    d3d12Options8         = 36
    d3d12Options9         = 37
    d3d12Options10        = 39
    d3d12Options11        = 40
    d3d12Options12        = 41
    d3d12Options13        = 42
}

/// D3D12_RAYTRACING_TIER — value of `RaytracingTier` in OPTIONS5.
pub enum D3D12RaytracingTier: u32 {
    notSupported = 0
    tier_1_0     = 10
    tier_1_1     = 11
}

/// D3D12_VARIABLE_SHADING_RATE_TIER — value in OPTIONS6.
pub enum D3D12VariableShadingRateTier: u32 {
    notSupported = 0
    tier_1       = 1
    tier_2       = 2
}

/// D3D12_MESH_SHADER_TIER — value in OPTIONS7.
pub enum D3D12MeshShaderTier: u32 {
    notSupported = 0
    tier_1       = 10
}

// ─── State object / raytracing pipeline (minimal enums) ──────────

pub enum D3D12StateObjectType: u32 {
    collection           = 0
    raytracingPipeline   = 3
    executableExportEmpty = 4
}

@flags
pub enum D3D12RaytracingPipelineFlags: u32 {
    none                  = 0
    skipTriangles         = 0x100
    skipProceduralPrimitives = 0x200
}

/// Default 4-component identity mapping used in SRVs (shader 4
/// components map 1:1 to texture channels).
/// `D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING` = 0x1688.
pub const D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING: u32 = 0x1688

/// `D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES` = 0xFFFFFFFF.
pub const D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES: u32 = 0xFFFFFFFF

/// `D3D12_APPEND_ALIGNED_ELEMENT` sentinel used in input layouts.
pub const D3D12_APPEND_ALIGNED_ELEMENT: u32 = 0xFFFFFFFF

// ─── Enhanced barrier enums (List7 / Agility SDK) ────────────────

/// D3D12_BARRIER_LAYOUT — texture layout tracked by enhanced barriers.
pub enum D3D12BarrierLayout: u32 {
    undefined           = 0xFFFFFFFF
    common              = 0
    present             = 0
    genericRead         = 1
    renderTarget        = 2
    unorderedAccess     = 3
    depthStencilWrite   = 4
    depthStencilRead    = 5
    shaderResource      = 6
    copySource          = 7
    copyDest            = 8
    resolveSource       = 9
    resolveDest         = 10
    shadingRateSource   = 11
    videoDecodeRead     = 12
    videoDecodeWrite    = 13
    videoProcessRead    = 14
    videoProcessWrite   = 15
    videoEncodeRead     = 16
    videoEncodeWrite    = 17
    directQueueCommon   = 18
    directQueueGenericRead     = 19
    directQueueUnorderedAccess = 20
    directQueueShaderResource  = 21
    directQueueCopySource      = 22
    directQueueCopyDest        = 23
    computeQueueCommon         = 24
    computeQueueGenericRead    = 25
    computeQueueUnorderedAccess = 26
    computeQueueShaderResource = 27
    computeQueueCopySource     = 28
    computeQueueCopyDest       = 29
    videoQueueCommon           = 30
}

/// D3D12_BARRIER_SYNC — pipeline-stage scope of an enhanced barrier.
@flags
pub enum D3D12BarrierSync: u32 {
    none                              = 0x0
    all                               = 0x1
    draw                              = 0x2
    indexInput                        = 0x4
    vertexShading                     = 0x8
    pixelShading                      = 0x10
    depthStencil                      = 0x20
    renderTarget                      = 0x40
    computeShading                    = 0x80
    raytracing                        = 0x100
    copy                              = 0x200
    resolve                           = 0x400
    executeIndirect                   = 0x800
    predication                       = 0x800
    allShading                        = 0x1000
    nonPixelShading                   = 0x2000
    emitRaytracingAccelerationStructurePostbuildInfo = 0x4000
    clearUnorderedAccessView          = 0x8000
    videoDecode                       = 0x100000
    videoProcess                      = 0x200000
    videoEncode                       = 0x400000
    buildRaytracingAccelerationStructure = 0x800000
    copyRaytracingAccelerationStructure  = 0x1000000
    splitBegin                        = 0x80000000
}

/// D3D12_BARRIER_ACCESS — memory access scope of an enhanced barrier.
@flags
pub enum D3D12BarrierAccess: u32 {
    common                                = 0x0
    vertexBuffer                          = 0x1
    constantBuffer                        = 0x2
    indexBuffer                           = 0x4
    renderTarget                          = 0x8
    unorderedAccess                       = 0x10
    depthStencilWrite                     = 0x20
    depthStencilRead                      = 0x40
    shaderResource                        = 0x80
    streamOutput                          = 0x100
    indirectArgument                      = 0x200
    predication                           = 0x200
    copyDest                              = 0x400
    copySource                            = 0x800
    resolveDest                           = 0x1000
    resolveSource                         = 0x2000
    raytracingAccelerationStructureRead   = 0x4000
    raytracingAccelerationStructureWrite  = 0x8000
    shadingRateSource                     = 0x10000
    videoDecodeRead                       = 0x20000
    videoDecodeWrite                      = 0x40000
    videoProcessRead                      = 0x80000
    videoProcessWrite                     = 0x100000
    videoEncodeRead                       = 0x200000
    videoEncodeWrite                      = 0x400000
    noAccess                              = 0x80000000
}

/// D3D12_BARRIER_TYPE — which variant of `D3D12_BARRIER_GROUP`.
pub enum D3D12BarrierType: u32 {
    global  = 0
    texture = 1
    buffer  = 2
}

/// D3D12_TEXTURE_BARRIER_FLAGS.
@flags
pub enum D3D12TextureBarrierFlags: u32 {
    none    = 0
    discard = 1
}

// ─── Render-pass enums (List4 / Agility SDK) ─────────────────────

/// D3D12_RENDER_PASS_BEGINNING_ACCESS_TYPE.
pub enum D3D12RenderPassBeginningAccessType: u32 {
    discard             = 0
    preserve            = 1
    clear               = 2
    noAccess            = 3
    preserveLocalRender = 4
    preserveLocalSrv    = 5
    preserveLocalUav    = 6
}

/// D3D12_RENDER_PASS_ENDING_ACCESS_TYPE.
pub enum D3D12RenderPassEndingAccessType: u32 {
    discard             = 0
    preserve            = 1
    resolve             = 2
    noAccess            = 3
    preserveLocalRender = 4
    preserveLocalSrv    = 5
    preserveLocalUav    = 6
}

@flags
pub enum D3D12RenderPassFlags: u32 {
    none                = 0
    allowUavWrites      = 1
    suspendingPass      = 2
    resumingPass        = 4
    bindReadOnlyDepth   = 8
    bindReadOnlyStencil = 16
}

/// D3D12_RESOLVE_MODE.
pub enum D3D12ResolveMode: u32 {
    decompress             = 0
    min                    = 1
    max                    = 2
    average                = 3
    encodeSamplerFeedback  = 4
    decodeSamplerFeedback  = 5
}

// ─── Indirect-arg / command-signature enums ──────────────────────

pub enum D3D12IndirectArgumentType: u32 {
    draw                          = 0
    drawIndexed                   = 1
    dispatch                      = 2
    vertexBufferView              = 3
    indexBufferView               = 4
    constant                      = 5
    constantBufferView            = 6
    shaderResourceView            = 7
    unorderedAccessView           = 8
    dispatchRays                  = 9
    dispatchMesh                  = 10
}

// ─── State-object subobject enums ────────────────────────────────

pub enum D3D12StateSubobjectType: u32 {
    stateObjectConfig                   = 0
    globalRootSignature                 = 1
    localRootSignature                  = 2
    nodeMask                            = 3
    dxilLibrary                         = 5
    existingCollection                  = 6
    subobjectToExportsAssociation       = 7
    dxilSubobjectToExportsAssociation   = 8
    raytracingShaderConfig              = 9
    raytracingPipelineConfig            = 10
    hitGroup                            = 11
    raytracingPipelineConfig1           = 12
    workGraph                           = 13
    streamOutput                        = 14
    blend                               = 15
    sampleMask                          = 16
    rasterizer                          = 17
    depthStencil                        = 18
    inputLayout                         = 19
    indexBufferStripCutValue            = 20
    primitiveTopology                   = 21
    renderTargetFormats                 = 22
    depthStencilFormat                  = 23
    sampleDesc                          = 24
    flags                               = 25
    cachedPSO                           = 26
    viewInstancing                      = 27
}

@flags
pub enum D3D12ExportFlags: u32 {
    none = 0
}

pub enum D3D12HitGroupType: u32 {
    triangles         = 0
    proceduralPrimitive = 1
}

// ─── PSO subobject stream enums ──────────────────────────────────

pub enum D3D12PipelineStateSubobjectType: u32 {
    rootSignature        = 0
    vs                   = 1
    ps                   = 2
    ds                   = 3
    hs                   = 4
    gs                   = 5
    cs                   = 6
    streamOutput         = 7
    blend                = 8
    sampleMask           = 9
    rasterizer           = 10
    depthStencil         = 11
    inputLayout          = 12
    ibStripCutValue      = 13
    primitiveTopology    = 14
    renderTargetFormats  = 15
    depthStencilFormat   = 16
    sampleDesc           = 17
    nodeMask             = 18
    cachedPSO            = 19
    flags                = 20
    depthStencil1        = 21
    viewInstancing       = 22
    as_                  = 24
    ms                   = 25
}

// ─── Tier / option enums ─────────────────────────────────────────

pub enum D3D12ResourceBindingTier: u32 {
    tier_1 = 1
    tier_2 = 2
    tier_3 = 3
}

pub enum D3D12TiledResourcesTier: u32 {
    notSupported = 0
    tier_1       = 1
    tier_2       = 2
    tier_3       = 3
    tier_4       = 4
}

pub enum D3D12ConservativeRasterizationTier: u32 {
    notSupported = 0
    tier_1       = 1
    tier_2       = 2
    tier_3       = 3
}

pub enum D3D12CrossNodeSharingTier: u32 {
    notSupported     = 0
    tier_1Emulated   = 1
    tier_1           = 2
    tier_2           = 3
    tier_3           = 4
}

pub enum D3D12ResourceHeapTier: u32 {
    tier_1 = 1
    tier_2 = 2
}

@flags
pub enum D3D12ShaderMinPrecisionSupport: u32 {
    none = 0
    bit10 = 1
    bit16 = 2
}

@flags
pub enum D3D12FormatSupport1: u32 {
    none                       = 0
    buffer                     = 0x1
    iaVertexBuffer             = 0x2
    iaIndexBuffer              = 0x4
    soBuffer                   = 0x8
    texture1d                  = 0x10
    texture2d                  = 0x20
    texture3d                  = 0x40
    textureCube                = 0x80
    shaderLoad                 = 0x100
    shaderSample               = 0x200
    shaderSampleComparison     = 0x400
    shaderSampleMonoText       = 0x800
    mip                        = 0x1000
    renderTarget               = 0x4000
    blendable                  = 0x8000
    depthStencil               = 0x10000
    cpuLockable                = 0x20000
    multisampleResolve         = 0x40000
    display                    = 0x80000
    castWithinBitLayout        = 0x100000
    multisampleRenderTarget    = 0x200000
    multisampleLoad            = 0x400000
    shaderGather               = 0x800000
    backBufferCast             = 0x1000000
    typedUnorderedAccessView   = 0x2000000
    shaderGatherComparison     = 0x4000000
    decoderOutput              = 0x8000000
    videoProcessorOutput       = 0x10000000
    videoProcessorInput        = 0x20000000
    videoEncoder               = 0x40000000
}

@flags
pub enum D3D12FormatSupport2: u32 {
    none                       = 0
    uavAtomicAdd               = 0x1
    uavAtomicBitwiseOps        = 0x2
    uavAtomicCompareStoreOrCompareExchange = 0x4
    uavAtomicExchange          = 0x8
    uavAtomicSignedMinOrMax    = 0x10
    uavAtomicUnsignedMinOrMax  = 0x20
    uavTypedLoad               = 0x40
    uavTypedStore              = 0x80
    outputMergerLogicOp        = 0x100
    tiled                      = 0x200
    multiplaneOverlay          = 0x4000
    samplerFeedback            = 0x8000
}

// ─── Protected session ───────────────────────────────────────────

@flags
pub enum D3D12ProtectedResourceSessionFlags: u32 {
    none                = 0
    protectedContent    = 1
}

pub enum D3D12ProtectedSessionStatus: u32 {
    ok        = 0
    invalid   = 1
}

// ─── Residency / lifetime ────────────────────────────────────────

pub enum D3D12ResidencyPriority: u32 {
    minimum = 0x28000000
    low     = 0x50000000
    normal  = 0x78000000
    high    = 0xa0010000
    maximum = 0xc8000000
}

pub enum D3D12LifetimeState: u32 {
    inUse       = 0
    notInUse    = 1
}

// ─── DRED (Device-Removed Extended Data) ─────────────────────────

@flags
pub enum D3D12DredFlags: u32 {
    none                       = 0
    forceEnable                = 1
    disableAutobreadcrumbs     = 2
    disablePageFaultReporting  = 4
    disableWatson              = 8
}

pub enum D3D12DredEnablement: u32 {
    systemControlled = 0
    forcedOff        = 1
    forcedOn         = 2
}

pub enum D3D12DredAllocationType: u32 {
    commandQueue       = 19
    commandAllocator   = 20
    pipelineState      = 21
    commandList        = 22
    fence              = 23
    descriptorHeap     = 24
    heap               = 25
    queryHeap          = 27
    commandSignature   = 28
    pipelineLibrary    = 29
    resource           = 34
    invalid            = 0xFFFFFFFF
}

@flags
pub enum D3D12AutoBreadcrumbOp: u32 {
    setMarker                            = 0
    beginEvent                           = 1
    endEvent                             = 2
    drawInstanced                        = 3
    drawIndexedInstanced                 = 4
    executeIndirect                      = 5
    dispatch                             = 6
    copyBufferRegion                     = 7
    copyTextureRegion                    = 8
    copyResource                         = 9
    copyTiles                            = 10
    resolveSubresource                   = 11
    clearRenderTargetView                = 12
    clearUnorderedAccessView             = 13
    clearDepthStencilView                = 14
    resourceBarrier                      = 15
    executeBundle                        = 16
    present                              = 17
    resolveQueryData                     = 18
    beginSubmission                      = 19
    endSubmission                        = 20
    decodeFrame                          = 21
    processFrames                        = 22
    atomicCopyBufferUint                 = 23
    atomicCopyBufferUint64               = 24
    resolveSubresourceRegion             = 25
    writeBufferImmediate                 = 26
    dispatchRays                         = 27
    dispatchMesh                         = 30
}

// ─── Shader cache support ────────────────────────────────────────

@flags
pub enum D3D12ShaderCacheSupportFlags: u32 {
    none                              = 0
    singlePso                         = 1
    library                           = 2
    automaticInprocCache              = 4
    automaticDiskCache                = 8
    driverManagedCache                = 16
    shaderControlClear                = 32
    shaderSessionDelete               = 64
}

// ─── Clear flags (depth-stencil) ─────────────────────────────────

@flags
pub enum D3D12ClearFlags: u32 {
    none    = 0
    depth   = 1
    stencil = 2
}

// ─── Pipeline state flags 1 (Agility SDK) ────────────────────────

@flags
pub enum D3D12PipelineStateFlags1: u32 {
    none                       = 0
    toolDebug                  = 1
    dynamicDepthBias           = 4
    dynamicIndexBufferStripCut = 8
}

// ─── Shading rate (VRS, List5) ───────────────────────────────────

pub enum D3D12ShadingRate: u32 {
    rate_1x1   = 0
    rate_1x2   = 0x1
    rate_2x1   = 0x4
    rate_2x2   = 0x5
    rate_2x4   = 0x6
    rate_4x2   = 0x9
    rate_4x4   = 0xa
}

pub enum D3D12ShadingRateCombiner: u32 {
    passthrough = 0
    override    = 1
    min         = 2
    max         = 3
    sum         = 4
}

// ─── DirectX Raytracing enums ────────────────────────────────────

pub enum D3D12RaytracingGeometryType: u32 {
    triangles                = 0
    proceduralPrimitiveAabbs = 1
}

@flags
pub enum D3D12RaytracingGeometryFlags: u32 {
    none                                = 0
    opaque                              = 1
    noDuplicateAnyHitInvocation         = 2
}

pub enum D3D12RaytracingAccelerationStructureType: u32 {
    topLevel    = 0
    bottomLevel = 1
}

@flags
pub enum D3D12RaytracingAccelerationStructureBuildFlags: u32 {
    none            = 0
    allowUpdate     = 1
    allowCompaction = 2
    preferFastTrace = 4
    preferFastBuild = 8
    minimizeMemory  = 16
    performUpdate   = 32
}

pub enum D3D12RaytracingAccelerationStructureCopyMode: u32 {
    clone                              = 0
    compact                            = 1
    visualizationDecodeForTools        = 2
    serialize                          = 3
    deserialize                        = 4
}

pub enum D3D12RaytracingAccelerationStructurePostbuildInfoType: u32 {
    compactedSize       = 0
    toolsVisualization  = 1
    serialization       = 2
    currentSize         = 3
}

pub enum D3D12ElementsLayout: u32 {
    array            = 0
    arrayOfPointers  = 1
}

@flags
pub enum D3D12RaytracingInstanceFlags: u32 {
    none                            = 0
    triangleCullDisable             = 1
    triangleFrontCounterclockwise   = 2
    forceOpaque                     = 4
    forceNonOpaque                  = 8
}

/// Reasonable maximum for `MaxTraceRecursionDepth`.
pub const D3D12_RAYTRACING_MAX_DECLARABLE_TRACE_RECURSION_DEPTH: u32 = 31

/// Standard shader identifier size for AS/DispatchRays tables.
pub const D3D12_SHADER_IDENTIFIER_SIZE_IN_BYTES: u32 = 32

/// Required alignment of shader-record start addresses.
pub const D3D12_RAYTRACING_SHADER_RECORD_BYTE_ALIGNMENT: u32 = 32

/// Required alignment of shader-table starts.
pub const D3D12_RAYTRACING_SHADER_TABLE_BYTE_ALIGNMENT: u32 = 64
