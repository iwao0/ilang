// uxtheme.dll — Windows Visual Styles ("themes"): draw themed
// controls (buttons, scrollbars, edits, etc.) using the active
// system theme. Use to make custom-drawn controls match the OS look.
//
// Common parts/states are listed in vsstyle.h; pass them as ints
// to DrawThemeBackground / GetThemeColor.

// ─── Theme parts: BUTTON class (BP_*) ────────────────────────────
pub enum ButtonPart: i32 {
    pushButton  = 1
    radioButton = 2
    checkbox    = 3
    groupBox    = 4
    userButton  = 5
}

// ─── BUTTON push-button states (PBS_*) ───────────────────────────
pub enum PushButtonState: i32 {
    normal    = 1
    hot       = 2
    pressed   = 3
    disabled  = 4
    defaulted = 5
}

// ─── Theme parts: EDIT class (EP_*) ──────────────────────────────
pub enum EditPart: i32 {
    editText      = 1
    caret         = 2
    background    = 3
    password      = 4
    backgroundWithBorder = 5
}

// ─── EDIT text states (ETS_*) ────────────────────────────────────
pub enum EditTextState: i32 {
    normal   = 1
    hot      = 2
    selected = 3
    disabled = 4
    focused  = 5
    readonly = 6
    assist   = 7
}

// ─── GetThemeColor color property IDs (TMT_*) ────────────────────
pub enum ThemeColorProp: i32 {
    borderColor    = 3801
    fillColor      = 3802
    textColor      = 3803
    edgeLightColor = 3804
    edgeHighlightColor = 3805
    edgeShadowColor = 3806
    edgeDkShadowColor = 3807
    edgeFillColor  = 3808
    transparentColor = 3809
    gradientColor1 = 3810
    gradientColor2 = 3811
    gradientColor3 = 3812
    gradientColor4 = 3813
    gradientColor5 = 3814
    shadowColor    = 3815
    glowColor      = 3816
    textBorderColor = 3817
    textShadowColor = 3818
    glyphTextColor = 3819
    glyphTransparentColor = 3820
    fillColorHint  = 3821
    borderColorHint = 3822
    accentColorHint = 3823
}

// ─── DrawThemeBackgroundEx flags ─────────────────────────────────
@flags
pub enum DtBgFlags: u32 {
    clipRect           = 0x00000001
    drawSolid          = 0x00000002
    omitBorder         = 0x00000004
    omitContent        = 0x00000008
    computingRegion    = 0x00000010
    mirrorDC           = 0x00000020
    noMirror           = 0x00000040
    validBitsMask      = 0x0000003F
}

// ─── DrawThemeText flags (DTT_*) ─────────────────────────────────
@flags
pub enum DttFlags: u32 {
    textColor   = 0x00000001
    borderColor = 0x00000002
    shadowColor = 0x00000004
    shadowType  = 0x00000008
    shadowOffset = 0x00000010
    borderSize  = 0x00000020
    fontProp    = 0x00000040
    colorProp   = 0x00000080
    stateId     = 0x00000100
    calcRect    = 0x00000200
    applyOverlay = 0x00000400
    glowSize    = 0x00000800
    callback    = 0x00001000
    composited  = 0x00002000
}

@extern(C, "uxtheme") {
    // ── Theme handle ──────────────────────────────────────────────

    /// Opaque theme-data handle (HTHEME).
    pub struct HTHEME {}

    // ── DTTOPTS for DrawThemeTextEx — 76 bytes ───────────────────

    pub struct DTTOPTS {
        dwSize:        u32
        dwFlags:       u32
        crText:        u32
        crBorder:      u32
        crShadow:      u32
        iTextShadowType: i32
        ptShadowOffset_x: i32   // POINT inline
        ptShadowOffset_y: i32
        iBorderSize:   i32
        iFontPropId:   i32
        iColorPropId:  i32
        iStateId:      i32
        fApplyOverlay: i32
        iGlowSize:     i32
        pfnDrawTextCallback: *void
        lParam:        i64
    }

    // ── Open / close ──────────────────────────────────────────────

    /// Opens a theme handle for the given class name (e.g. "BUTTON",
    /// "EDIT", "SCROLLBAR", "HEADER", "TAB"). Returns NULL if themes
    /// are disabled or the class is unknown.
    @lib pub fn OpenThemeData(hwnd: i64, pszClassList: *const u16): *HTHEME

    @lib pub fn CloseThemeData(hTheme: *HTHEME): i32

    /// Re-opens the theme for an HWND after the OS theme changed
    /// (call from WM_THEMECHANGED).
    @lib pub fn OpenThemeDataEx(hwnd: i64, pszClassList: *const u16, dwFlags: u32): *HTHEME

    // ── Drawing ───────────────────────────────────────────────────

    /// Draws the themed background of a part/state into `pRect`.
    /// `pClipRect`: NULL to use the full DC.
    @lib pub fn DrawThemeBackground(
        hTheme:    *HTHEME,
        hdc:       *void,
        iPartId:   i32,
        iStateId:  i32,
        pRect:     *const void,    // *RECT
        pClipRect: *const void
    ): i32

    /// Extended variant with explicit options (DtBgFlags.*).
    @lib pub fn DrawThemeBackgroundEx(
        hTheme:    *HTHEME,
        hdc:       *void,
        iPartId:   i32,
        iStateId:  i32,
        pRect:     *const void,
        pOptions:  *const void     // *DTBGOPTS
    ): i32

    /// Draws themed text. `dwTextFlags`: DT_* (gdi DrawTextFormat),
    /// `dwTextFlags2`: typically 0.
    @lib pub fn DrawThemeText(
        hTheme:        *HTHEME,
        hdc:           *void,
        iPartId:       i32,
        iStateId:      i32,
        pszText:       *const u16,
        cchText:       i32,
        dwTextFlags:   u32,
        dwTextFlags2:  u32,
        pRect:         *const void
    ): i32

    /// Extended text with shadow/glow/color overrides (DTTOPTS).
    @lib pub fn DrawThemeTextEx(
        hTheme:    *HTHEME,
        hdc:       *void,
        iPartId:   i32,
        iStateId:  i32,
        pszText:   *const u16,
        iCharCount: i32,
        dwTextFlags: u32,
        pRect:     *void,
        pOptions:  *const DTTOPTS
    ): i32

    // ── Geometry helpers ──────────────────────────────────────────

    /// Returns the rect that the part would draw inside `pRect`.
    @lib pub fn GetThemeBackgroundContentRect(
        hTheme:    *HTHEME,
        hdc:       *void,
        iPartId:   i32,
        iStateId:  i32,
        pBoundingRect: *const void,
        pContentRect:  *void
    ): i32

    /// Returns the rect of the themed background surrounding `pRect`.
    @lib pub fn GetThemeBackgroundExtent(
        hTheme:    *HTHEME,
        hdc:       *void,
        iPartId:   i32,
        iStateId:  i32,
        pContentRect: *const void,
        pExtentRect:  *void
    ): i32

    /// Returns the natural size of a part. `eSize`: 0=min, 1=true, 2=draw.
    @lib pub fn GetThemePartSize(
        hTheme:    *HTHEME,
        hdc:       *void,
        iPartId:   i32,
        iStateId:  i32,
        prc:       *const void,
        eSize:     i32,
        psz:       *void      // *SIZE
    ): i32

    // ── Properties ────────────────────────────────────────────────

    /// Returns a COLORREF for the given color property.
    @lib pub fn GetThemeColor(
        hTheme:    *HTHEME,
        iPartId:   i32,
        iStateId:  i32,
        iPropId:   i32,
        pColor:    *u32
    ): i32

    @lib pub fn GetThemeFont(
        hTheme:    *HTHEME,
        hdc:       *void,
        iPartId:   i32,
        iStateId:  i32,
        iPropId:   i32,
        pFont:     *void      // *LOGFONTW (92 bytes)
    ): i32

    @lib pub fn GetThemeMargins(
        hTheme:    *HTHEME,
        hdc:       *void,
        iPartId:   i32,
        iStateId:  i32,
        iPropId:   i32,
        prc:       *const void,
        pMargins:  *void      // *MARGINS (16 bytes)
    ): i32

    @lib pub fn GetThemeInt(
        hTheme:    *HTHEME,
        iPartId:   i32,
        iStateId:  i32,
        iPropId:   i32,
        piVal:     *i32
    ): i32

    @lib pub fn GetThemeString(
        hTheme:    *HTHEME,
        iPartId:   i32,
        iStateId:  i32,
        iPropId:   i32,
        pszBuff:   *u16,
        cchMaxBuffChars: i32
    ): i32

    @lib pub fn GetThemeMetric(
        hTheme:    *HTHEME,
        hdc:       *void,
        iPartId:   i32,
        iStateId:  i32,
        iPropId:   i32,
        piVal:     *i32
    ): i32

    @lib pub fn GetThemePosition(
        hTheme:    *HTHEME,
        iPartId:   i32,
        iStateId:  i32,
        iPropId:   i32,
        pPoint:    *void      // *POINT
    ): i32

    @lib pub fn GetThemeRect(
        hTheme:    *HTHEME,
        iPartId:   i32,
        iStateId:  i32,
        iPropId:   i32,
        pRect:     *void
    ): i32

    @lib pub fn GetThemeBool(
        hTheme:    *HTHEME,
        iPartId:   i32,
        iStateId:  i32,
        iPropId:   i32,
        pfVal:     *i32
    ): i32

    // ── State / queries ──────────────────────────────────────────

    /// Returns non-zero if themes are currently enabled.
    @lib pub fn IsThemeActive(): i32

    /// Returns non-zero if the application is themed (not opted out).
    @lib pub fn IsAppThemed(): i32

    /// Returns non-zero if the part has any transparent or alpha pixels.
    @lib pub fn IsThemeBackgroundPartiallyTransparent(
        hTheme:    *HTHEME,
        iPartId:   i32,
        iStateId:  i32
    ): i32

    // ── Window theming ────────────────────────────────────────────

    /// Sets a window's theme name override (e.g. L"Explorer"
    /// for the modern list/tree look). NULL to clear.
    @lib pub fn SetWindowTheme(
        hwnd:           i64,
        pszSubAppName:  *const u16,
        pszSubIdList:   *const u16
    ): i32

    /// Reapplies the active theme to `hwnd` (call from WM_THEMECHANGED).
    @lib pub fn EnableThemeDialogTexture(hwnd: i64, dwFlags: u32): i32
    @lib pub fn EnableTheming(fEnable: i32): i32

    /// Returns the file system path to the .msstyles file currently active.
    @lib pub fn GetCurrentThemeName(
        pszThemeFileName: *u16,
        cchMaxNameChars:  i32,
        pszColorBuff:     *u16,
        cchMaxColorChars: i32,
        pszSizeBuff:      *u16,
        cchMaxSizeChars:  i32
    ): i32

    // ── Buffered painting (smooth flicker-free drawing) ──────────

    /// Begins double-buffered painting. Returns a handle to be passed
    /// to EndBufferedPaint / BufferedPaintClear.  `phdc`: receives an
    /// HDC for the off-screen buffer.
    @lib pub fn BeginBufferedPaint(
        hdcTarget: *void,
        prcTarget: *const void,
        dwFormat:  u32,
        pPaintParams: *const void,
        phdc:      **void
    ): *void

    /// `fUpdateTarget`: 1 = copy buffer to target, 0 = discard.
    @lib pub fn EndBufferedPaint(
        hBufferedPaint: *void,
        fUpdateTarget:  i32
    ): i32

    @lib pub fn BufferedPaintInit(): i32
    @lib pub fn BufferedPaintUnInit(): i32
}
