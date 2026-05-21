/// dcomp.dll — DirectComposition: GPU-accelerated visual layer.
///
/// Builds the "visual tree" that Windows 8+ uses to composite
/// layered windows efficiently. Pairs naturally with D3D11 / D2D1
/// surfaces.
///
/// Bring-up:
///   let device: *void = 0 as *void
///   DCompositionCreateDevice(d3d11device, &IID_IDCompositionDevice,
///                            &device)
///   device.CreateTargetForHwnd(hwnd, TRUE, &target)
///   device.CreateVisual(&visual)
///   visual.SetContent(d3d11surface)
///   target.SetRoot(visual)
///   device.Commit()
///
/// IDCompositionDevice / Visual / Target methods aren't bound at
/// the ilang call level — walk the COM vtables.

@extern(C, "dcomp") {
    /// `DCompositionCreateDevice(dxgiDevice, iid, ppDevice)`.
    /// `dxgiDevice` is an `*IDXGIDevice` (or NULL for a Software
    /// device).
    @lib pub fn DCompositionCreateDevice(
        dxgiDevice: *void,                  // *IDXGIDevice
        iid:        *const void,            // *GUID
        dcompDevice: **void
    ): i32

    /// Vista-style two-arg variant — same shape but returns the
    /// `IDCompositionDevice2` family (DComp 2 + per-frame
    /// statistics). On Win10+ both return the same physical object.
    @lib pub fn DCompositionCreateDevice2(
        renderingDevice: *void,             // *IDXGIDevice or *D3D11Device
        iid:            *const void,
        dcompDevice:    **void
    ): i32

    @lib pub fn DCompositionCreateDevice3(
        renderingDevice: *void,
        iid:            *const void,
        dcompDevice:    **void
    ): i32

    /// `DCompositionCreateSurfaceHandle` — share a surface across
    /// processes via duplicated kernel handles.
    @lib pub fn DCompositionCreateSurfaceHandle(
        desiredAccess: u32,
        securityAttributes: *void,
        surfaceHandle:  *i64
    ): i32

    /// One-shot wait until the next compositor V-sync.
    @lib pub fn DCompositionWaitForCompositorClock(
        count:          u32,
        handles:        *const i64,
        timeoutInMs:    u32
    ): u32

    /// Total frame number from the compositor's perspective.
    @lib pub fn DCompositionGetFrameId(
        frameIdType:    u32,                // COMPOSITION_FRAME_ID_TYPE: 0 CREATED, 1 CONFIRMED, 2 COMPLETED
        frameId:        *u64
    ): i32

    /// Stats per frame — frame id is the value from `GetFrameId`.
    @lib pub fn DCompositionGetStatistics(
        frameId:             u64,
        stats:               *void,         // *COMPOSITION_FRAME_STATS
        targetIdCount:       u32,
        targetIds:           *const u64,
        actualTargetIdCount: *u32
    ): i32

    @lib pub fn DCompositionGetTargetStatistics(
        frameId:    u64,
        targetId:   u64,
        stats:      *void                   // *COMPOSITION_TARGET_STATS
    ): i32

    /// Make the next root-set update atomic across multiple
    /// devices.
    @lib pub fn DCompositionBoostCompositorClock(
        enable: i32
    ): i32

    /// Notify the compositor that the calling visual / surface is
    /// off-screen and animations can pause.
    @lib pub fn DCompositionAttachMouseDragToHwnd(
        hwnd:    i64,
        enable:  i32
    ): i32

    /// Wheel-drag attach — see Win10 build 19041+.
    @lib pub fn DCompositionAttachMouseWheelToHwnd(
        hwnd:    i64,
        enable:  i32
    ): i32
}
