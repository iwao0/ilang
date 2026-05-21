/// d3d11.dll — Direct3D 11 device creation + key constants.
///
/// Pipeline:
///   1. D3D11CreateDevice (or D3D11CreateDeviceAndSwapChain)
///   2. ID3D11Device / ID3D11DeviceContext via COM
///   3. CreateBuffer / CreateTexture2D / CreateShader… on the
///      device; OMSetRenderTargets / Draw / DrawIndexed on the
///      context.
///   4. SwapChain1.Present
///
/// For UWP / desktop hybrid: DXGI.SwapChain1 + composition swap
/// chain APIs live in dxgi.dll (already bound under directx12/).
///
/// The bulk of the D3D11 COM surface (ID3D11Device, ID3D11DeviceContext,
/// ID3D11Resource, ID3D11ShaderResourceView etc.) is too big to bind
/// in one pass and is deferred — this file gives you the function
/// entry points + the constants needed to feed them.

// ─── D3D_DRIVER_TYPE ─────────────────────────────────────────────
pub enum D3dDriverType: i32 {
    unknown   = 0
    hardware  = 1
    reference = 2
    null_     = 3
    software  = 4
    warp      = 5
}

// ─── D3D_FEATURE_LEVEL ───────────────────────────────────────────
pub enum D3dFeatureLevel: u32 {
    level_9_1  = 0x9100
    level_9_2  = 0x9200
    level_9_3  = 0x9300
    level_10_0 = 0xA000
    level_10_1 = 0xA100
    level_11_0 = 0xB000
    level_11_1 = 0xB100
    level_12_0 = 0xC000
    level_12_1 = 0xC100
    level_12_2 = 0xC200
}

// ─── D3D11_CREATE_DEVICE_FLAG ────────────────────────────────────
@flags
pub enum D3d11CreateDeviceFlags: u32 {
    none                = 0
    singleThreaded      = 0x1
    debug               = 0x2
    switchToRef         = 0x4
    preventInternalThreadingOptimizations = 0x8
    bgraSupport         = 0x20
    debuggable          = 0x40
    preventAlteringLayerSettingsFromRegistry = 0x80
    disableGpuTimeout   = 0x100
    videoSupport        = 0x800
}

// ─── D3D11_USAGE ─────────────────────────────────────────────────
pub enum D3d11Usage: i32 {
    default   = 0
    immutable = 1
    dynamic   = 2
    staging   = 3
}

// ─── D3D11_BIND_FLAG ─────────────────────────────────────────────
@flags
pub enum D3d11BindFlags: u32 {
    vertexBuffer    = 0x0001
    indexBuffer     = 0x0002
    constantBuffer  = 0x0004
    shaderResource  = 0x0008
    streamOutput    = 0x0010
    renderTarget    = 0x0020
    depthStencil    = 0x0040
    unorderedAccess = 0x0080
    decoder         = 0x0200
    videoEncoder    = 0x0400
}

// ─── D3D11_MAP ───────────────────────────────────────────────────
pub enum D3d11Map: i32 {
    read         = 1
    write        = 2
    readWrite    = 3
    writeDiscard = 4
    writeNoOverwrite = 5
}

// ─── D3D11_PRIMITIVE_TOPOLOGY ────────────────────────────────────
pub enum D3d11PrimitiveTopology: i32 {
    undefined     = 0
    pointList     = 1
    lineList      = 2
    lineStrip     = 3
    triangleList  = 4
    triangleStrip = 5
}

@extern(C, "d3d11") {
    /// D3D11_VIEWPORT.
    pub struct D3D11_VIEWPORT {
        TopLeftX: f32
        TopLeftY: f32
        Width:    f32
        Height:   f32
        MinDepth: f32
        MaxDepth: f32
    }

    /// D3D11_BOX — sub-resource region for Map / UpdateSubresource.
    pub struct D3D11_BOX {
        left:   u32
        top:    u32
        front:  u32
        right:  u32
        bottom: u32
        back:   u32
    }

    /// `D3D11CreateDevice` — request a D3D11 device with the named
    /// driver type. `ppFeatureLevel` may be NULL.
    @lib pub fn D3D11CreateDevice(
        pAdapter:         *void,                  // *IDXGIAdapter
        DriverType:       i32,                    // D3dDriverType
        Software:         i64,                    // HMODULE for software
        Flags:            u32,                    // D3d11CreateDeviceFlags
        pFeatureLevels:   *const u32,
        FeatureLevels:    u32,
        SDKVersion:       u32,                    // D3D11_SDK_VERSION = 7
        ppDevice:         **void,                 // *ID3D11Device
        pFeatureLevel:    *u32,
        ppImmediateContext: **void                // *ID3D11DeviceContext
    ): i32

    /// Same plus a DXGI swap chain in one call. `pSwapChainDesc`
    /// is a *DXGI_SWAP_CHAIN_DESC (legacy).
    @lib pub fn D3D11CreateDeviceAndSwapChain(
        pAdapter:         *void,
        DriverType:       i32,
        Software:         i64,
        Flags:            u32,
        pFeatureLevels:   *const u32,
        FeatureLevels:    u32,
        SDKVersion:       u32,
        pSwapChainDesc:   *const void,
        ppSwapChain:      **void,                 // *IDXGISwapChain
        ppDevice:         **void,
        pFeatureLevel:    *u32,
        ppImmediateContext: **void
    ): i32
}
