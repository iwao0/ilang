/// dwmapi.dll — Desktop Window Manager: composition, transparency,
/// Acrylic/Mica effects, thumbnail registration, and DWM flush.
///
/// All functions require Windows Vista or later.  On Windows 8 and
/// later, DWM composition is always on and DwmIsCompositionEnabled
/// always returns 1.

// ─── DWM window attribute IDs (DWMWA_*) ──────────────────────────
pub enum DwmWindowAttribute: u32 {
    ncRenderingEnabled    = 1    // [get] i32 — is non-client rendering on?
    ncRenderingPolicy     = 2    // [set] DwmNcRenderingPolicy
    transitionsForceDisabled = 3 // [set] i32 — disable transitions
    allowNcPaint          = 4    // [set] i32
    captionButtonBounds   = 5    // [get] RECT — caption button bounding rect
    nonClientRtlLayout    = 6    // [set] i32
    forceIconicRepresentation = 7// [set] i32
    flip3dPolicy          = 8    // [set] DwmFlip3dPolicy
    extendedFrameBounds   = 9    // [get] RECT — extended frame bounds
    hasIconicBitmap       = 10   // [set] i32
    disallowPeek          = 11   // [set] i32
    excludedFromPeek      = 12   // [set] i32
    cloak                 = 13   // [set] i32 — hide from DWM
    cloaked               = 14   // [get] u32 — why it is cloaked
    freezeRepresentation  = 15   // [set] i32
    // Windows 10 1607+
    passiveupdatemode     = 16
    useHostbackdropbrush  = 17
    // Windows 10 20H1+
    useImmersiveDarkMode  = 20
    // Windows 11+
    windowCornerPreference = 33
    borderColor           = 34
    captionColor          = 35
    textColor             = 36
    visibleFrameBorderThickness = 37
    systembackdroptype    = 38
}

// ─── DwmSetWindowAttribute DWMWCP_* (corner preference) ──────────
pub enum DwmWindowCornerPreference: u32 {
    default   = 0   // let system choose
    doNotRound = 1
    round     = 2
    roundSmall = 3
}

// ─── DWMSBT_* (system backdrop type, Win 11+) ────────────────────
pub enum DwmSystemBackdropType: u32 {
    auto     = 0
    none     = 1
    mainWindow = 2   // Mica
    transientWindow = 3  // Acrylic
    tabbedWindow    = 4  // Mica Alt
}

// ─── DWM blur-behind flags (DWM_BB_*) ────────────────────────────
@flags
pub enum DwmBlurBehindFlags: u32 {
    enable       = 0x00000001
    blurRegion   = 0x00000002
    transitionOnMaximized = 0x00000004
}

// ─── DWM non-client rendering policy ─────────────────────────────
pub enum DwmNcRenderingPolicy: u32 {
    useWindowStyle = 0   // follow WS_EX_COMPOSITED
    disabled       = 1
    enabled        = 2
}

// ─── DWM Flip3D policy ───────────────────────────────────────────
pub enum DwmFlip3dPolicy: u32 {
    default        = 0
    excludeBelow   = 1
    excludeAbove   = 2
}

// ─── DWM thumbnail flags (DWM_TNP_*) ─────────────────────────────
@flags
pub enum DwmThumbnailFlags: u32 {
    rectdestination = 0x00000001
    rectsource      = 0x00000002
    opacity         = 0x00000004
    visible         = 0x00000008
    sourceclientareaonly = 0x00000010
}

use user32 { HWND }
use gdi32 { SIZE }

@extern(C, "dwmapi") {
    // ── Opaque thumbnail handle ───────────────────────────────────

    /// Opaque handle for a DWM live thumbnail registration.
    @handle pub struct HTHUMBNAIL {}

    // ── MARGINS — 16 bytes ────────────────────────────────────────

    /// Describes glass-frame extension margins (pixels). Pass
    /// `{-1,-1,-1,-1}` to `DwmExtendFrameIntoClientArea` to
    /// render the entire window as glass ("sheet of glass" effect).
    pub struct MARGINS {
        cxLeftWidth:    i32
        cxRightWidth:   i32
        cyTopHeight:    i32
        cyBottomHeight: i32
    }

    // ── DWM_BLURBEHIND — 24 bytes ────────────────────────────────

    /// Passed to `DwmEnableBlurBehindWindow`.
    /// `dwFlags`: DwmBlurBehindFlags.* — which fields are valid.
    /// `hRgnBlur`: clip region for blur (0 as *void = whole window).
    /// `fTransitionOnMaximized`: i32 boolean.
    pub struct DWM_BLURBEHIND {
        dwFlags:               u32
        fEnable:               i32
        hRgnBlur:              *void   // HRGN
        fTransitionOnMaximized: i32
    }

    // ── DWM_THUMBNAIL_PROPERTIES — 52 bytes ─────────────────────

    /// Configures a live thumbnail. Pass to
    /// `DwmUpdateThumbnailProperties`.
    /// `dwFlags`: DwmThumbnailFlags.* — which fields are active.
    pub struct DWM_THUMBNAIL_PROPERTIES {
        dwFlags:             u32
        rcDestination:       i64    // RECT (left,top,right,bottom as i32×4 = 16 bytes)
                                    // stored as i64 pair to keep alignment simple
        rcDestination2:      i64
        rcSource:            i64
        rcSource2:           i64
        opacity:             u8
        fVisible:            i32
        fSourceClientAreaOnly: i32
    }

    // ── Composition ──────────────────────────────────────────────

    /// Returns 1 in `pfEnabled` if DWM composition is enabled.
    /// On Windows 8+ this is always 1.
    @lib pub fn DwmIsCompositionEnabled(pfEnabled: *i32): i32

    /// Forces a DWM frame to be rendered, then waits for it to
    /// appear on screen. Useful for synchronised screen captures.
    @lib pub fn DwmFlush(): i32

    // ── Frame extension ───────────────────────────────────────────

    /// Extends the glass frame into the client area.
    /// Pass `MARGINS {-1,-1,-1,-1}` to paint the entire window as glass.
    @lib pub fn DwmExtendFrameIntoClientArea(
        hwnd:   HWND,
        pMarInset: *const MARGINS
    ): i32

    // ── Window attributes ─────────────────────────────────────────

    /// Sets a DWM attribute for `hwnd`.
    /// `dwAttribute`: DwmWindowAttribute.*, `pvAttribute`: pointer
    /// to the value (typically `*i32` or `*u32`), `cbAttribute`:
    /// size in bytes of that value.
    @lib pub fn DwmSetWindowAttribute(
        hwnd:        HWND,
        dwAttribute: u32,
        pvAttribute: *const void,
        cbAttribute: u32
    ): i32

    /// Reads a DWM attribute into `pvAttribute`.
    @lib pub fn DwmGetWindowAttribute(
        hwnd:        HWND,
        dwAttribute: u32,
        pvAttribute: *void,
        cbAttribute: u32
    ): i32

    // ── Blur behind ───────────────────────────────────────────────

    /// Enables or disables blur-behind for `hwnd`.
    @lib pub fn DwmEnableBlurBehindWindow(
        hwnd:  HWND,
        pBlurBehind: *const DWM_BLURBEHIND
    ): i32

    // ── Live thumbnails ───────────────────────────────────────────

    /// Registers a thumbnail relationship between `hwndDestination`
    /// and `hwndSource`. The handle is returned in `phThumbnailId`.
    @lib pub fn DwmRegisterThumbnail(
        hwndDestination: HWND,
        hwndSource:      HWND,
        phThumbnailId:   *HTHUMBNAIL
    ): i32

    @lib pub fn DwmUnregisterThumbnail(hThumbnailId: HTHUMBNAIL): i32

    @lib pub fn DwmUpdateThumbnailProperties(
        hThumbnailId: HTHUMBNAIL,
        ptnProperties: *const DWM_THUMBNAIL_PROPERTIES
    ): i32

    @lib pub fn DwmQueryThumbnailSourceSize(
        hThumbnail: HTHUMBNAIL,
        pSize:      *SIZE
    ): i32
}
