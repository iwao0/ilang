// dxgi.dll — DirectX Graphics Infrastructure.
//
// DXGI is the COM-based factory and swap-chain layer underneath
// D3D10/11/12 and many other GPU APIs.  Most non-trivial usage
// requires walking COM vtables; the bindings here cover the
// flat C entry points plus the most common structs.

// ─── DXGI_FORMAT values (selected) ───────────────────────────────
pub enum DxgiFormat: u32 {
    unknown            = 0
    r32g32b32a32_typeless = 1
    r32g32b32a32_float = 2
    r32g32b32a32_uint  = 3
    r32g32b32a32_sint  = 4
    r32g32b32_typeless = 5
    r32g32b32_float    = 6
    r32g32b32_uint     = 7
    r32g32b32_sint     = 8
    r16g16b16a16_float = 10
    r16g16b16a16_unorm = 11
    r32g32_float       = 16
    r10g10b10a2_unorm  = 24
    r11g11b10_float    = 26
    r8g8b8a8_typeless  = 27
    r8g8b8a8_unorm     = 28
    r8g8b8a8_unorm_srgb = 29
    r8g8b8a8_uint      = 30
    r8g8b8a8_snorm     = 31
    r8g8b8a8_sint      = 32
    r16g16_float       = 34
    r32_typeless       = 39
    d32_float          = 40
    r32_float          = 41
    r32_uint           = 42
    r32_sint           = 43
    r24g8_typeless     = 44
    d24_unorm_s8_uint  = 45
    r8g8_unorm         = 49
    r16_float          = 54
    d16_unorm          = 55
    r16_unorm          = 56
    r16_uint           = 57
    r8_unorm           = 61
    r8_uint            = 62
    a8_unorm           = 65
    bc1_unorm          = 71
    bc1_unorm_srgb     = 72
    bc3_unorm          = 77
    bc3_unorm_srgb     = 78
    bc4_unorm          = 80
    bc5_unorm          = 83
    b8g8r8a8_unorm     = 87
    b8g8r8x8_unorm     = 88
    bc7_unorm          = 98
    bc7_unorm_srgb     = 99
}

// ─── DXGI_SWAP_EFFECT ────────────────────────────────────────────
pub enum DxgiSwapEffect: u32 {
    discard          = 0   // legacy blit-and-discard (DX9-style)
    sequential       = 1   // legacy preserve back-buffers
    flipSequential   = 3   // modern DWM Flip (Win 8+)
    flipDiscard      = 4   // recommended for D3D11/12 (Win 10+)
}

// ─── DXGI_USAGE flags ────────────────────────────────────────────
@flags
pub enum DxgiUsage: u32 {
    backBuffer         = 0x00000040
    renderTargetOutput = 0x00000020
    shaderInput        = 0x00000010
    sharedSurface      = 0x00000200
    unorderedAccess    = 0x00000400
    discardOnPresent   = 0x00000100
    readOnly           = 0x00000800
}

// ─── DXGI_PRESENT flags ──────────────────────────────────────────
@flags
pub enum DxgiPresent: u32 {
    none           = 0
    doNotSequence  = 0x02
    test           = 0x01
    restart        = 0x04
    doNotWait      = 0x08
    restrictToOutput = 0x10
    stereoPreferRight = 0x20
    stereoTemporaryMono = 0x40
    allowTearing   = 0x200   // requires DXGI_SWAP_CHAIN_FLAG_ALLOW_TEARING
}

// ─── DXGI_SWAP_CHAIN_FLAG values ─────────────────────────────────
@flags
pub enum DxgiSwapChainFlag: u32 {
    nonprerotated      = 0x1
    allowModeSwitch    = 0x2
    gdiCompatible      = 0x4
    restrictedContent  = 0x8
    restrictedShared   = 0x10
    displayOnly        = 0x20
    frameLatencyWaitableObject = 0x100
    foregroundLayer    = 0x80
    fullscreenVideo    = 0x200
    yuvVideo           = 0x400
    hwProtected        = 0x800
    allowTearing       = 0x800
    restrictedToAllHolographicDisplays = 0x1000
}

// ─── DXGI_ALPHA_MODE ─────────────────────────────────────────────
pub enum DxgiAlphaMode: u32 {
    unspecified  = 0
    premultiplied = 1
    straight     = 2
    ignore       = 3
}

// ─── DXGI_SCALING ────────────────────────────────────────────────
pub enum DxgiScaling: u32 {
    stretch      = 0
    none_        = 1
    aspectRatioStretch = 2
}

// ─── Common HRESULT for DXGI ─────────────────────────────────────
pub enum DxgiResult: i32 {
    ok               = 0
    invalidCall      = -2005270527  // 0x887A0001
    notFound         = -2005270526  // 0x887A0002
    moreData         = -2005270525  // 0x887A0003
    unsupported      = -2005270524  // 0x887A0004
    deviceRemoved    = -2005270523  // 0x887A0005
    deviceHung       = -2005270522  // 0x887A0006
    deviceReset      = -2005270521  // 0x887A0007
    wasStillDrawing  = -2005270518  // 0x887A000A
    frameStatisticsDisjoint = -2005270517 // 0x887A000B
    accessDenied     = -2005270505  // 0x887A0026
    accessLost       = -2005270504  // 0x887A0027
}

@extern(C, "dxgi") {
    // ── DXGI_SAMPLE_DESC — 8 bytes ───────────────────────────────

    pub struct DXGI_SAMPLE_DESC {
        Count:   u32
        Quality: u32
    }

    // ── DXGI_MODE_DESC — 28 bytes ────────────────────────────────

    pub struct DXGI_MODE_DESC {
        Width:            u32
        Height:           u32
        RefreshRate_numerator: u32   // inline DXGI_RATIONAL
        RefreshRate_denominator: u32
        Format:           u32
        ScanlineOrdering: u32
        Scaling:          u32
    }

    // ── DXGI_SWAP_CHAIN_DESC — 56 bytes ──────────────────────────

    /// Legacy DX10/11 swap-chain description.
    /// For DX11.1+ / DX12, prefer DXGI_SWAP_CHAIN_DESC1.
    pub struct DXGI_SWAP_CHAIN_DESC {
        BufferDesc:    DXGI_MODE_DESC
        SampleDesc:    DXGI_SAMPLE_DESC
        BufferUsage:   u32
        BufferCount:   u32
        OutputWindow:  i64    // *HWND
        Windowed:      i32
        SwapEffect:    u32
        Flags:         u32
    }

    // ── DXGI_SWAP_CHAIN_DESC1 — 44 bytes ─────────────────────────

    /// Modern DXGI swap-chain description (DX11.1+ / DX12).
    /// Pair with `CreateSwapChainForHwnd` / `_ForComposition`.
    pub struct DXGI_SWAP_CHAIN_DESC1 {
        Width:       u32
        Height:      u32
        Format:      u32
        Stereo:      i32
        SampleDesc:  DXGI_SAMPLE_DESC
        BufferUsage: u32
        BufferCount: u32
        Scaling:     u32
        SwapEffect:  u32
        AlphaMode:   u32
        Flags:       u32
    }

    // ── DXGI_ADAPTER_DESC — 256 bytes (approx) ───────────────────

    /// Adapter information returned by IDXGIAdapter::GetDesc.
    /// `Description`: UTF-16 device string, max 128 chars (256 bytes
    /// = 32 × u64).  Cast `&adapter._d00 as *const u16` to read it.
    pub struct DXGI_ADAPTER_DESC {
        _d00: u64
        _d01: u64
        _d02: u64
        _d03: u64
        _d04: u64
        _d05: u64
        _d06: u64
        _d07: u64
        _d08: u64
        _d09: u64
        _d10: u64
        _d11: u64
        _d12: u64
        _d13: u64
        _d14: u64
        _d15: u64
        _d16: u64
        _d17: u64
        _d18: u64
        _d19: u64
        _d20: u64
        _d21: u64
        _d22: u64
        _d23: u64
        _d24: u64
        _d25: u64
        _d26: u64
        _d27: u64
        _d28: u64
        _d29: u64
        _d30: u64
        _d31: u64
        VendorId:              u32
        DeviceId:              u32
        SubSysId:              u32
        Revision:              u32
        DedicatedVideoMemory:  u64
        DedicatedSystemMemory: u64
        SharedSystemMemory:    u64
        AdapterLuid_lo:        u32     // LUID = i32 lo, i32 hi
        AdapterLuid_hi:        i32
    }

    // ── Factory creation ──────────────────────────────────────────

    /// Creates a DXGI 1.0 factory. Pass IID_IDXGIFactory.
    @lib pub fn CreateDXGIFactory(
        riid:     *const void,    // *IID
        ppFactory: **void
    ): i32

    /// Creates a DXGI 1.1 factory (preferred for D3D11). Pass IID_IDXGIFactory1.
    @lib pub fn CreateDXGIFactory1(
        riid:     *const void,
        ppFactory: **void
    ): i32

    /// Creates a DXGI 1.3 / 1.4 / 1.6 factory.  `Flags`: 0 or 1 (DXGI_CREATE_FACTORY_DEBUG).
    /// Use with IID_IDXGIFactory2/3/4/5/6/7 depending on the version needed.
    @lib pub fn CreateDXGIFactory2(
        Flags:    u32,
        riid:     *const void,
        ppFactory: **void
    ): i32

    // ── Debug helpers (dxgidebug.dll, but documented here) ───────

    /// Returns the DXGI debug interface (live-object reporting,
    /// API tracing).  IID_IDXGIDebug is well-known.
    /// Note: Lives in dxgidebug.dll on most systems.
    @lib pub fn DXGIGetDebugInterface1(
        Flags: u32,
        riid:  *const void,
        pDebug: **void
    ): i32

    // ─── DXGI @com interfaces ─────────────────────────────────────
    //
    // Slot positions match the SDK vtable; methods this repo doesn't
    // call yet stay as `_slotN_Name()` placeholders so binary
    // compatibility is preserved.

    @com pub interface IUnknown {
        QueryInterface(riid: *const void, ppvObject: **void): i32
        AddRef(): u32
        Release(): u32
    }

    @com pub interface IDXGIObject : IUnknown {
        SetPrivateData(Name: *const void, DataSize: u32, pData: *const void): i32
        SetPrivateDataInterface(Name: *const void, pUnknown: i64): i32
        GetPrivateData(Name: *const void, pDataSize: *u32, pData: *void): i32
        GetParent(riid: *const void, ppParent: **void): i32
    }

    @com pub interface IDXGIDeviceSubObject : IDXGIObject {
        GetDevice(riid: *const void, ppDevice: **void): i32
    }

    @com pub interface IDXGISwapChain : IDXGIDeviceSubObject {
        Present(SyncInterval: u32, Flags: u32): i32
        GetBuffer(Buffer: u32, riid: *const void, ppSurface: **void): i32
        _slot10_SetFullscreenState()
        _slot11_GetFullscreenState()
        _slot12_GetDesc()
        _slot13_ResizeBuffers()
        _slot14_ResizeTarget()
        _slot15_GetContainingOutput()
        _slot16_GetFrameStatistics()
        _slot17_GetLastPresentCount()
    }

    @com pub interface IDXGISwapChain1 : IDXGISwapChain {
        _slot18_GetDesc1()
        _slot19_GetFullscreenDesc()
        _slot20_GetHwnd()
        _slot21_GetCoreWindow()
        _slot22_Present1()
        _slot23_IsTemporaryMonoSupported()
        _slot24_GetRestrictToOutput()
        _slot25_SetBackgroundColor()
        _slot26_GetBackgroundColor()
        _slot27_SetRotation()
        _slot28_GetRotation()
    }

    @com pub interface IDXGISwapChain2 : IDXGISwapChain1 {
        _slot29_SetSourceSize()
        _slot30_GetSourceSize()
        _slot31_SetMaximumFrameLatency()
        _slot32_GetMaximumFrameLatency()
        _slot33_GetFrameLatencyWaitableObject()
        _slot34_SetMatrixTransform()
        _slot35_GetMatrixTransform()
    }

    @com pub interface IDXGISwapChain3 : IDXGISwapChain2 {
        GetCurrentBackBufferIndex(): u32
        _slot37_CheckColorSpaceSupport()
        _slot38_SetColorSpace1()
        _slot39_ResizeBuffers1()
    }

    @com pub interface IDXGIFactory : IDXGIObject {
        _slot7_EnumAdapters()
        MakeWindowAssociation(WindowHandle: i64, Flags: u32): i32
        _slot9_GetWindowAssociation()
        _slot10_CreateSwapChain()
        _slot11_CreateSoftwareAdapter()
    }

    @com pub interface IDXGIFactory1 : IDXGIFactory {
        _slot12_EnumAdapters1()
        _slot13_IsCurrent()
    }

    @com pub interface IDXGIFactory2 : IDXGIFactory1 {
        _slot14_IsWindowedStereoEnabled()
        CreateSwapChainForHwnd(
            pDevice:               i64,                    // IUnknown* (the command queue for D3D12)
            hWnd:                  i64,                    // HWND
            pDesc:                 *const DXGI_SWAP_CHAIN_DESC1,
            pFullscreenDesc:       *const void,            // optional, NULL for windowed
            pRestrictToOutput:     i64,                    // IDXGIOutput*, NULL = no restriction
            ppSwapChain:           **void
        ): i32
        _slot16_CreateSwapChainForCoreWindow()
        _slot17_GetSharedResourceAdapterLuid()
        _slot18_RegisterStereoStatusWindow()
        _slot19_RegisterStereoStatusEvent()
        _slot20_UnregisterStereoStatus()
        _slot21_RegisterOcclusionStatusWindow()
        _slot22_RegisterOcclusionStatusEvent()
        _slot23_UnregisterOcclusionStatus()
        _slot24_CreateSwapChainForComposition()
    }

    @com pub interface IDXGIFactory3 : IDXGIFactory2 {
        _slot25_GetCreationFlags()
    }

    @com pub interface IDXGIFactory4 : IDXGIFactory3 {
        _slot26_EnumAdapterByLuid()
        _slot27_EnumWarpAdapter()
    }
}
