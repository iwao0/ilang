/// dxgi.dll — DirectX Graphics Infrastructure.
///
/// DXGI is the COM-based factory and swap-chain layer underneath
/// D3D10/11/12 and many other GPU APIs.  Most non-trivial usage
/// requires walking COM vtables; the bindings here cover the
/// flat C entry points plus the most common structs.

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

// ─── DXGI_GPU_PREFERENCE ─────────────────────────────────────────
pub enum DxgiGpuPreference: u32 {
    unspecified     = 0
    minimumPower    = 1
    highPerformance = 2
}

// ─── DXGI_ADAPTER_FLAG ───────────────────────────────────────────
@flags
pub enum DxgiAdapterFlag: u32 {
    none        = 0
    remote      = 0x1   // legacy
    software    = 0x2   // WARP-style basic render driver
}

// ─── DXGI_ADAPTER_FLAG3 (Adapter4-era) ────────────────────────────
@flags
pub enum DxgiAdapterFlag3: u32 {
    none                       = 0
    remote                     = 0x1
    software                   = 0x2
    aclEnforcedCrossAdapter    = 0x4
    supportMonitoredFences     = 0x8
    supportNonMonitoredFences  = 0x10
    keyedMutexConformance      = 0x20
}

// ─── DXGI_COLOR_SPACE_TYPE (commonly used values) ────────────────
pub enum DxgiColorSpaceType: u32 {
    rgbFullG22NoneP709         = 0     // sRGB
    rgbFullG10NoneP709         = 1     // scRGB
    rgbStudioG22NoneP709       = 2
    rgbStudioG22NoneP2020      = 3
    reserved                   = 4
    yccFullG22NoneP709X601     = 5
    yccStudioG22LeftP601       = 6
    yccFullG22LeftP601         = 7
    yccStudioG22LeftP709       = 8
    yccFullG22LeftP709         = 9
    yccStudioG22LeftP2020      = 10
    yccFullG22LeftP2020        = 11
    rgbFullG2084NoneP2020      = 12    // HDR10
    yccStudioG2084LeftP2020    = 14
    rgbFullG22NoneP2020        = 17
    yccStudioGhlgTopLeftP2020  = 19
    yccFullGhlgTopLeftP2020    = 20
    rgbStudioG24NoneP709       = 21
    rgbStudioG24NoneP2020      = 22
    custom                     = 0xFFFFFFFF
}

// ─── DXGI_HDR_METADATA_TYPE ──────────────────────────────────────
pub enum DxgiHdrMetadataType: u32 {
    none  = 0
    hdr10 = 1
    hdr10plus = 2
}

// ─── DXGI_MWA (MakeWindowAssociation flags) ──────────────────────
@flags
pub enum DxgiMwa: u32 {
    none           = 0
    noWindowChanges = 1
    noAltEnter     = 2
    noPrintScreen  = 4
    valid          = 0x7
}

// ─── DXGI_RESOURCE_PRIORITY ──────────────────────────────────────
pub enum DxgiResourcePriority: u32 {
    minimum = 0x28000000
    low     = 0x50000000
    normal  = 0x78000000
    high    = 0xa0000000
    maximum = 0xc8000000
}

// ─── IDXGIDebug RLO flags ────────────────────────────────────────
@flags
pub enum DxgiDebugRloFlags: u32 {
    summary       = 1
    detail        = 2
    ignoreInternal = 4
    all           = 7
}

// ─── IDXGIInfoQueue ──────────────────────────────────────────────
pub enum DxgiInfoQueueMessageCategory: u32 {
    unknown               = 0
    miscellaneous         = 1
    initialization        = 2
    cleanup               = 3
    compilation           = 4
    stateCreation         = 5
    stateSetting          = 6
    stateGetting          = 7
    resourceManipulation  = 8
    execution             = 9
    shader                = 10
}

pub enum DxgiInfoQueueMessageSeverity: u32 {
    corruption = 0
    error      = 1
    warning    = 2
    info       = 3
    message    = 4
}

// ─── DXGI_FEATURE ────────────────────────────────────────────────
pub enum DxgiFeature: u32 {
    presentAllowTearing = 0
}

// ─── IDXGIOutputDuplication pointer-shape types ──────────────────
pub enum DxgiOutduplPointerShapeType: u32 {
    monochrome = 1
    color      = 2
    maskedColor = 4
}

// ─── Common HRESULT for DXGI ─────────────────────────────────────
pub enum DxgiResult: u32 {
    ok               = 0
    invalidCall      = 0x887a0001  //
    notFound         = 0x887a0002  //
    moreData         = 0x887a0003  //
    unsupported      = 0x887a0004  //
    deviceRemoved    = 0x887a0005  //
    deviceHung       = 0x887a0006  //
    deviceReset      = 0x887a0007  //
    wasStillDrawing  = 0x887a000a  //
    frameStatisticsDisjoint = 0x887a000b //
    accessDenied     = 0x887a0017  //
    accessLost       = 0x887a0018  //
}

use user32 { HWND }
use kernel32 { HANDLE }
use ole32 { IUnknown }

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
        OutputWindow:  i64    // HWND
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

    // ── DXGI_RATIONAL — 8 bytes ──────────────────────────────────

    /// Generic numerator/denominator pair. Used directly by some
    /// structs and inlined into others (e.g. DXGI_MODE_DESC).
    pub struct DXGI_RATIONAL {
        Numerator:   u32
        Denominator: u32
    }

    // ── DXGI_OUTPUT_DESC — ~96 bytes ─────────────────────────────

    /// `DeviceName`: UTF-16 GDI device name, 32 wide chars (64 bytes).
    pub struct DXGI_OUTPUT_DESC {
        DeviceName:        u16[32]
        DesktopCoordinates_left:   i32
        DesktopCoordinates_top:    i32
        DesktopCoordinates_right:  i32
        DesktopCoordinates_bottom: i32
        AttachedToDesktop: i32
        Rotation:          u32
        Monitor:           i64    // HMONITOR
    }

    // ── DXGI_QUERY_VIDEO_MEMORY_INFO — 32 bytes ──────────────────

    /// Returned by IDXGIAdapter3::QueryVideoMemoryInfo.
    /// `Budget`: app's current OS-allotted budget (bytes).
    /// `CurrentUsage`: bytes the app's resident set currently uses.
    pub struct DXGI_QUERY_VIDEO_MEMORY_INFO {
        Budget:                u64
        CurrentUsage:          u64
        AvailableForReservation: u64
        CurrentReservation:    u64
    }

    // ── DXGI_HDR_METADATA_HDR10 — 24 bytes ───────────────────────

    /// HDR10 mastering display metadata (ST.2086). Coordinates are
    /// fixed-point at the standard scale; see the SDK header for
    /// exact units.
    pub struct DXGI_HDR_METADATA_HDR10 {
        RedPrimary0:                u16
        RedPrimary1:                u16
        GreenPrimary0:              u16
        GreenPrimary1:              u16
        BluePrimary0:               u16
        BluePrimary1:               u16
        WhitePoint0:                u16
        WhitePoint1:                u16
        MaxMasteringLuminance:      u32
        MinMasteringLuminance:      u32
        MaxContentLightLevel:       u16
        MaxFrameAverageLightLevel:  u16
    }

    // ── DXGI_PRESENT_PARAMETERS — 24 bytes ───────────────────────

    /// Used by IDXGISwapChain1::Present1 for dirty-rect / scroll-
    /// rect Present optimisations.
    pub struct DXGI_PRESENT_PARAMETERS {
        DirtyRectsCount: u32
        _pad0:           u32
        pDirtyRects:     *const D3D12_PRESENT_RECT_PLACEHOLDER
        pScrollRect:     *const D3D12_PRESENT_RECT_PLACEHOLDER
        pScrollOffset:   *const D3D12_PRESENT_POINT_PLACEHOLDER
    }

    /// Helper layouts used inside DXGI_PRESENT_PARAMETERS — match
    /// Win32 `RECT` / `POINT` exactly. Kept here so swap-chain code
    /// can name them without dragging in the broader user32 binding.
    pub struct D3D12_PRESENT_RECT_PLACEHOLDER {
        left:   i32
        top:    i32
        right:  i32
        bottom: i32
    }

    pub struct D3D12_PRESENT_POINT_PLACEHOLDER {
        x: i32
        y: i32
    }

    // ── DXGI_ADAPTER_DESC — 256 bytes (approx) ───────────────────

    /// Adapter information returned by IDXGIAdapter::GetDesc.
    /// `Description`: UTF-16 device string, max 128 chars (256 bytes
    /// = 32 × u64).  Cast `&adapter._d00 as *const u16` to read it.
    pub struct DXGI_ADAPTER_DESC {
                Description: u16[128]
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

    // ── DXGI_ADAPTER_DESC1 — DESC + Flags field ──────────────────

    /// Returned by IDXGIAdapter1::GetDesc1. Adds the `Flags` field
    /// (DxgiAdapterFlag.*) so callers can distinguish software /
    /// remote adapters from real GPUs without probing them.
    pub struct DXGI_ADAPTER_DESC1 {
                Description: u16[128]
        VendorId:              u32
        DeviceId:              u32
        SubSysId:              u32
        Revision:              u32
        DedicatedVideoMemory:  u64
        DedicatedSystemMemory: u64
        SharedSystemMemory:    u64
        AdapterLuid_lo:        u32
        AdapterLuid_hi:        i32
        Flags:                 u32       // DxgiAdapterFlag
        _pad0:                 u32
    }

    // ── DXGI_ADAPTER_DESC2 — DESC1 + graphics/compute pref ───────

    /// Returned by IDXGIAdapter2::GetDesc2. Adds the
    /// `GraphicsPreemptionGranularity` / `ComputePreemptionGranularity`
    /// enum-valued fields so callers can reason about how aggressively
    /// the OS can preempt long-running work.
    pub struct DXGI_ADAPTER_DESC2 {
                Description: u16[128]
        VendorId:              u32
        DeviceId:              u32
        SubSysId:              u32
        Revision:              u32
        DedicatedVideoMemory:  u64
        DedicatedSystemMemory: u64
        SharedSystemMemory:    u64
        AdapterLuid_lo:        u32
        AdapterLuid_hi:        i32
        Flags:                 u32
        GraphicsPreemptionGranularity: u32
        ComputePreemptionGranularity:  u32
        _pad0:                 u32
    }

    // ── DXGI_ADAPTER_DESC3 — DESC2 + DxgiAdapterFlag3 ────────────

    /// Returned by IDXGIAdapter4::GetDesc3. Adds the `Flags` field
    /// widened to `DxgiAdapterFlag3` (sync-fence + ACL bits).
    pub struct DXGI_ADAPTER_DESC3 {
                Description: u16[128]
        VendorId:              u32
        DeviceId:              u32
        SubSysId:              u32
        Revision:              u32
        DedicatedVideoMemory:  u64
        DedicatedSystemMemory: u64
        SharedSystemMemory:    u64
        AdapterLuid_lo:        u32
        AdapterLuid_hi:        i32
        Flags:                 u32       // DxgiAdapterFlag3
        GraphicsPreemptionGranularity: u32
        ComputePreemptionGranularity:  u32
        _pad0:                 u32
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
            pDevice:               *void,                  // IUnknown* (the command queue for D3D12)
            hWnd:                  HWND,
            pDesc:                 *const DXGI_SWAP_CHAIN_DESC1,
            pFullscreenDesc:       *const void,            // optional, NULL for windowed
            pRestrictToOutput:     *void,                  // IDXGIOutput*, NULL = no restriction
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
        EnumAdapterByLuid(
            AdapterLuid_lo: u32,
            AdapterLuid_hi: i32,
            riid:           *const void,
            ppvAdapter:     **void
        ): i32
        EnumWarpAdapter(riid: *const void, ppvAdapter: **void): i32
    }

    /// IDXGIFactory5 — adds CheckFeatureSupport (the only feature
    /// currently reachable from here is `DXGI_FEATURE_PRESENT_ALLOW_TEARING`).
    @com pub interface IDXGIFactory5 : IDXGIFactory4 {
        CheckFeatureSupport(
            Feature: u32,
            pFeatureSupportData: *void,
            FeatureSupportDataSize: u32
        ): i32
    }

    /// IDXGIFactory6 — adds `EnumAdapterByGpuPreference`, the only
    /// adapter enumeration entry point on modern Windows that
    /// honours the user-configurable GPU-preference list (Settings
    /// → Display → Graphics).
    @com pub interface IDXGIFactory6 : IDXGIFactory5 {
        EnumAdapterByGpuPreference(
            Adapter:        u32,
            GpuPreference:  u32,                    // DxgiGpuPreference
            riid:           *const void,
            ppvAdapter:     **void
        ): i32
    }

    /// IDXGIFactory7 — adds adapter-list-changed event registration.
    @com pub interface IDXGIFactory7 : IDXGIFactory6 {
        RegisterAdaptersChangedEvent(hEvent: HANDLE, pdwCookie: *u32): i32
        UnregisterAdaptersChangedEvent(dwCookie: u32): i32
    }

    /// IDXGISwapChain4 — adds HDR colour-space + metadata setters.
    @com pub interface IDXGISwapChain4 : IDXGISwapChain3 {
        SetHDRMetaData(
            Type:    u32,                           // DxgiHdrMetadataType
            Size:    u32,
            pMetaData: *const void
        ): i32
    }

    // ── Adapter chain ────────────────────────────────────────────

    /// IDXGIAdapter — physical / WARP GPU handle.
    @com pub interface IDXGIAdapter : IDXGIObject {
        EnumOutputs(Output: u32, ppOutput: **void): i32
        GetDesc(retval: *DXGI_ADAPTER_DESC): i32
        _slot9_CheckInterfaceSupport()
    }

    /// IDXGIAdapter1 — adds GetDesc1 (with the `Flags` field that
    /// distinguishes software-vs-hardware adapters).
    @com pub interface IDXGIAdapter1 : IDXGIAdapter {
        GetDesc1(retval: *DXGI_ADAPTER_DESC1): i32
    }

    /// IDXGIAdapter2 — adds GetDesc2 (preemption granularity).
    @com pub interface IDXGIAdapter2 : IDXGIAdapter1 {
        GetDesc2(retval: *DXGI_ADAPTER_DESC2): i32
    }

    /// IDXGIAdapter3 — adds residency budget queries + change events.
    @com pub interface IDXGIAdapter3 : IDXGIAdapter2 {
        _slot12_RegisterHardwareContentProtectionTeardownStatusEvent()
        _slot13_UnregisterHardwareContentProtectionTeardownStatus()
        QueryVideoMemoryInfo(
            NodeIndex:               u32,
            MemorySegmentGroup:      u32,
            pVideoMemoryInfo:        *DXGI_QUERY_VIDEO_MEMORY_INFO
        ): i32
        SetVideoMemoryReservation(
            NodeIndex:               u32,
            MemorySegmentGroup:      u32,
            Reservation:             u64
        ): i32
        RegisterVideoMemoryBudgetChangeNotificationEvent(
            hEvent:   HANDLE,
            pdwCookie: *u32
        ): i32
        UnregisterVideoMemoryBudgetChangeNotification(dwCookie: u32)
    }

    /// IDXGIAdapter4 — adds GetDesc3 (DxgiAdapterFlag3 widened).
    @com pub interface IDXGIAdapter4 : IDXGIAdapter3 {
        GetDesc3(retval: *DXGI_ADAPTER_DESC3): i32
    }

    // ── Output chain ─────────────────────────────────────────────

    /// IDXGIOutput — display output (a monitor attached to the
    /// adapter). Many methods take/return `DXGI_MODE_DESC` arrays;
    /// only the most-used ones get typed signatures here.
    @com pub interface IDXGIOutput : IDXGIObject {
        GetDesc(retval: *DXGI_OUTPUT_DESC): i32
        GetDisplayModeList(
            EnumFormat:         u32,
            Flags:              u32,
            pNumModes:          *u32,
            pDesc:              *DXGI_MODE_DESC
        ): i32
        FindClosestMatchingMode(
            pModeToMatch:    *const DXGI_MODE_DESC,
            pClosestMatch:   *DXGI_MODE_DESC,
            pConcernedDevice: *void
        ): i32
        WaitForVBlank(): i32
        TakeOwnership(pDevice: *void, Exclusive: i32): i32
        ReleaseOwnership()
        _slot13_GetGammaControlCapabilities()
        _slot14_SetGammaControl()
        _slot15_GetGammaControl()
        _slot16_SetDisplaySurface()
        _slot17_GetDisplaySurfaceData()
        _slot18_GetFrameStatistics()
    }

    /// IDXGIOutput1 — adds 1.2-era mode listing + duplicate-output.
    @com pub interface IDXGIOutput1 : IDXGIOutput {
        _slot19_GetDisplayModeList1()
        _slot20_FindClosestMatchingMode1()
        _slot21_GetDisplaySurfaceData1()
        DuplicateOutput(pDevice: *void, ppOutputDuplication: **void): i32
    }

    /// IDXGIOutput2 — adds overlay-format support query.
    @com pub interface IDXGIOutput2 : IDXGIOutput1 {
        SupportsOverlays(): i32
    }

    /// IDXGIOutput3 — overlay-color-format / interlace support.
    @com pub interface IDXGIOutput3 : IDXGIOutput2 {
        _slot24_CheckOverlaySupport()
    }

    /// IDXGIOutput4 — adds HDR / wide-color colour-space probing.
    @com pub interface IDXGIOutput4 : IDXGIOutput3 {
        _slot25_CheckOverlayColorSpaceSupport()
    }

    /// IDXGIOutput5 — adds DuplicateOutput1 (HDR-aware capture).
    @com pub interface IDXGIOutput5 : IDXGIOutput4 {
        DuplicateOutput1(
            pDevice:    *void,
            Flags:      u32,
            SupportedFormatsCount: u32,
            pSupportedFormats:     *const u32,
            ppOutputDuplication:   **void
        ): i32
    }

    /// IDXGIOutput6 — adds GetDesc1 (HDR + colour-space data).
    @com pub interface IDXGIOutput6 : IDXGIOutput5 {
        _slot27_GetDesc1()
        _slot28_CheckHardwareCompositionSupport()
    }

    // ── Device chain (mostly used through D3D11/D3D12 wrappers) ──

    @com pub interface IDXGIDevice : IDXGIObject {
        GetAdapter(pAdapter: **void): i32
        _slot8_CreateSurface()
        _slot9_QueryResourceResidency()
        SetGPUThreadPriority(Priority: i32): i32
        GetGPUThreadPriority(pPriority: *i32): i32
    }

    @com pub interface IDXGIDevice1 : IDXGIDevice {
        SetMaximumFrameLatency(MaxLatency: u32): i32
        GetMaximumFrameLatency(pMaxLatency: *u32): i32
    }

    @com pub interface IDXGIDevice2 : IDXGIDevice1 {
        _slot14_OfferResources()
        _slot15_ReclaimResources()
        EnqueueSetEvent(hEvent: HANDLE): i32
    }

    @com pub interface IDXGIDevice3 : IDXGIDevice2 {
        Trim()
    }

    @com pub interface IDXGIDevice4 : IDXGIDevice3 {
        _slot18_OfferResources1()
        _slot19_ReclaimResources1()
    }

    // ── Resource / surface / keyed-mutex chain ───────────────────

    /// `DXGI_MAPPED_RECT` — pitch + base pointer for IDXGISurface::Map.
    pub struct DXGI_MAPPED_RECT {
        Pitch:  i32
        _pad0:  u32
        pBits:  *u8
    }

    /// `DXGI_SHARED_RESOURCE` — handle wrapper.
    pub struct DXGI_SHARED_RESOURCE {
        Handle: HANDLE
    }

    @com pub interface IDXGIResource : IDXGIDeviceSubObject {
        GetSharedHandle(pSharedHandle: *HANDLE): i32
        GetUsage(pUsage: *u32): i32
        SetEvictionPriority(EvictionPriority: u32): i32
        GetEvictionPriority(pEvictionPriority: *u32): i32
    }

    @com pub interface IDXGIResource1 : IDXGIResource {
        CreateSubresourceSurface(index: u32, ppSurface: **void): i32
        CreateSharedHandle(
            pAttributes: *const void,
            dwAccess:    u32,
            lpName:      *const u16,
            pHandle:     *HANDLE
        ): i32
    }

    @com pub interface IDXGIKeyedMutex : IDXGIDeviceSubObject {
        AcquireSync(Key: u64, dwMilliseconds: u32): i32
        ReleaseSync(Key: u64): i32
    }

    @com pub interface IDXGISurface : IDXGIDeviceSubObject {
        GetDesc(retval: *void): i32                          // DXGI_SURFACE_DESC
        Map(pLockedRect: *DXGI_MAPPED_RECT, MapFlags: u32): i32
        Unmap(): i32
    }

    @com pub interface IDXGISurface1 : IDXGISurface {
        GetDC(Discard: i32, phdc: *i64): i32                // HDC out
        ReleaseDC(pDirtyRect: *const D3D12_PRESENT_RECT_PLACEHOLDER): i32
    }

    @com pub interface IDXGISurface2 : IDXGISurface1 {
        GetResource(riid: *const void, ppParentResource: **void, pSubresourceIndex: *u32): i32
    }

    // ── Output duplication (desktop capture) ─────────────────────

    /// `DXGI_OUTDUPL_DESC` — describes a duplicated output target.
    pub struct DXGI_OUTDUPL_DESC {
        ModeDesc:             DXGI_MODE_DESC
        Rotation:             u32
        DesktopImageInSystemMemory: i32
    }

    /// `DXGI_OUTDUPL_POINTER_POSITION`.
    pub struct DXGI_OUTDUPL_POINTER_POSITION {
        Position_x:  i32
        Position_y:  i32
        Visible:     i32
    }

    /// `DXGI_OUTDUPL_FRAME_INFO`.
    pub struct DXGI_OUTDUPL_FRAME_INFO {
        LastPresentTime_lo:           u32
        LastPresentTime_hi:           i32
        LastMouseUpdateTime_lo:       u32
        LastMouseUpdateTime_hi:       i32
        AccumulatedFrames:            u32
        RectsCoalesced:               i32
        ProtectedContentMaskedOut:    i32
        PointerPosition:              DXGI_OUTDUPL_POINTER_POSITION
        TotalMetadataBufferSize:      u32
        PointerShapeBufferSize:       u32
    }

    /// `DXGI_OUTDUPL_POINTER_SHAPE_INFO`.
    pub struct DXGI_OUTDUPL_POINTER_SHAPE_INFO {
        Type:       u32
        Width:      u32
        Height:     u32
        Pitch:      u32
        HotSpot_x:  i32
        HotSpot_y:  i32
    }

    @com pub interface IDXGIOutputDuplication : IDXGIObject {
        GetDesc(retval: *DXGI_OUTDUPL_DESC)
        AcquireNextFrame(
            TimeoutInMilliseconds: u32,
            pFrameInfo:            *DXGI_OUTDUPL_FRAME_INFO,
            ppDesktopResource:     **void
        ): i32
        GetFrameDirtyRects(
            DirtyRectsBufferSize: u32,
            pDirtyRectsBuffer:    *void,
            pDirtyRectsBufferSizeRequired: *u32
        ): i32
        GetFrameMoveRects(
            MoveRectsBufferSize: u32,
            pMoveRectBuffer:     *void,
            pMoveRectsBufferSizeRequired: *u32
        ): i32
        GetFramePointerShape(
            PointerShapeBufferSize: u32,
            pPointerShapeBuffer:    *void,
            pPointerShapeBufferSizeRequired: *u32,
            pPointerShapeInfo:      *DXGI_OUTDUPL_POINTER_SHAPE_INFO
        ): i32
        MapDesktopSurface(pLockedRect: *DXGI_MAPPED_RECT): i32
        UnMapDesktopSurface(): i32
        ReleaseFrame(): i32
    }

    // ── Frame statistics / gamma control ─────────────────────────

    /// `DXGI_FRAME_STATISTICS`.
    pub struct DXGI_FRAME_STATISTICS {
        PresentCount:       u32
        PresentRefreshCount: u32
        SyncRefreshCount:   u32
        _pad0:              u32
        SyncQPCTime_lo:     u32
        SyncQPCTime_hi:     i32
        SyncGPUTime_lo:     u32
        SyncGPUTime_hi:     i32
    }

    /// `DXGI_RGB` — float RGB triple, used by gamma control.
    pub struct DXGI_RGB {
        Red:   f32
        Green: f32
        Blue:  f32
    }

    /// `DXGI_GAMMA_CONTROL_CAPABILITIES` (1031 floats — abbreviated to
    /// the leading fields; consumers walk the trailing curve via raw
    /// pointer arith over the 1025 `ControlPointPositions` floats).
    pub struct DXGI_GAMMA_CONTROL_CAPABILITIES {
        ScaleAndOffsetSupported: i32
        MaxConvertedValue:       f32
        MinConvertedValue:       f32
        NumGammaControlPoints:   u32
        // Trailing: ControlPointPositions[1025] f32 (4100 bytes).
        // Read via &caps + sizeof header for raw access.
    }

    /// `DXGI_GAMMA_CONTROL` — Scale/Offset + 1025 control-point
    /// `DXGI_RGB`s. Same trailing-array story as the capabilities
    /// struct; expose the header here.
    pub struct DXGI_GAMMA_CONTROL {
        Scale:   DXGI_RGB
        Offset:  DXGI_RGB
        // Trailing: GammaCurve[1025] DXGI_RGB.
    }

    // ── Info queue / debug ───────────────────────────────────────

    /// `DXGI_INFO_QUEUE_MESSAGE`. Variable-sized — the trailing
    /// `pDescription` UTF-8 string runs `DescriptionByteLength`
    /// bytes after the struct.
    pub struct DXGI_INFO_QUEUE_MESSAGE {
        Producer_d1: u32
        Producer_d2: u16
        Producer_d3: u16
        Producer_d4lo: u32
        Producer_d4hi: u32
        Category:    u32
        Severity:    u32
        ID:          i32
        _pad0:       u32
        pDescription: *const char
        DescriptionByteLength: u64
    }

    /// `DXGI_INFO_QUEUE_FILTER_DESC`.
    pub struct DXGI_INFO_QUEUE_FILTER_DESC {
        NumCategories: u32
        _pad0:         u32
        pCategoryList: *const u32
        NumSeverities: u32
        _pad1:         u32
        pSeverityList: *const u32
        NumIDs:        u32
        _pad2:         u32
        pIDList:       *const i32
    }

    /// `DXGI_INFO_QUEUE_FILTER`.
    pub struct DXGI_INFO_QUEUE_FILTER {
        AllowList: DXGI_INFO_QUEUE_FILTER_DESC
        DenyList:  DXGI_INFO_QUEUE_FILTER_DESC
    }

    @com pub interface IDXGIInfoQueue : IUnknown {
        _slot3_SetMessageCountLimit()
        ClearStoredMessages(Producer_d1: u32, Producer_d2: u16, Producer_d3: u16,
                            Producer_d4lo: u32, Producer_d4hi: u32)
        GetMessage(
            Producer_d1: u32, Producer_d2: u16, Producer_d3: u16,
            Producer_d4lo: u32, Producer_d4hi: u32,
            MessageIndex:           u64,
            pMessage:               *void,
            pMessageByteLength:     *u64
        ): i32
        _slot6_GetNumStoredMessagesAllowedByRetrievalFilters()
        GetNumStoredMessages(
            Producer_d1: u32, Producer_d2: u16, Producer_d3: u16,
            Producer_d4lo: u32, Producer_d4hi: u32
        ): u64
        _slot8_GetNumMessagesDiscardedByMessageCountLimit()
        _slot9_GetMessageCountLimit()
        _slot10_GetNumMessagesAllowedByStorageFilter()
        _slot11_GetNumMessagesDeniedByStorageFilter()
        _slot12_AddStorageFilterEntries()
        _slot13_GetStorageFilter()
        _slot14_ClearStorageFilter()
        _slot15_PushEmptyStorageFilter()
        _slot16_PushDenyAllStorageFilter()
        _slot17_PushCopyOfStorageFilter()
        _slot18_PushStorageFilter()
        _slot19_PopStorageFilter()
        _slot20_GetStorageFilterStackSize()
        _slot21_AddRetrievalFilterEntries()
        _slot22_GetRetrievalFilter()
        _slot23_ClearRetrievalFilter()
        _slot24_PushEmptyRetrievalFilter()
        _slot25_PushDenyAllRetrievalFilter()
        _slot26_PushCopyOfRetrievalFilter()
        _slot27_PushRetrievalFilter()
        _slot28_PopRetrievalFilter()
        _slot29_GetRetrievalFilterStackSize()
        _slot30_AddMessage()
        _slot31_AddApplicationMessage()
        _slot32_SetBreakOnCategory()
        _slot33_SetBreakOnSeverity()
        _slot34_SetBreakOnID()
        _slot35_GetBreakOnCategory()
        _slot36_GetBreakOnSeverity()
        _slot37_GetBreakOnID()
        _slot38_SetMuteDebugOutput()
        _slot39_GetMuteDebugOutput()
    }

    @com pub interface IDXGIDebug : IUnknown {
        /// Reports live DXGI / D3D objects matching `apiid`. Pass
        /// `DXGI_DEBUG_ALL` as the GUID to dump everything.
        ReportLiveObjects(
            apiid_d1: u32, apiid_d2: u16, apiid_d3: u16,
            apiid_d4lo: u32, apiid_d4hi: u32,
            flags: u32                                       // DxgiDebugRloFlags
        ): i32
    }

    @com pub interface IDXGIDebug1 : IDXGIDebug {
        EnableLeakTrackingForThread()
        DisableLeakTrackingForThread()
        IsLeakTrackingEnabledForThread(): i32
    }
}
