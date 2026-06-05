/// gdi32.dll — GDI drawing: device contexts, pens, brushes, fonts,
/// bitmaps, regions, paths, and coordinate transforms.
///
/// All handles below are opaque empty structs; cast integers to them
/// when using stock-object IDs.  The Win64 ABI treats `*T` and `i64`
/// identically in registers, so unsafe casts are safe at the boundary.

// ─── Graphics mode (SetGraphicsMode) ─────────────────────────────
pub enum GraphicsMode: i32 {
    compatible = 1
    advanced   = 2
}

// ─── World-transform modify mode (MWT_*) ─────────────────────────
pub enum WorldTransformMode: u32 {
    identity     = 1
    leftMultiply = 2
    rightMultiply = 3
}

// ─── Polygon fill mode (SetPolyFillMode) ─────────────────────────
pub enum PolyFillMode: i32 {
    alternate = 1
    winding   = 2
}

// ─── Binary raster ops (SetROP2) ─────────────────────────────────
pub enum ROP2: i32 {
    black        = 1
    notMergePen  = 2
    maskNotPen   = 3
    notCopyPen   = 4
    maskPenNot   = 5
    not          = 6
    xorPen       = 7
    notMaskPen   = 8
    maskPen      = 9
    notXorPen    = 10
    nop          = 11
    mergeNotPen  = 12
    copyPen      = 13
    mergePenNot  = 14
    mergePen     = 15
    white        = 16
}

// ─── PolyDraw point types (PT_*) ─────────────────────────────────
@flags
pub enum PolyDrawType: u8 {
    moveTo     = 0x06
    lineTo     = 0x02
    bezierTo   = 0x04
    closeFigure = 0x01
}

// ─── DIB compression (BI_*) ──────────────────────────────────────
pub enum BitmapCompression: u32 {
    rgb       = 0
    rle8      = 1
    rle4      = 2
    bitfields = 3
    jpeg      = 4
    png       = 5
}

// ─── ICM mode (SetICMMode) ───────────────────────────────────────
pub enum IcmMode: i32 {
    off            = 1
    on             = 2
    query          = 3
    doneOutsidedc  = 4
}

// ─── GDI device cap indices (GetDeviceCaps) ──────────────────────
pub enum DeviceCap: i32 {
    driverVersion     = 0
    technology        = 2
    horzSize          = 4    // mm
    vertSize          = 6
    horzRes           = 8    // pixels
    vertRes           = 10
    bitsPixel         = 12
    planes            = 14
    numBrushes        = 16
    numPens           = 18
    numMarkers        = 20
    numFonts          = 22
    numColors         = 24
    pdeviceSize       = 26
    curveCaps         = 28
    lineCaps          = 30
    polygonalCaps     = 32
    textCaps          = 34
    clipCaps          = 36
    rasterCaps        = 38
    aspectX           = 40
    aspectY           = 42
    aspectXY          = 44
    logPixelsX        = 88
    logPixelsY        = 90
    sizePalette       = 104
    numReserved       = 106
    colorRes          = 108
    physicalWidth     = 110
    physicalHeight    = 111
    physicalOffsetX   = 112
    physicalOffsetY   = 113
    scalingFactorX    = 114
    scalingFactorY    = 115
    vRefresh          = 116
    desktopVertRes    = 117
    desktopHorzRes    = 118
    bltAlignment      = 119
    shadeBlendCaps    = 120
    colorMgmtCaps     = 121
}

// ─── Object types returned by GetObjectType ──────────────────────
pub enum GdiObjectType: u32 {
    pen           = 1
    brush         = 2
    dc            = 3
    metadc        = 4
    pal           = 5
    font          = 6
    bitmap        = 7
    region        = 8
    metafile      = 9
    memdc         = 10
    extpen        = 11
    enhmetadc     = 12
    enhmetafile   = 13
    colorspace    = 14
}

// ─── Region fill mode (PaintRgn etc.) is implicit; no enum ───────

// ─── ExtCreatePen styles (PS_* extended) ─────────────────────────
@flags
pub enum PenStyleEx: u32 {
    geometric   = 0x00010000
    cosmetic    = 0x00000000
    endcapRound = 0x00000000
    endcapSquare = 0x00000100
    endcapFlat  = 0x00000200
    joinRound   = 0x00000000
    joinBevel   = 0x00001000
    joinMiter   = 0x00002000
    typeMask    = 0x000F0000
    styleMask   = 0x0000000F
    endcapMask  = 0x00000F00
    joinMask    = 0x0000F000
}

use winnt { HANDLE }
use windef { HDC, HWND, HBRUSH, HBITMAP, HFONT, HPEN, HRGN, HPALETTE, HGDIOBJ, RECT, POINT, SIZE }

// ─── Raster-operation codes (SRCCOPY etc.) ───────────────────────
pub enum RasterOp: u32 {
    blackness   = 0x00000042   // dest = BLACK
    noMirrorBitmap = 0x80000000
    captureBlt  = 0x40000000
    dstInvert   = 0x00550009   // dest = NOT dest
    mergeCopy   = 0x00C000CA   // dest = src AND pat
    mergePaint  = 0x00BB0226   // dest = NOT src OR dest
    notsrccopy  = 0x00330008   // dest = NOT src
    notsrcerase = 0x001100A6   // dest = NOT(src OR dest)
    patCopy     = 0x00F00021   // dest = pat
    patInvert   = 0x005A0049   // dest = pat XOR dest
    patPaint    = 0x00FB0A09   // dest = pat OR NOT src OR dest
    srcAnd      = 0x008800C6   // dest = src AND dest
    srcCopy     = 0x00CC0020   // dest = src  (most common)
    srcErase    = 0x00440328   // dest = src AND NOT dest
    srcInvert   = 0x00660046   // dest = src XOR dest
    srcPaint    = 0x00EE0086   // dest = src OR dest
    whiteness   = 0x00FF0062   // dest = WHITE
}

// ─── Stock GDI objects (GetStockObject) ──────────────────────────
pub enum StockObject: i32 {
    whiteBrush    = 0
    ltGrayBrush   = 1
    grayBrush     = 2
    dkGrayBrush   = 3
    blackBrush    = 4
    nullBrush     = 5    // HOLLOW_BRUSH
    whitePen      = 6
    blackPen      = 7
    nullPen       = 8
    oemFixedFont  = 10
    ansiFixedFont = 11
    ansiVarFont   = 12
    systemFont    = 13
    deviceDefaultFont = 14
    defaultPalette    = 15
    systemFixedFont   = 16
    defaultGuiFont    = 17
    dcBrush       = 18
    dcPen         = 19
}

// ─── Pen styles (PS_*) ───────────────────────────────────────────
pub enum PenStyle: i32 {
    solid       = 0
    dash        = 1
    dot         = 2
    dashDot     = 3
    dashDotDot  = 4
    null        = 5
    insideFrame = 6
}

// ─── Hatch-brush styles (HS_*) ───────────────────────────────────
pub enum HatchStyle: i32 {
    horizontal = 0   // -----
    vertical   = 1   // |||||
    fDiagonal  = 2   // \\\\\
    bDiagonal  = 3   // /////
    cross      = 4   // +++++
    diagCross  = 5   // xxxxx
}

// ─── Background mix mode (OPAQUE / TRANSPARENT) ──────────────────
pub enum BackgroundMode: i32 {
    transparent = 1
    opaque      = 2
}

// ─── Text alignment (TA_*) ───────────────────────────────────────
@flags
pub enum TextAlign: u32 {
    noUpdateCP = 0x0000
    updateCP   = 0x0001
    left       = 0x0000
    right      = 0x0002
    center     = 0x0006
    top        = 0x0000
    bottom     = 0x0008
    baseline   = 0x0018
    rtlReading = 0x0100
}

// ─── DrawText format flags (DT_*) ────────────────────────────────
@flags
pub enum DrawTextFormat: u32 {
    top              = 0x00000000
    left             = 0x00000000
    center           = 0x00000001
    right            = 0x00000002
    vCenter          = 0x00000004
    bottom           = 0x00000008
    wordBreak        = 0x00000010
    singleLine       = 0x00000020
    expandTabs       = 0x00000040
    tabStop          = 0x00000080
    noClip           = 0x00000100
    externalLeading  = 0x00000200
    calcrect         = 0x00000400
    noPrefix         = 0x00000800
    internal         = 0x00001000
    editControl      = 0x00002000
    pathEllipsis     = 0x00004000
    endEllipsis      = 0x00008000
    modifyString     = 0x00010000
    rtlReading       = 0x00020000
    wordEllipsis     = 0x00040000
    hidePrefix       = 0x00100000
    prefixOnly       = 0x00200000
}

// ─── StretchBlt mode (COLORONCOLOR etc.) ─────────────────────────
pub enum StretchBltMode: i32 {
    blackonwhite = 1   // STRETCH_ANDSCANS
    whiteonblack = 2   // STRETCH_ORSCANS
    colorOnColor = 3   // STRETCH_DELETESCANS (default; fast)
    halftone     = 4   // STRETCH_HALFTONE (best quality)
}

// ─── Map mode (MM_*) ─────────────────────────────────────────────
pub enum MapMode: i32 {
    text        = 1   // 1 logical unit = 1 device pixel (default)
    lometric    = 2   // 1 unit = 0.1 mm
    himetric    = 3   // 1 unit = 0.01 mm
    loenglish   = 4   // 1 unit = 0.01 inch
    hienglish   = 5   // 1 unit = 0.001 inch
    twips       = 6   // 1 unit = 1/1440 inch
    isotropic   = 7   // programmer-defined, equal axes
    anisotropic = 8   // programmer-defined, independent axes
}

// ─── CombineRgn / SelectClipRgn mode (RGN_*) ────────────────────
pub enum CombineRgnMode: i32 {
    and     = 1
    or      = 2
    xor     = 3
    diff    = 4
    copy    = 5
}

// ─── CombineRgn return values ────────────────────────────────────
pub enum RegionResult: i32 {
    error    = 0
    nullRgn  = 1
    simpleRgn = 2
    complexRgn = 3
}

// ─── DIB colour table usage (DIB_*) ─────────────────────────────
pub enum DIBColorMode: u32 {
    rgbColors = 0   // colour table has literal RGB values
    palColors = 1   // colour table has palette indices
}

// ─── Font weight (FW_*) ──────────────────────────────────────────
pub enum FontWeight: i32 {
    dontCare   = 0
    thin       = 100
    extraLight = 200   // FW_ULTRALIGHT
    light      = 300
    normal     = 400   // FW_REGULAR
    medium     = 500
    semiBold   = 600   // FW_DEMIBOLD
    bold       = 700
    extraBold  = 800   // FW_ULTRABOLD
    heavy      = 900   // FW_BLACK
}

// ─── Font charset (ANSI_CHARSET etc.) ────────────────────────────
pub enum FontCharset: u32 {
    ansi        = 0
    default     = 1
    symbol      = 2
    shiftjis    = 128
    hangeul     = 129
    gb2312      = 134
    chineseBig5 = 136
    oem         = 255
    johab       = 130
    hebrew      = 177
    arabic      = 178
    greek       = 161
    turkish     = 162
    vietnamese  = 163
    thai        = 222
    eastEurope  = 238
    russian     = 204
    mac         = 77
    baltic      = 186
}

// ─── Font output precision (OUT_*_PRECIS) ────────────────────────
pub enum FontOutPrecision: u32 {
    default       = 0
    string        = 1
    character     = 2
    stroke        = 3
    truetypeOnly  = 7
    outline       = 8
    screenOutline = 9
    postscriptOnly = 10
}

// ─── Font clip precision (CLIP_*) ────────────────────────────────
pub enum FontClipPrecision: u32 {
    default       = 0
    character     = 1
    stroke        = 2
    mask          = 0xF
    ltbStroke     = 0x10
    truetypeAlways = 0x20
    dfa_disable   = 0x40
    embedded      = 0x80
}

// ─── Font quality (NONANTIALIASED_QUALITY etc.) ──────────────────
pub enum FontQuality: u32 {
    default        = 0
    draft          = 1
    proof          = 2
    nonantialiased = 3
    antialiased    = 4
    cleartype      = 5
    cleartypeNatural = 6
}

// ─── Font pitch and family ────────────────────────────────────────
pub enum FontPitchFamily: u32 {
    defaultPitch  = 0x00
    fixedPitch    = 0x01
    variablePitch = 0x02
    ffDontCare    = 0x00
    ffRoman       = 0x10
    ffSwiss       = 0x20
    ffModern      = 0x30
    ffScript      = 0x40
    ffDecorative  = 0x50
}

@extern(C, "gdi32") {
    // ── Font / text structs ───────────────────────────────────────

    /// `LOGFONTA` — 60 bytes. Describes a logical font.
    /// `lfFaceName` is 32 bytes (LF_FACESIZE=32), stored as 8 × u32
    /// for alignment; cast to `*const char` when passing to
    /// `CreateFontIndirectA` after filling with a C-string.
    pub struct LOGFONTA {
        lfHeight:         i32
        lfWidth:          i32
        lfEscapement:     i32
        lfOrientation:    i32
        lfWeight:         i32
        lfItalic:         u8
        lfUnderline:      u8
        lfStrikeOut:      u8
        lfCharSet:        u8
        lfOutPrecision:   u8
        lfClipPrecision:  u8
        lfQuality:        u8
        lfPitchAndFamily: u8
        lfFaceName:       u8[32]   // LF_FACESIZE = 32
    }

    /// `LOGFONTW` — 92 bytes. Wide-string twin of LOGFONTA: same
    /// numeric fields, but `lfFaceName` is UTF-16 (LF_FACESIZE = 32 wchars).
    pub struct LOGFONTW {
        lfHeight:         i32
        lfWidth:          i32
        lfEscapement:     i32
        lfOrientation:    i32
        lfWeight:         i32
        lfItalic:         u8
        lfUnderline:      u8
        lfStrikeOut:      u8
        lfCharSet:        u8
        lfOutPrecision:   u8
        lfClipPrecision:  u8
        lfQuality:        u8
        lfPitchAndFamily: u8
        lfFaceName:       u16[32]
    }

    /// `TEXTMETRICA` — 60 bytes. Returned by `GetTextMetricsA`.
    pub struct TEXTMETRICA {
        tmHeight:           i32
        tmAscent:           i32
        tmDescent:          i32
        tmInternalLeading:  i32
        tmExternalLeading:  i32
        tmAveCharWidth:     i32
        tmMaxCharWidth:     i32
        tmWeight:           i32
        tmOverhang:         i32
        tmDigitizedAspectX: i32
        tmDigitizedAspectY: i32
        tmFirstChar:        u8
        tmLastChar:         u8
        tmDefaultChar:      u8
        tmBreakChar:        u8
        tmItalic:           u8
        tmUnderlined:       u8
        tmStruckOut:        u8
        tmPitchAndFamily:   u8
        tmCharSet:          u8
        // 3 bytes padding, then 3 DWORD fields
        _pad0: u8
        _pad1: u8
        _pad2: u8
        tmOverhangReserved: i32
        tmAveCharWidthReserved: i32
        tmMaxCharWidthReserved: i32
    }

    // ── Bitmap structs ────────────────────────────────────────────

    /// `BITMAP` — 32 bytes. Describes a device-dependent bitmap.
    pub struct BITMAP {
        bmType:       i32
        bmWidth:      i32
        bmHeight:     i32
        bmWidthBytes: i32
        bmPlanes:     u16
        bmBitsPixel:  u16
        // 4 bytes implicit padding before pointer
        bmBits:       *void
    }

    /// `BITMAPINFOHEADER` — 40 bytes. DIB header.
    pub struct BITMAPINFOHEADER {
        biSize:          u32
        biWidth:         i32
        biHeight:        i32
        biPlanes:        u16
        biBitCount:      u16
        biCompression:   u32
        biSizeImage:     u32
        biXPelsPerMeter: i32
        biYPelsPerMeter: i32
        biClrUsed:       u32
        biClrImportant:  u32
    }

    /// `RGBQUAD` — 4 bytes. One entry in a DIB colour table.
    pub struct RGBQUAD {
        rgbBlue:     u8
        rgbGreen:    u8
        rgbRed:      u8
        rgbReserved: u8
    }

    // ── Device context ────────────────────────────────────────────

    /// Creates an in-memory DC compatible with `hdc`.
    /// Pass `0 as HDC` to create a screen-compatible DC.
    @lib pub fn CreateCompatibleDC(hdc: HDC): HDC

    @lib pub fn DeleteDC(hdc: HDC): i32

    /// Saves the current state of `hdc` and returns a save-state
    /// index. Restored with `RestoreDC(hdc, savedIndex)` or
    /// `RestoreDC(hdc, -1)` to restore the most recent save.
    @lib pub fn SaveDC(hdc: HDC): i32
    @lib pub fn RestoreDC(hdc: HDC, nSavedDC: i32): i32

    // ── Object selection / deletion ───────────────────────────────

    /// Selects `hObject` into `hdc`, returning the previously
    /// selected object of the same type (or NULL on error).
    /// Returned HGDIOBJ must be selected back before DeleteObject.
    @lib pub fn SelectObject(hdc: HDC, hObject: HGDIOBJ): HGDIOBJ

    /// Deletes a GDI object (pen, brush, font, bitmap, region,
    /// palette). Never delete an object while it is selected.
    @lib pub fn DeleteObject(hObject: HGDIOBJ): i32

    /// Returns a predefined stock GDI object. Cast result to the
    /// appropriate handle type.
    @lib pub fn GetStockObject(fnObject: i32): HGDIOBJ

    /// Fills `lpObject` with data about a GDI object. `nCount` is
    /// the byte size of the buffer (e.g. sizeof LOGFONTA for fonts).
    @lib pub fn GetObjectA(hObject: HGDIOBJ, nCount: i32, lpObject: *void): i32
    @lib pub fn GetObjectW(hObject: HGDIOBJ, nCount: i32, lpObject: *void): i32

    // ── Pen ───────────────────────────────────────────────────────

    /// Creates a cosmetic pen. `iStyle`: PenStyle.*, `cWidth`: pixel
    /// width (0 = 1 px), `color`: COLORREF (0x00BBGGRR).
    @lib pub fn CreatePen(iStyle: i32, cWidth: i32, color: u32): HPEN

    // ── Brush ─────────────────────────────────────────────────────

    /// Creates a solid-color brush. `color`: COLORREF (0x00BBGGRR).
    @lib pub fn CreateSolidBrush(color: u32): HBRUSH

    /// Creates a hatch brush. `iHatch`: HatchStyle.*, `color`: COLORREF.
    @lib pub fn CreateHatchBrush(iHatch: i32, color: u32): HBRUSH

    /// Creates a pattern brush from a bitmap handle.
    @lib pub fn CreatePatternBrush(hbm: HBITMAP): HBRUSH

    // ── Color / background ────────────────────────────────────────

    /// Sets the foreground text color. Returns the previous value.
    @lib pub fn SetTextColor(hdc: HDC, color: u32): u32
    @lib pub fn GetTextColor(hdc: HDC): u32

    /// Sets the background color used to fill gaps in hatched brushes,
    /// character cells, and dashed lines.
    @lib pub fn SetBkColor(hdc: HDC, color: u32): u32
    @lib pub fn GetBkColor(hdc: HDC): u32

    /// Sets background mix mode (BackgroundMode.transparent or .opaque).
    @lib pub fn SetBkMode(hdc: HDC, iBkMode: i32): i32
    @lib pub fn GetBkMode(hdc: HDC): i32

    // ── Drawing ───────────────────────────────────────────────────

    /// Moves the current position to `(X, Y)`. `lpPoint` receives
    /// the previous position; pass `0 as *void` to discard.
    @lib pub fn MoveToEx(hdc: HDC, X: i32, Y: i32, lpPoint: *POINT): i32

    /// Draws a line from the current position to `(nXEnd, nYEnd)`,
    /// then updates the current position to `(nXEnd, nYEnd)`.
    @lib pub fn LineTo(hdc: HDC, nXEnd: i32, nYEnd: i32): i32

    /// Draws a filled rectangle using the current pen and brush.
    @lib pub fn Rectangle(hdc: HDC, left: i32, top: i32, right: i32, bottom: i32): i32

    /// Draws a filled ellipse inscribed in the given bounding rect.
    @lib pub fn Ellipse(hdc: HDC, left: i32, top: i32, right: i32, bottom: i32): i32

    /// Draws a filled rectangle with rounded corners.
    @lib pub fn RoundRect(
        hdc: HDC,
        left: i32, top: i32, right: i32, bottom: i32,
        width: i32, height: i32
    ): i32

    /// Draws a filled polygon through `cPoints` vertices in `lpPoints`.
    /// `lpPoints` is a pointer to an array of POINT structs.
    @lib pub fn Polygon(hdc: HDC, lpPoints: *POINT, cPoints: i32): i32

    /// Draws connected line segments through `cPoints` vertices.
    @lib pub fn Polyline(hdc: HDC, lpPoints: *POINT, cPoints: i32): i32

    /// Draws an arc. `(nXStartArc,nYStartArc)` and `(nXEndArc,nYEndArc)`
    /// are radial endpoints of the arc (not on the ellipse itself).
    @lib pub fn Arc(
        hdc: HDC,
        left: i32, top: i32, right: i32, bottom: i32,
        nXStartArc: i32, nYStartArc: i32,
        nXEndArc:   i32, nYEndArc:   i32
    ): i32

    /// Sets a single pixel to `color` (COLORREF). Returns the
    /// previous color at that position.
    @lib pub fn SetPixel(hdc: HDC, X: i32, Y: i32, color: u32): u32

    /// Returns the COLORREF of the pixel at `(X, Y)`, or
    /// `0xFFFFFFFF` (CLR_INVALID) on error.
    @lib pub fn GetPixel(hdc: HDC, X: i32, Y: i32): u32

    // ── Text output ───────────────────────────────────────────────

    /// Writes `cbString` bytes of `lpString` at `(nXStart, nYStart)`.
    @lib pub fn TextOutA(
        hdc:      HDC,
        nXStart:  i32,
        nYStart:  i32,
        lpString: *const char,
        cbString: i32
    ): i32
    @lib pub fn TextOutW(
        hdc:      HDC,
        nXStart:  i32,
        nYStart:  i32,
        lpString: *const u16,
        cbString: i32
    ): i32

    /// Draws formatted text within `lpRect`. `nCount` = −1 to use
    /// the NUL-terminated length. Returns height of the text in
    /// logical units, or 0 on error.
    @lib pub fn DrawTextA(
        hdc:     HDC,
        lpchText: *const char,
        nCount:  i32,
        lpRect:  *RECT,
        uFormat: u32
    ): i32
    @lib pub fn DrawTextW(
        hdc:     HDC,
        lpchText: *const u16,
        nCount:  i32,
        lpRect:  *RECT,
        uFormat: u32
    ): i32

    /// Fills `lpSize` with the width and height of `lpString` using
    /// the DC's current font.
    @lib pub fn GetTextExtentPoint32A(
        hdc:      HDC,
        lpString: *const char,
        cbString: i32,
        lpSize:   *SIZE
    ): i32
    @lib pub fn GetTextExtentPoint32W(
        hdc:      HDC,
        lpString: *const u16,
        cbString: i32,
        lpSize:   *SIZE
    ): i32

    @lib pub fn SetTextAlign(hdc: HDC, fMode: u32): u32
    @lib pub fn GetTextAlign(hdc: HDC): u32

    /// Fills `lptm` with font metrics for the DC's current font.
    @lib pub fn GetTextMetricsA(hdc: HDC, lptm: *TEXTMETRICA): i32
    @lib pub fn GetTextMetricsW(hdc: HDC, lptm: *TEXTMETRICA): i32

    // ── Font ──────────────────────────────────────────────────────

    /// Creates a logical font from explicit parameters. Most
    /// callers prefer `CreateFontIndirectA` with a filled LOGFONTA.
    /// `HFONT` is `@handle` (pointer-sized opaque) — returned by
    /// value, not as `*HFONT`.
    @lib pub fn CreateFontA(
        nHeight:          i32,
        nWidth:           i32,
        nEscapement:      i32,
        nOrientation:     i32,
        fnWeight:         i32,
        fdwItalic:        u32,
        fdwUnderline:     u32,
        fdwStrikeOut:     u32,
        fdwCharSet:       u32,
        fdwOutputPrecision: u32,
        fdwClipPrecision: u32,
        fdwQuality:       u32,
        fdwPitchAndFamily: u32,
        lpszFace:         *const char
    ): HFONT
    @lib pub fn CreateFontW(
        nHeight:          i32,
        nWidth:           i32,
        nEscapement:      i32,
        nOrientation:     i32,
        fnWeight:         i32,
        fdwItalic:        u32,
        fdwUnderline:     u32,
        fdwStrikeOut:     u32,
        fdwCharSet:       u32,
        fdwOutputPrecision: u32,
        fdwClipPrecision: u32,
        fdwQuality:       u32,
        fdwPitchAndFamily: u32,
        lpszFace:         *const u16
    ): HFONT

    @lib pub fn CreateFontIndirectA(lplf: *const LOGFONTA): HFONT
    @lib pub fn CreateFontIndirectW(lplf: *const LOGFONTA): HFONT

    // ── Bitmap ────────────────────────────────────────────────────

    /// Creates a device-compatible bitmap with the same colour format
    /// as `hdc`, sized `nWidth` × `nHeight` pixels.
    @lib pub fn CreateCompatibleBitmap(hdc: HDC, nWidth: i32, nHeight: i32): HBITMAP

    /// Creates a DDB with explicitly specified planes and bits-per-pixel.
    @lib pub fn CreateBitmap(
        nWidth:     i32,
        nHeight:    i32,
        nPlanes:    u32,
        nBitCount:  u32,
        lpBits:     *const void
    ): HBITMAP

    /// Block-transfers `nWidth`×`nHeight` pixels from `hdcSrc`
    /// to `hdcDest`. `dwRop`: RasterOp.srcCopy for a plain copy.
    @lib pub fn BitBlt(
        hdcDest: HDC,
        nXDest: i32, nYDest: i32,
        nWidth: i32, nHeight: i32,
        hdcSrc: HDC,
        nXSrc: i32, nYSrc: i32,
        dwRop: u32
    ): i32

    /// Block-transfers with scaling. `iStretchMode`: StretchBltMode.*.
    @lib pub fn StretchBlt(
        hdcDest: HDC,
        nXOriginDest: i32, nYOriginDest: i32,
        nWidthDest: i32, nHeightDest: i32,
        hdcSrc: HDC,
        nXOriginSrc: i32, nYOriginSrc: i32,
        nWidthSrc: i32, nHeightSrc: i32,
        dwRop: u32
    ): i32

    @lib pub fn SetStretchBltMode(hdc: HDC, iStretchMode: i32): i32

    // ── Region ────────────────────────────────────────────────────

    @lib pub fn CreateRectRgn(left: i32, top: i32, right: i32, bottom: i32): HRGN
    @lib pub fn CreateEllipticRgn(left: i32, top: i32, right: i32, bottom: i32): HRGN
    @lib pub fn CreateRoundRectRgn(
        left: i32, top: i32, right: i32, bottom: i32,
        widthEllipse: i32, heightEllipse: i32
    ): HRGN
    @lib pub fn CreatePolygonRgn(lpPoints: *POINT, nCount: i32, fnPolyFillMode: i32): HRGN

    /// Combines `hrgnSrc1` and `hrgnSrc2` into `hrgnDest` using
    /// `fnCombineMode` (CombineRgnMode.*). Returns RegionResult.*.
    @lib pub fn CombineRgn(hrgnDest: HRGN, hrgnSrc1: HRGN, hrgnSrc2: HRGN, fnCombineMode: i32): i32

    /// Replaces the DC clipping region with `hRgn`.
    /// Pass `0 as HRGN` to remove clipping.
    @lib pub fn SelectClipRgn(hdc: HDC, hRgn: HRGN): i32

    @lib pub fn OffsetRgn(hRgn: HRGN, nXOffset: i32, nYOffset: i32): i32

    // ── Path ──────────────────────────────────────────────────────

    @lib pub fn BeginPath(hdc: HDC): i32
    @lib pub fn EndPath(hdc: HDC): i32
    @lib pub fn StrokePath(hdc: HDC): i32
    @lib pub fn FillPath(hdc: HDC): i32
    @lib pub fn StrokeAndFillPath(hdc: HDC): i32
    @lib pub fn PathToRegion(hdc: HDC): HRGN

    // ── Coordinate transforms ─────────────────────────────────────

    @lib pub fn SetMapMode(hdc: HDC, fnMapMode: i32): i32
    @lib pub fn GetMapMode(hdc: HDC): i32

    @lib pub fn SetViewportOrgEx(hdc: HDC, X: i32, Y: i32, lpPoint: *POINT): i32
    @lib pub fn SetWindowOrgEx(hdc: HDC, X: i32, Y: i32, lpPoint: *POINT): i32
    @lib pub fn SetViewportExtEx(hdc: HDC, nXExtent: i32, nYExtent: i32, lpSize: *SIZE): i32
    @lib pub fn SetWindowExtEx(hdc: HDC, nXExtent: i32, nYExtent: i32, lpSize: *SIZE): i32
    @lib pub fn GetViewportOrgEx(hdc: HDC, lpPoint: *POINT): i32
    @lib pub fn GetWindowOrgEx(hdc: HDC, lpPoint: *POINT): i32

    // ── Device capabilities ───────────────────────────────────────

    /// Returns a capability value for the DC.  Common `nIndex` values:
    ///   HORZRES=8, VERTRES=10, BITSPIXEL=12, PLANES=14,
    ///   LOGPIXELSX=88, LOGPIXELSY=90, SIZEPALETTE=104.
    @lib pub fn GetDeviceCaps(hdc: HDC, nIndex: i32): i32

    // ── Pattern blit ──────────────────────────────────────────────

    /// Fills a rectangle using the current brush and a raster op.
    /// Useful for fast solid fills without creating a RECT.
    @lib pub fn PatBlt(
        hdc:   HDC,
        nXLeft: i32,
        nYLeft: i32,
        nWidth: i32,
        nHeight: i32,
        dwRop: u32     // RasterOp.patCopy for solid fill
    ): i32

    // ── Flood fill ────────────────────────────────────────────────

    /// Flood-fills starting at `(nXStart, nYStart)`.
    /// `fuFillType`: 0=FLOODFILLBORDER (stop at `crColor` boundary),
    ///               1=FLOODFILLSURFACE (fill while pixel == crColor).
    @lib pub fn ExtFloodFill(
        hdc:      HDC,
        nXStart:  i32,
        nYStart:  i32,
        crColor:  u32,
        fuFillType: u32
    ): i32

    // ── Advanced curves ───────────────────────────────────────────

    /// Draws a series of cubic Bézier curves.
    /// `lpPoints`: array of POINT structs; count must be 3n+1.
    @lib pub fn PolyBezier(hdc: HDC, lpPoints: *POINT, cPoints: u32): i32

    /// Draws Bézier segments from current position; count must be 3n.
    @lib pub fn PolyBezierTo(hdc: HDC, lpPoints: *POINT, cCount: u32): i32

    /// Draws connected line segments updating current position.
    @lib pub fn PolylineTo(hdc: HDC, lpPoints: *POINT, cCount: u32): i32

    /// Draws a pie wedge (filled arc + two radii).
    @lib pub fn Pie(
        hdc: HDC,
        left: i32, top: i32, right: i32, bottom: i32,
        xStartArc: i32, yStartArc: i32,
        xEndArc:   i32, yEndArc:   i32
    ): i32

    /// Draws a chord (filled arc + chord line).
    @lib pub fn Chord(
        hdc: HDC,
        left: i32, top: i32, right: i32, bottom: i32,
        xStartArc: i32, yStartArc: i32,
        xEndArc:   i32, yEndArc:   i32
    ): i32

    /// Sets the arc direction: 1=AD_COUNTERCLOCKWISE, 2=AD_CLOCKWISE.
    @lib pub fn SetArcDirection(hdc: HDC, ArcDirection: i32): i32

    // ── Current position ──────────────────────────────────────────

    /// Fills `lpPoint` with the current pen position in logical coords.
    @lib pub fn GetCurrentPositionEx(hdc: HDC, lpPoint: *POINT): i32

    // ── Clip rect helpers ─────────────────────────────────────────

    /// Returns the bounding rect of the current clip region.
    /// Return value: 0=null region, 1=simple, 2=complex.
    @lib pub fn GetClipBox(hdc: HDC, lpRect: *RECT): i32

    /// Subtracts a rect from the current clip region.
    @lib pub fn ExcludeClipRect(
        hdc: HDC, left: i32, top: i32, right: i32, bottom: i32
    ): i32

    /// Intersects the current clip region with a rect.
    @lib pub fn IntersectClipRect(
        hdc: HDC, left: i32, top: i32, right: i32, bottom: i32
    ): i32

    // ── DIB sections ──────────────────────────────────────────────

    /// Creates a DIB that can be drawn into directly via `ppvBits`.
    /// `hSection`/`dwOffset`: 0 for a pagefile-backed section.
    /// `ppvBits` receives a pointer to the raw pixel data.
    @lib pub fn CreateDIBSection(
        hdc:        HDC,
        pbmi:       *const void,   // *BITMAPINFO
        iUsage:     u32,           // DIBColorMode.*
        ppvBits:    **void,        // receives pointer to pixel data
        hSection:   *void,         // file-mapping object, or NULL
        dwOffset:   u32
    ): HBITMAP

    /// Draws `cScanLines` scan lines of DIB data.
    @lib pub fn SetDIBitsToDevice(
        hdc:           HDC,
        XDest:         i32,
        YDest:         i32,
        dwWidth:       u32,
        dwHeight:      u32,
        XSrc:          i32,
        YSrc:          i32,
        uStartScan:    u32,
        cScanLines:    u32,
        lpvBits:       *const void,
        lpbmi:         *const void,  // *BITMAPINFO
        fuColorUse:    u32           // DIBColorMode.*
    ): i32

    /// Copies and stretches DIB bits. Negative `nSrcHeight` flips vertically.
    @lib pub fn StretchDIBits(
        hdc:       HDC,
        XDest:     i32, YDest:   i32, nDestWidth: i32, nDestHeight: i32,
        XSrc:      i32, YSrc:    i32, nSrcWidth:  i32, nSrcHeight:  i32,
        lpBits:    *const void,
        lpBitsInfo: *const void,  // *BITMAPINFO
        iUsage:    u32,
        dwRop:     u32
    ): i32

    // ── Standard UI drawing ───────────────────────────────────────

    /// Draws a 3-D border around `lprc`. `edge`: BDR_* flags (0x0..0x9),
    /// `grfFlags`: BF_* flags (BF_RECT=0x0F for all sides).
    @lib pub fn DrawEdge(hdc: HDC, qrc: *void, edge: u32, grfFlags: u32): i32

    /// Draws a standard button, caption, or scroll-bar control.
    /// `uType`: DFC_* (1=CAPTION, 2=MENU, 3=SCROLL, 4=BUTTON),
    /// `uState`: DFCS_* state flags.
    @lib pub fn DrawFrameControl(hdc: HDC, lprc: *void, uType: u32, uState: u32): i32

    // ── DC creation for device names ─────────────────────────────

    /// Creates a DC for a named device (e.g. a printer).
    /// `lpszDriver`: "DISPLAY" for screen. Pass NULL for auto-selection.
    @lib pub fn CreateDCA(
        lpszDriver:  *const char,
        lpszDevice:  *const char,
        lpszOutput:  *const char,
        lpInitData:  *const void
    ): *void
    @lib pub fn CreateDCW(
        lpszDriver:  *const u16,
        lpszDevice:  *const u16,
        lpszOutput:  *const u16,
        lpInitData:  *const void
    ): *void

    // ── Color ─────────────────────────────────────────────────────

    /// Returns the nearest solid colour supported by the device.
    @lib pub fn GetNearestColor(hdc: HDC, crColor: u32): u32

    /// Returns the DIB colour table entries.
    @lib pub fn GetDIBColorTable(
        hdc:        HDC,
        uStartIndex: u32,
        cEntries:   u32,
        prgbq:      *RGBQUAD
    ): u32

    @lib pub fn SetDIBColorTable(
        hdc:        HDC,
        uStartIndex: u32,
        cEntries:   u32,
        prgbq:      *const RGBQUAD
    ): u32

    // ── Clipping figure ───────────────────────────────────────────

    /// Closes the open figure in a path (connects last point to first).
    @lib pub fn CloseFigure(hdc: HDC): i32

    // ── Misc ──────────────────────────────────────────────────────

    @lib pub fn FrameRect(hDC: HDC, lprc: *const RECT, hbr: HBRUSH): i32
    @lib pub fn InvertRect(hDC: *void, lprc: *const void): i32

    // ── Font enumeration ─────────────────────────────────────────

    /// Enumerates installed fonts matching `lpLogfont`.
    /// Pass NULL for `lpLogfont` to list one font per family.
    /// Set `lpLogfont->lfCharSet` = 1 (DEFAULT_CHARSET) to enumerate all charsets,
    /// or a specific FontCharset.* value to filter.
    ///
    /// The callback receives:
    ///   *ENUMLOGFONTEXA (logical font),
    ///   *NEWTEXTMETRICEXA (metrics),
    ///   DWORD font type (1=raster, 2=device, 4=truetype),
    ///   LPARAM lParam (user-supplied).
    /// Returning 0 stops enumeration; non-zero continues.
    @lib pub fn EnumFontFamiliesExA(
        hdc:        HDC,
        lpLogfont:  *const LOGFONTA,
        lpProc:     fn(i64, i64, u32, i64): i32,
        lParam:     i64,
        dwFlags:    u32       // must be 0
    ): i32
    @lib pub fn EnumFontFamiliesExW(
        hdc:        HDC,
        lpLogfont:  *const LOGFONTA,
        lpProc:     fn(i64, i64, u32, i64): i32,
        lParam:     i64,
        dwFlags:    u32       // must be 0
    ): i32

    /// Older variant — `lpszFamily` is the family-name filter (NULL = all).
    @lib pub fn EnumFontsA(
        hdc:        HDC,
        lpFaceName: *const char,
        lpFontFunc: fn(i64, i64, u32, i64): i32,
        lParam:     i64
    ): i32
    @lib pub fn EnumFontsW(
        hdc:        HDC,
        lpFaceName: *const u16,
        lpFontFunc: fn(i64, i64, u32, i64): i32,
        lParam:     i64
    ): i32

    /// Returns the per-character widths of the characters in
    /// [iFirstChar, iLastChar] for the currently selected font.
    @lib pub fn GetCharWidth32A(
        hdc:        HDC,
        iFirstChar: u32,
        iLastChar:  u32,
        lpBuffer:   *i32
    ): i32
    @lib pub fn GetCharWidth32W(
        hdc:        HDC,
        iFirstChar: u32,
        iLastChar:  u32,
        lpBuffer:   *i32
    ): i32

    /// Returns ABC width metrics (a,b,c) for a range of characters.
    /// `lpabc`: array of `iLastChar-iFirstChar+1` ABC structs (12 bytes each).
    @lib pub fn GetCharABCWidthsA(
        hdc:        HDC,
        wFirst:     u32,
        wLast:      u32,
        lpabc:      *void
    ): i32
    @lib pub fn GetCharABCWidthsW(
        hdc:        HDC,
        wFirst:     u32,
        wLast:      u32,
        lpabc:      *void
    ): i32

    /// Translates `c` characters to glyph indices for the current font.
    /// `lpgi`: output array of u16 glyph indices.
    /// `fl`: 0 (default) or 1 (GGI_MARK_NONEXISTING_GLYPHS).
    @lib pub fn GetGlyphIndicesA(
        hdc:    HDC,
        lpstr:  *const char,
        c:      i32,
        pgi:    *u16,
        fl:     u32
    ): u32
    @lib pub fn GetGlyphIndicesW(
        hdc:    HDC,
        lpstr:  *const u16,
        c:      i32,
        pgi:    *u16,
        fl:     u32
    ): u32

    // ── Object enumeration ────────────────────────────────────────

    /// Enumerates the pens or brushes available in `hdc`.
    /// `nObjectType`: 1=OBJ_PEN, 2=OBJ_BRUSH.
    @lib pub fn EnumObjects(
        hdc:         HDC,
        nObjectType: i32,
        lpFunc:      fn(i64, i64): i32,
        lParam:      i64
    ): i32

    // ── Path abort ────────────────────────────────────────────────

    @lib pub fn AbortPath(hdc: HDC): i32
    @lib pub fn WidenPath(hdc: HDC): i32
    @lib pub fn FlattenPath(hdc: HDC): i32

    // ── DC metadata ───────────────────────────────────────────────

    /// Cancels an outstanding drawing or printing operation.
    @lib pub fn CancelDC(hdc: HDC): i32

    /// Returns the type of `hdc` (0=OBJ_DC, …); see GetObjectType.
    @lib pub fn GetObjectType(h: *void): u32

    /// Snaps `hdc` to the current display device's identification.
    @lib pub fn GetDCBrushColor(hdc: HDC): u32
    @lib pub fn SetDCBrushColor(hdc: HDC, crColor: u32): u32
    @lib pub fn GetDCPenColor(hdc: HDC): u32
    @lib pub fn SetDCPenColor(hdc: HDC, crColor: u32): u32

    // ── Additional handle types (metafile / colour space) ────────

    @handle pub struct HMETAFILE {}
    @handle pub struct HENHMETAFILE {}
    @handle pub struct HCOLORSPACE {}

    // ── Bitmap structures (BITMAP / RGBQUAD / BITMAPINFOHEADER above) ─

    /// `BITMAPINFO` — 44 bytes (header + first color). For larger
    /// palettes, allocate `sizeof(BITMAPINFOHEADER) + n*sizeof(RGBQUAD)`
    /// and cast the pointer.
    pub struct BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER
        bmiColors: RGBQUAD
    }

    /// `BITMAPFILEHEADER` — 14 bytes (packed on Windows). The actual
    /// SDK declaration is `#pragma pack(2)`. Most users only need the
    /// fields by name, not the literal layout, so we expose unpacked.
    pub struct BITMAPFILEHEADER {
        bfType:      u16   // 'BM' = 0x4D42
        _pad:        u16
        bfSize:      u32
        bfReserved1: u16
        bfReserved2: u16
        bfOffBits:   u32
    }

    @lib pub fn LoadBitmapA(hInstance: i64, lpBitmapName: *const char): HBITMAP
    @lib pub fn LoadBitmapW(hInstance: i64, lpBitmapName: *const u16): HBITMAP

    @lib pub fn CreateBitmapIndirect(pbm: *const BITMAP): HBITMAP

    /// Creates a DIB-based HBITMAP from `lpbInit` (optional initial
    /// pixels) and the BITMAPINFO header. `iUsage`: DIB_RGB_COLORS=0
    /// or DIB_PAL_COLORS=1.
    @lib pub fn CreateDIBitmap(
        hdc:    HDC,
        pbmih:  *const BITMAPINFOHEADER,
        flInit: u32,                       // CBM_INIT = 4 to copy pixels
        pjBits: *const void,
        pbmi:   *const BITMAPINFO,
        iUsage: u32
    ): HBITMAP

    /// Copy bitmap bits into `lpvBits`. `cb`: byte size of buffer.
    /// Returns the number of bytes copied, or 0 on failure.
    @lib pub fn GetBitmapBits(hbit: HBITMAP, cb: i32, lpvBits: *void): i32

    @lib pub fn SetBitmapBits(hbm: HBITMAP, cb: u32, pvBits: *const void): i32

    /// Reads `cLines` rows starting at `uStartScan` from `hbmp` into
    /// `lpvBits`. `lpbi` describes the desired output format.
    @lib pub fn GetDIBits(
        hdc:        HDC,
        hbm:        HBITMAP,
        uStartScan: u32,
        cLines:     u32,
        lpvBits:    *void,
        lpbmi:      *BITMAPINFO,
        uUsage:     u32
    ): i32

    @lib pub fn SetDIBits(
        hdc:    HDC,
        hbm:    HBITMAP,
        start:  u32,
        cLines: u32,
        lpBits: *const void,
        lpbmi:  *const BITMAPINFO,
        ColorUse: u32
    ): i32

    /// Bit-block transfer with built-in DIB conversion.
    @lib pub fn StretchDIBits(
        hdc:           HDC,
        xDest:         i32,
        yDest:         i32,
        DestWidth:     i32,
        DestHeight:    i32,
        xSrc:          i32,
        ySrc:          i32,
        SrcWidth:      i32,
        SrcHeight:     i32,
        lpBits:        *const void,
        lpbmi:         *const BITMAPINFO,
        iUsage:        u32,
        rop:           u32
    ): i32

    // ── Pen / brush extras ───────────────────────────────────────

    /// `LOGPEN` — 16 bytes.
    pub struct LOGPEN {
        lopnStyle: u32
        lopnWidth: POINT
        lopnColor: u32
    }

    /// `LOGBRUSH` — 16 bytes. `lbStyle` is BS_SOLID=0, BS_NULL=1,
    /// BS_HATCHED=2, BS_PATTERN=3, BS_DIBPATTERN=5, BS_DIBPATTERNPT=6.
    pub struct LOGBRUSH {
        lbStyle: u32
        lbColor: u32
        lbHatch: u64
    }

    @lib pub fn CreateBrushIndirect(plbrush: *const LOGBRUSH): HBRUSH
    @lib pub fn CreatePenIndirect(plpen: *const LOGPEN): HPEN

    /// Geometric or cosmetic pen. `iPenStyle`: PenStyleEx.* OR'd
    /// with one of PenStyle.* in the low nibble.
    @lib pub fn ExtCreatePen(
        iPenStyle: u32,
        cWidth:    u32,
        plbrush:   *const LOGBRUSH,
        cStyle:    u32,
        pstyle:    *const u32
    ): HPEN

    @lib pub fn SetBrushOrgEx(hdc: HDC, x: i32, y: i32, lppt: *POINT): i32
    @lib pub fn GetBrushOrgEx(hdc: HDC, lppt: *POINT): i32

    /// Unrealizes the brush so its next selection re-aligns to the
    /// new brush origin.
    @lib pub fn UnrealizeObject(h: HGDIOBJ): i32

    // ── Region (extended) ────────────────────────────────────────

    @lib pub fn SetRectRgn(hrgn: HRGN, left: i32, top: i32, right: i32, bottom: i32): i32

    @lib pub fn EqualRgn(hrgn1: HRGN, hrgn2: HRGN): i32

    @lib pub fn PtInRegion(hrgn: HRGN, x: i32, y: i32): i32
    @lib pub fn RectInRegion(hrgn: HRGN, lprect: *const RECT): i32

    @lib pub fn GetRgnBox(hrgn: HRGN, lprc: *RECT): i32

    @lib pub fn PaintRgn(hdc: HDC, hrgn: HRGN): i32
    @lib pub fn FillRgn(hdc: HDC, hrgn: HRGN, hbr: HBRUSH): i32
    @lib pub fn FrameRgn(hdc: HDC, hrgn: HRGN, hbr: HBRUSH, w: i32, h: i32): i32
    @lib pub fn InvertRgn(hdc: HDC, hrgn: HRGN): i32

    /// Header preceding the variable-length RECT array in an
    /// `RGNDATA` blob (passed to ExtCreateRegion).
    pub struct RGNDATAHEADER {
        dwSize:  u32
        iType:   u32   // RDH_RECTANGLES = 1
        nCount:  u32
        nRgnSize: u32
        rcBound: RECT
    }

    @lib pub fn ExtCreateRegion(
        lpx:       *const void,   // *XFORM (NULL = identity)
        nCount:    u32,
        lpData:    *const void    // RGNDATAHEADER + RECT[nCount]
    ): HRGN

    // ── Clipping (extended) ──────────────────────────────────────

    /// `mode`: RGN_AND=1, RGN_OR=2, RGN_XOR=3, RGN_DIFF=4, RGN_COPY=5.
    @lib pub fn ExtSelectClipRgn(hdc: HDC, hrgn: HRGN, mode: i32): i32

    /// Fills `hrgn` with the DC's clip region, or returns 0 if there
    /// is none.  Returns -1 on error.
    @lib pub fn GetClipRgn(hdc: HDC, hrgn: HRGN): i32

    @lib pub fn PtVisible(hdc: HDC, x: i32, y: i32): i32
    @lib pub fn RectVisible(hdc: HDC, lprect: *const RECT): i32

    // ── World / mapping transform ────────────────────────────────

    /// `XFORM` — 24 bytes. 2x3 affine matrix used by Set/GetWorldTransform.
    pub struct XFORM {
        eM11: f32
        eM12: f32
        eM21: f32
        eM22: f32
        eDx:  f32
        eDy:  f32
    }

    @lib pub fn GetWorldTransform(hdc: HDC, lpxf: *XFORM): i32
    @lib pub fn SetWorldTransform(hdc: HDC, lpxf: *const XFORM): i32

    /// `iMode`: WorldTransformMode.*.
    @lib pub fn ModifyWorldTransform(hdc: HDC, lpxf: *const XFORM, iMode: u32): i32

    @lib pub fn CombineTransform(
        lpxfOut: *XFORM,
        lpxf1:   *const XFORM,
        lpxf2:   *const XFORM
    ): i32

    @lib pub fn GetGraphicsMode(hdc: HDC): i32
    @lib pub fn SetGraphicsMode(hdc: HDC, iMode: i32): i32

    @lib pub fn DPtoLP(hdc: HDC, lppt: *POINT, c: i32): i32
    @lib pub fn LPtoDP(hdc: HDC, lppt: *POINT, c: i32): i32

    // ── Coordinates (extended) ───────────────────────────────────

    @lib pub fn GetViewportExtEx(hdc: HDC, lpsize: *SIZE): i32
    @lib pub fn GetWindowExtEx(hdc: HDC, lpsize: *SIZE): i32

    @lib pub fn ScaleViewportExtEx(
        hdc: HDC, xn: i32, dx: i32, yn: i32, dy: i32, lpsz: *SIZE
    ): i32
    @lib pub fn ScaleWindowExtEx(
        hdc: HDC, xn: i32, dx: i32, yn: i32, dy: i32, lpsz: *SIZE
    ): i32

    @lib pub fn OffsetViewportOrgEx(hdc: HDC, x: i32, y: i32, lppt: *POINT): i32
    @lib pub fn OffsetWindowOrgEx(hdc: HDC, x: i32, y: i32, lppt: *POINT): i32

    // ── Drawing extras ───────────────────────────────────────────

    @lib pub fn PolyDraw(
        hdc:     HDC,
        apt:     *const POINT,
        aj:      *const u8,    // PolyDrawType.*
        cpt:     i32
    ): i32

    @lib pub fn PolyPolygon(
        hdc:    HDC,
        apt:    *const POINT,
        asz:    *const i32,
        csz:    i32
    ): i32

    @lib pub fn PolyPolyline(
        hdc:        HDC,
        apt:        *const POINT,
        asz:        *const u32,
        csz:        u32
    ): i32

    @lib pub fn ArcTo(
        hdc:  HDC,
        left: i32, top: i32, right: i32, bottom: i32,
        xr1:  i32, yr1: i32, xr2: i32, yr2: i32
    ): i32

    @lib pub fn AngleArc(
        hdc:        HDC,
        x:          i32,
        y:          i32,
        r:          u32,
        StartAngle: f32,
        SweepAngle: f32
    ): i32

    @lib pub fn SetPolyFillMode(hdc: HDC, iPolyFillMode: i32): i32
    @lib pub fn GetPolyFillMode(hdc: HDC): i32

    @lib pub fn SetROP2(hdc: HDC, rop2: i32): i32
    @lib pub fn GetROP2(hdc: HDC): i32

    @lib pub fn GetCurrentObject(hdc: HDC, type_: u32): HGDIOBJ

    @lib pub fn GetMiterLimit(hdc: HDC, plimit: *f32): i32
    @lib pub fn SetMiterLimit(hdc: HDC, limit: f32, plimit: *f32): i32

    /// Forces all batched GDI commands to flush to the screen / DC.
    @lib pub fn GdiFlush(): i32
    @lib pub fn GdiSetBatchLimit(dw: u32): u32
    @lib pub fn GdiGetBatchLimit(): u32

    /// MaskBlt — three-source blit with a mask bitmap (msimg32 too).
    @lib pub fn MaskBlt(
        hdcDest: HDC,
        xDest:   i32, yDest: i32, width: i32, height: i32,
        hdcSrc:  HDC,
        xSrc:    i32, ySrc: i32,
        hbmMask: HBITMAP,
        xMask:   i32, yMask: i32,
        rop:     u32
    ): i32

    @lib pub fn PlgBlt(
        hdcDest: HDC,
        lpPoint: *const POINT,
        hdcSrc:  HDC,
        xSrc:    i32, ySrc: i32, width: i32, height: i32,
        hbmMask: HBITMAP,
        xMask:   i32, yMask: i32
    ): i32

    // ── Text / font (extended) ───────────────────────────────────

    @lib pub fn GetTextFaceA(hdc: HDC, c: i32, lpName: *char): i32
    @lib pub fn GetTextFaceW(hdc: HDC, c: i32, lpName: *u16): i32

    @lib pub fn SetTextJustification(hdc: HDC, extra: i32, count: i32): i32
    @lib pub fn SetTextCharacterExtra(hdc: HDC, extra: i32): i32
    @lib pub fn GetTextCharacterExtra(hdc: HDC): i32

    /// Filled by GetTextExtentExPoint — character widths up to the
    /// returned `*lpnFit`.
    @lib pub fn GetTextExtentExPointA(
        hdc:     HDC,
        lpszString: *const char,
        cchString: i32,
        nMaxExtent: i32,
        lpnFit:   *i32,
        lpnDx:    *i32,
        lpSize:   *SIZE
    ): i32
    @lib pub fn GetTextExtentExPointW(
        hdc:     HDC,
        lpszString: *const u16,
        cchString: i32,
        nMaxExtent: i32,
        lpnFit:   *i32,
        lpnDx:    *i32,
        lpSize:   *SIZE
    ): i32

    @lib pub fn GetFontData(
        hdc:        HDC,
        dwTable:    u32,    // 4-byte OpenType tag, e.g. 'name'
        dwOffset:   u32,
        pvBuffer:   *void,
        cjBuffer:   u32
    ): u32

    /// `GLYPHMETRICS` — 24 bytes.
    pub struct GLYPHMETRICS {
        gmBlackBoxX:        u32
        gmBlackBoxY:        u32
        gmptGlyphOrigin:    POINT
        gmCellIncX:         i16
        gmCellIncY:         i16
        _pad:               u32
    }

    /// `MAT2` — 16 bytes. Two-by-two 16.16 fixed-point matrix.
    pub struct MAT2 {
        eM11_value: i32
        eM12_value: i32
        eM21_value: i32
        eM22_value: i32
    }

    @lib pub fn GetGlyphOutlineA(
        hdc:           HDC,
        uChar:         u32,
        fuFormat:      u32,
        lpgm:          *GLYPHMETRICS,
        cjBuffer:      u32,
        pvBuffer:      *void,
        lpmat2:        *const MAT2
    ): u32
    @lib pub fn GetGlyphOutlineW(
        hdc:           HDC,
        uChar:         u32,
        fuFormat:      u32,
        lpgm:          *GLYPHMETRICS,
        cjBuffer:      u32,
        pvBuffer:      *void,
        lpmat2:        *const MAT2
    ): u32

    /// `KERNINGPAIR` — 6 bytes packed; padded to 8 here.
    pub struct KERNINGPAIR {
        wFirst:    u16
        wSecond:   u16
        iKernAmount: i32
    }

    @lib pub fn GetKerningPairsA(hdc: HDC, nPairs: u32, lpKernPair: *KERNINGPAIR): u32
    @lib pub fn GetKerningPairsW(hdc: HDC, nPairs: u32, lpKernPair: *KERNINGPAIR): u32

    /// `GCP_RESULTSW` — 56 bytes. Fields point at caller-allocated arrays.
    pub struct GCP_RESULTSW {
        lStructSize: u32
        _pad:        u32
        lpOutString: *u16
        lpOrder:     *u32
        lpDx:        *i32
        lpCaretPos:  *i32
        lpClass:     *u8
        lpGlyphs:    *u16
        nGlyphs:     u32
        nMaxFit:     i32
    }

    @lib pub fn GetCharacterPlacementA(
        hdc:        HDC,
        lpString:   *const char,
        nCount:     i32,
        nMexExtent: i32,
        lpResults:  *GCP_RESULTSW,   // ANSI variant takes GCP_RESULTSA in C; layout matches
        dwFlags:    u32
    ): u32
    @lib pub fn GetCharacterPlacementW(
        hdc:        HDC,
        lpString:   *const u16,
        nCount:     i32,
        nMexExtent: i32,
        lpResults:  *GCP_RESULTSW,
        dwFlags:    u32
    ): u32

    // ── Font resource management ─────────────────────────────────

    @lib pub fn AddFontResourceA(lpFileName: *const char): i32
    @lib pub fn AddFontResourceW(lpFileName: *const u16): i32

    @lib pub fn RemoveFontResourceA(lpFileName: *const char): i32
    @lib pub fn RemoveFontResourceW(lpFileName: *const u16): i32

    /// `fl`: FR_PRIVATE = 0x10, FR_NOT_ENUM = 0x20.
    @lib pub fn AddFontResourceExA(name: *const char, fl: u32, res: *void): i32
    @lib pub fn AddFontResourceExW(name: *const u16, fl: u32, res: *void): i32

    @lib pub fn RemoveFontResourceExA(name: *const char, fl: u32, pdv: *void): i32
    @lib pub fn RemoveFontResourceExW(name: *const u16, fl: u32, pdv: *void): i32

    /// Loads a font from memory into the process-private list.
    @lib pub fn AddFontMemResourceEx(
        pFileView:   *void,
        cjSize:      u32,
        pvResrved:   *void,
        pNumFonts:   *u32
    ): HANDLE

    @lib pub fn RemoveFontMemResourceEx(h: HANDLE): i32

    @lib pub fn CreateScalableFontResourceA(
        fdwHidden:  u32,
        lpszFont:   *const char,
        lpszFile:   *const char,
        lpszPath:   *const char
    ): i32
    @lib pub fn CreateScalableFontResourceW(
        fdwHidden:  u32,
        lpszFont:   *const u16,
        lpszFile:   *const u16,
        lpszPath:   *const u16
    ): i32

    @lib pub fn SetMapperFlags(hdc: HDC, flags: u32): u32

    // ── Palette ──────────────────────────────────────────────────

    /// `PALETTEENTRY` — 4 bytes.
    pub struct PALETTEENTRY {
        peRed:   u8
        peGreen: u8
        peBlue:  u8
        peFlags: u8
    }

    /// `LOGPALETTE` — 8 bytes + n*PALETTEENTRY. Allocate
    /// `sizeof(LOGPALETTE) + (n-1)*sizeof(PALETTEENTRY)` for n entries.
    pub struct LOGPALETTE {
        palVersion:    u16   // typically 0x300
        palNumEntries: u16
        palPalEntry:   PALETTEENTRY
    }

    @lib pub fn CreatePalette(plpal: *const LOGPALETTE): HPALETTE

    @lib pub fn SelectPalette(hdc: HDC, hPal: HPALETTE, bForceBkgd: i32): HPALETTE

    @lib pub fn RealizePalette(hdc: HDC): u32

    @lib pub fn GetPaletteEntries(
        hpal:       HPALETTE,
        iStart:     u32,
        cEntries:   u32,
        pPalEntries: *PALETTEENTRY
    ): u32

    @lib pub fn SetPaletteEntries(
        hpal:       HPALETTE,
        iStart:     u32,
        cEntries:   u32,
        pPalEntries: *const PALETTEENTRY
    ): u32

    @lib pub fn AnimatePalette(
        hPal:       HPALETTE,
        iStartIndex: u32,
        cEntries:   u32,
        ppe:        *const PALETTEENTRY
    ): i32

    @lib pub fn ResizePalette(hpal: HPALETTE, n: u32): i32

    @lib pub fn GetSystemPaletteEntries(
        hdc:       HDC,
        iStart:    u32,
        cEntries:  u32,
        pPalEntries: *PALETTEENTRY
    ): u32

    @lib pub fn GetSystemPaletteUse(hdc: HDC): u32
    @lib pub fn SetSystemPaletteUse(hdc: HDC, use_: u32): u32

    @lib pub fn UpdateColors(hdc: HDC): i32

    // ── Print / spooler hooks ────────────────────────────────────

    /// `DOCINFOA` / `DOCINFOW` — 40 bytes (struct size only counts
    /// the first 5 fields on Win9x; on NT+ the full layout below).
    pub struct DOCINFOA {
        cbSize:       i32
        _pad:         u32
        lpszDocName:  *const char
        lpszOutput:   *const char
        lpszDatatype: *const char
        fwType:       u32
    }
    pub struct DOCINFOW {
        cbSize:       i32
        _pad:         u32
        lpszDocName:  *const u16
        lpszOutput:   *const u16
        lpszDatatype: *const u16
        fwType:       u32
    }

    @lib pub fn StartDocA(hdc: HDC, lpdi: *const DOCINFOA): i32
    @lib pub fn StartDocW(hdc: HDC, lpdi: *const DOCINFOW): i32
    @lib pub fn EndDoc(hdc: HDC): i32
    @lib pub fn StartPage(hdc: HDC): i32
    @lib pub fn EndPage(hdc: HDC): i32
    @lib pub fn AbortDoc(hdc: HDC): i32
    @lib pub fn SetAbortProc(hdc: HDC, lpAbortProc: fn(HDC, i32): i32): i32

    /// Sends a low-level command directly to a device driver.
    @lib pub fn Escape(
        hdc:        HDC,
        iEscape:    i32,
        cjIn:       i32,
        pvIn:       *const void,
        pvOut:      *void
    ): i32

    @lib pub fn ExtEscape(
        hdc:        HDC,
        iEscape:    i32,
        cjInput:    i32,
        lpInData:   *const u8,
        cjOutput:   i32,
        lpOutData:  *u8
    ): i32

    // ── DC creation extras / reset ──────────────────────────────

    /// Information context — like CreateDC but only for queries
    /// (no drawing).  Cheaper to create than a DC.
    @lib pub fn CreateICA(
        pszDriver: *const char,
        pszDevice: *const char,
        pszPort:   *const char,
        pdm:       *const void   // *DEVMODEA
    ): HDC
    @lib pub fn CreateICW(
        pszDriver: *const u16,
        pszDevice: *const u16,
        pszPort:   *const u16,
        pdm:       *const void
    ): HDC

    @lib pub fn ResetDCA(hdc: HDC, lpInitData: *const void): HDC
    @lib pub fn ResetDCW(hdc: HDC, lpInitData: *const void): HDC

    // ── Metafiles (legacy WMF) ──────────────────────────────────

    @lib pub fn CreateMetaFileA(lpszFile: *const char): HDC
    @lib pub fn CreateMetaFileW(lpszFile: *const u16): HDC
    @lib pub fn CloseMetaFile(hdc: HDC): HMETAFILE
    @lib pub fn DeleteMetaFile(hmf: HMETAFILE): i32
    @lib pub fn PlayMetaFile(hdc: HDC, hmf: HMETAFILE): i32
    @lib pub fn GetMetaFileA(lpName: *const char): HMETAFILE
    @lib pub fn GetMetaFileW(lpName: *const u16): HMETAFILE
    @lib pub fn GetMetaFileBitsEx(hmf: HMETAFILE, cbBuffer: u32, lpData: *void): u32
    @lib pub fn SetMetaFileBitsEx(cbBuffer: u32, lpData: *const u8): HMETAFILE
    @lib pub fn CopyMetaFileA(hmf: HMETAFILE, file: *const char): HMETAFILE
    @lib pub fn CopyMetaFileW(hmf: HMETAFILE, file: *const u16): HMETAFILE

    // ── Enhanced metafiles ──────────────────────────────────────

    @lib pub fn CreateEnhMetaFileA(
        hdcRef:     HDC,
        lpFilename: *const char,
        lprc:       *const RECT,
        lpDesc:     *const char
    ): HDC
    @lib pub fn CreateEnhMetaFileW(
        hdcRef:     HDC,
        lpFilename: *const u16,
        lprc:       *const RECT,
        lpDesc:     *const u16
    ): HDC

    @lib pub fn CloseEnhMetaFile(hdc: HDC): HENHMETAFILE
    @lib pub fn DeleteEnhMetaFile(hmf: HENHMETAFILE): i32
    @lib pub fn PlayEnhMetaFile(hdc: HDC, hmf: HENHMETAFILE, lprect: *const RECT): i32
    @lib pub fn GetEnhMetaFileA(lpName: *const char): HENHMETAFILE
    @lib pub fn GetEnhMetaFileW(lpName: *const u16): HENHMETAFILE
    @lib pub fn GetEnhMetaFileBits(hemf: HENHMETAFILE, nSize: u32, lpData: *u8): u32
    @lib pub fn SetEnhMetaFileBits(nSize: u32, pb: *const u8): HENHMETAFILE
    @lib pub fn CopyEnhMetaFileA(hemfSrc: HENHMETAFILE, lpszFile: *const char): HENHMETAFILE
    @lib pub fn CopyEnhMetaFileW(hemfSrc: HENHMETAFILE, lpszFile: *const u16): HENHMETAFILE

    /// Convert a WMF blob to a portable WMF with placeable header
    /// for embedding in clipboard / OLE.
    @lib pub fn GetWinMetaFileBits(
        hemf:    HENHMETAFILE,
        cbData16: u32,
        pData16: *u8,
        iMapMode: i32,
        hdcRef:  HDC
    ): u32

    @lib pub fn SetWinMetaFileBits(
        nSize:   u32,
        lpMeta16Data: *const u8,
        hdcRef:  HDC,
        lpMFP:   *const void    // *METAFILEPICT (24 bytes)
    ): HENHMETAFILE

    // ── Color spaces ─────────────────────────────────────────────

    @lib pub fn CreateColorSpaceA(lpcs: *const void): HCOLORSPACE
    @lib pub fn CreateColorSpaceW(lpcs: *const void): HCOLORSPACE
    @lib pub fn DeleteColorSpace(hcs: HCOLORSPACE): i32
    @lib pub fn GetColorSpace(hdc: HDC): HCOLORSPACE
    @lib pub fn SetColorSpace(hdc: HDC, hcs: HCOLORSPACE): HCOLORSPACE

    /// ICC mode for the DC — IcmMode.*.
    @lib pub fn SetICMMode(hdc: HDC, mode: i32): i32

    /// Returns the BOUNDS rect accumulated since the last call.
    /// `dwFlags`: 0 = leave bounds, 4 = DCB_RESET, 8 = DCB_ENABLE, 1 = DCB_DISABLE.
    @lib pub fn GetBoundsRect(hdc: HDC, lprcBounds: *RECT, flags: u32): u32
    @lib pub fn SetBoundsRect(hdc: HDC, lprc: *const RECT, flags: u32): u32

    // ── Misc ────────────────────────────────────────────────────

    /// Returns the version of the GDI on the calling process.
    @lib pub fn GdiGetSpoolMessage(
        pData:    *void,
        cbSize:   u32,
        cbReceived: *u32,
        unused1:  u32
    ): i32

    /// Returns a system-default device context for `iDeviceType` (1=display
    /// adapter, 2=printer).
    @lib pub fn EngLockDirectDrawSurface(): *void

    /// Tells GDI that subsequent drawing should be ROP'd against the
    /// "alternate" plane.  Most callers won't need this.
    @lib pub fn GdiAlphaBlend(
        hdcDst: HDC, xoriginDst: i32, yoriginDst: i32, wDst: i32, hDst: i32,
        hdcSrc: HDC, xoriginSrc: i32, yoriginSrc: i32, wSrc: i32, hSrc: i32,
        ftn:    u32    // BLENDFUNCTION packed into u32
    ): i32

    // Pixel-format APIs (ChoosePixelFormat / SetPixelFormat /
    // GetPixelFormat / DescribePixelFormat / SwapBuffers, plus
    // PIXELFORMATDESCRIPTOR) are bound in opengl32.il together with
    // the rest of the OpenGL setup surface.

    // ── ExtTextOut / PolyTextOut ─────────────────────────────────

    /// `options`: ETO_CLIPPED=4, ETO_OPAQUE=2, ETO_NUMERICSLATIN=0x800,
    /// ETO_GLYPH_INDEX=0x10, ETO_RTLREADING=0x80, ETO_PDY=0x2000.
    @lib pub fn ExtTextOutA(
        hdc:     HDC,
        x:       i32,
        y:       i32,
        options: u32,
        lprect:  *const RECT,
        lpString: *const char,
        c:       u32,
        lpDx:    *const i32
    ): i32
    @lib pub fn ExtTextOutW(
        hdc:     HDC,
        x:       i32,
        y:       i32,
        options: u32,
        lprect:  *const RECT,
        lpString: *const u16,
        c:       u32,
        lpDx:    *const i32
    ): i32

    /// `POLYTEXTA` / `POLYTEXTW` — 32 bytes; describes one run for
    /// PolyTextOut. Layouts match: only the `lpstr` pointer differs.
    pub struct POLYTEXTW {
        x:       i32
        y:       i32
        n:       u32
        lpstr:   *const u16
        uiFlags: u32
        _pad:    u32
        rcl:     RECT
        pdx:     *const i32
    }

    @lib pub fn PolyTextOutA(hdc: HDC, ppt: *const void, nStrings: i32): i32
    @lib pub fn PolyTextOutW(hdc: HDC, ppt: *const POLYTEXTW, nStrings: i32): i32

    // ── Charset info ─────────────────────────────────────────────

    /// `CHARSETINFO` — 32 bytes.
    pub struct CHARSETINFO {
        ciCharset:     u32
        ciACP:         u32
        fsUsbLow:      u32
        fsUsbHigh1:    u32
        fsUsbHigh2:    u32
        fsUsbHigh3:    u32
        fsCsbDefault:  u32
        fsCsbSupported: u32
    }

    /// `dwSrc`: TCI_SRCCHARSET=1, TCI_SRCCODEPAGE=2, TCI_SRCFONTSIG=3,
    /// TCI_SRCLOCALE=0x1000.
    @lib pub fn TranslateCharsetInfo(
        lpSrc:   *u32,
        lpCs:    *CHARSETINFO,
        dwFlags: u32
    ): i32

    @lib pub fn GetTextCharset(hdc: HDC): i32
    @lib pub fn GetTextCharsetInfo(hdc: HDC, lpSig: *void, dwFlags: u32): i32

    @lib pub fn GetFontLanguageInfo(hdc: HDC): u32

    /// `GLYPHSET` — variable-length, header is 16 bytes.
    pub struct GLYPHSET_HEADER {
        cbThis:           u32
        flAccel:          u32   // 1 = GS_8BIT_INDICES
        cGlyphsSupported: u32
        cRanges:          u32
    }

    @lib pub fn GetFontUnicodeRanges(hdc: HDC, lpgs: *void): u32

    // ── Char widths (extended) ───────────────────────────────────

    /// Legacy 16-bit width function — prefer GetCharWidth32A.
    @lib pub fn GetCharWidthA(hdc: HDC, iFirst: u32, iLast: u32, lpBuffer: *i32): i32
    @lib pub fn GetCharWidthW(hdc: HDC, iFirst: u32, iLast: u32, lpBuffer: *i32): i32

    @lib pub fn GetCharWidthFloatA(hdc: HDC, iFirst: u32, iLast: u32, lpBuffer: *f32): i32
    @lib pub fn GetCharWidthFloatW(hdc: HDC, iFirst: u32, iLast: u32, lpBuffer: *f32): i32

    /// Glyph-index variant. `pgi`: NULL = sequential 0..cgi-1.
    @lib pub fn GetCharWidthI(
        hdc:        HDC,
        giFirst:    u32,
        cgi:        u32,
        pgi:        *u16,
        piWidths:   *i32
    ): i32

    /// `ABC` — 12 bytes; per-glyph spacing (overhang + advance + underhang).
    pub struct ABC {
        abcA: i32
        abcB: u32
        abcC: i32
    }
    pub struct ABCFLOAT {
        abcfA: f32
        abcfB: f32
        abcfC: f32
    }

    @lib pub fn GetCharABCWidthsFloatA(
        hdc:     HDC,
        iFirst:  u32,
        iLast:   u32,
        lpABCF:  *ABCFLOAT
    ): i32
    @lib pub fn GetCharABCWidthsFloatW(
        hdc:     HDC,
        iFirst:  u32,
        iLast:   u32,
        lpABCF:  *ABCFLOAT
    ): i32

    @lib pub fn GetCharABCWidthsI(
        hdc:    HDC,
        giFirst: u32,
        cgi:    u32,
        pgi:    *u16,
        pabc:   *ABC
    ): i32

    @lib pub fn GetTextExtentPointI(
        hdc:        HDC,
        pgiIn:      *u16,
        cgi:        i32,
        psize:      *SIZE
    ): i32

    @lib pub fn GetTextExtentExPointI(
        hdc:         HDC,
        lpwszString: *u16,
        cwchString:  i32,
        nMaxExtent:  i32,
        lpnFit:      *i32,
        lpnDx:       *i32,
        lpSize:      *SIZE
    ): i32

    // ── OUTLINETEXTMETRIC ────────────────────────────────────────

    /// `PANOSE` — 10 bytes. Visual classification of a typeface.
    pub struct PANOSE {
        bFamilyType:     u8
        bSerifStyle:     u8
        bWeight:         u8
        bProportion:     u8
        bContrast:       u8
        bStrokeVariation: u8
        bArmStyle:       u8
        bLetterform:     u8
        bMidline:        u8
        bXHeight:        u8
    }

    /// `OUTLINETEXTMETRICW` — 312 bytes on x64. We expose the header
    /// fields callers actually read; the trailing variable-length face
    /// name block is reached via pointer arithmetic past this struct.
    pub struct OUTLINETEXTMETRICW {
        otmSize:                u32
        otmTextMetrics:         TEXTMETRICA   // 60 bytes shared by A/W
        otmFiller:              u8
        otmPanoseNumber:        PANOSE
        otmfsSelection:         u16
        _pad0:                  u16
        otmfsType:              u32
        otmsCharSlopeRise:      i32
        otmsCharSlopeRun:       i32
        otmItalicAngle:         i32
        otmEMSquare:            u32
        otmAscent:              i32
        otmDescent:             i32
        otmLineGap:             u32
        otmsCapEmHeight:        u32
        otmsXHeight:            u32
        otmrcFontBox:           RECT
        otmMacAscent:           i32
        otmMacDescent:          i32
        otmMacLineGap:          u32
        otmusMinimumPPEM:       u32
        otmptSubscriptSize:     POINT
        otmptSubscriptOffset:   POINT
        otmptSuperscriptSize:   POINT
        otmptSuperscriptOffset: POINT
        otmsStrikeoutSize:      u32
        otmsStrikeoutPosition:  i32
        otmsUnderscoreSize:     i32
        otmsUnderscorePosition: i32
        // Variable-length face-name block follows as offsets:
        otmpFamilyName:         u64
        otmpFaceName:           u64
        otmpStyleName:          u64
        otmpFullName:           u64
    }

    @lib pub fn GetOutlineTextMetricsA(hdc: HDC, cjCopy: u32, potm: *void): u32
    @lib pub fn GetOutlineTextMetricsW(hdc: HDC, cjCopy: u32, potm: *OUTLINETEXTMETRICW): u32

    // ── Path queries ─────────────────────────────────────────────

    /// Returns the points + types of the current path, or -1 on
    /// failure. Pass NULL pointers and `cpt = 0` to query the count.
    @lib pub fn GetPath(
        hdc:  HDC,
        apt:  *POINT,
        aj:   *u8,
        cpt:  i32
    ): i32

    /// Combines the current path with the clipping region. `mode`:
    /// RGN_AND / RGN_OR / RGN_XOR / RGN_DIFF / RGN_COPY.
    @lib pub fn SelectClipPath(hdc: HDC, mode: i32): i32

    @lib pub fn PathToRegion2(hdc: HDC): HRGN   // alias for symmetry; same export

    // ── Region create extras ─────────────────────────────────────

    @lib pub fn CreateRectRgnIndirect(lprect: *const RECT): HRGN
    @lib pub fn CreateEllipticRgnIndirect(lprect: *const RECT): HRGN

    // ── Bitmap dim ───────────────────────────────────────────────

    @lib pub fn SetBitmapDimensionEx(hbm: HBITMAP, w: i32, h: i32, lpsz: *SIZE): i32
    @lib pub fn GetBitmapDimensionEx(hbm: HBITMAP, lpsz: *SIZE): i32

    @lib pub fn GetStretchBltMode(hdc: HDC): i32

    // ── Line DDA ─────────────────────────────────────────────────

    /// Iterates `lpProc(x, y, lpData)` over every pixel of the line
    /// from (`xStart`, `yStart`) to (`xEnd`, `yEnd`). Useful for
    /// custom rasterisation.
    @lib pub fn LineDDA(
        xStart:  i32,
        yStart:  i32,
        xEnd:    i32,
        yEnd:    i32,
        lpProc:  fn(i32, i32, i64),
        lpData:  i64
    ): i32

    // ── SetPixelV ────────────────────────────────────────────────

    /// Faster than SetPixel — does not return the previous colour.
    @lib pub fn SetPixelV(hdc: HDC, x: i32, y: i32, color: u32): i32

    // ── Gamma ramp (display-DC only) ─────────────────────────────

    /// `lpRamp`: pointer to 3 × 256 × WORD (1536 bytes) gamma table.
    @lib pub fn SetDeviceGammaRamp(hdc: HDC, lpRamp: *const u16): i32
    @lib pub fn GetDeviceGammaRamp(hdc: HDC, lpRamp: *u16): i32

    // ── Color matching extras ────────────────────────────────────

    @lib pub fn GetICMProfileA(hdc: HDC, pBufSize: *u32, pszFilename: *char): i32
    @lib pub fn GetICMProfileW(hdc: HDC, pBufSize: *u32, pszFilename: *u16): i32
    @lib pub fn SetICMProfileA(hdc: HDC, lpFileName: *const char): i32
    @lib pub fn SetICMProfileW(hdc: HDC, lpFileName: *const u16): i32

    @lib pub fn UpdateICMRegKeyA(reserved: u32, lpszCMID: *const char, lpszFileName: *const char, command: u32): i32
    @lib pub fn UpdateICMRegKeyW(reserved: u32, lpszCMID: *const u16, lpszFileName: *const u16, command: u32): i32

    // ── ColorMatchToTarget ───────────────────────────────────────

    /// `action`: CS_ENABLE=1, CS_DISABLE=2, CS_DELETE_TRANSFORM=3.
    @lib pub fn ColorMatchToTarget(hdcSrc: HDC, hdcTgt: HDC, action: u32): i32

    @lib pub fn EnumICMProfilesA(
        hdc:       HDC,
        proc:      fn(*const char, i64): i32,
        param:     i64
    ): i32
    @lib pub fn EnumICMProfilesW(
        hdc:       HDC,
        proc:      fn(*const u16, i64): i32,
        param:     i64
    ): i32

    // ── ExtFloodFill (have) / FloodFill ──────────────────────────

    /// Legacy FloodFill — fills from `(x, y)` with the current brush
    /// until reaching `crColor`.  Use ExtFloodFill for modern code.
    @lib pub fn FloodFill(hdc: HDC, x: i32, y: i32, color: u32): i32

    // ── Object cleanup helpers ───────────────────────────────────

    /// Returns the previous "draw" mode for path operations: 1 = ALTERNATE,
    /// 2 = WINDING.  Available as GetArcDirection (clockwise default).
    @lib pub fn GetArcDirection(hdc: HDC): i32

    // ── Print job control (gdi32-exported wrappers) ──────────────

    /// Cancels an outstanding draw / page if invoked on the printer DC.
    /// Most callers use AbortDoc instead.
    @lib pub fn QueryAbort(hdc: HDC, iParam: i32): i32

    // ── Set selection extras ─────────────────────────────────────

    @lib pub fn IntersectClipRect2(hdc: HDC, left: i32, top: i32, right: i32, bottom: i32): i32
    @lib pub fn OffsetClipRgn(hdc: HDC, x: i32, y: i32): i32

    @lib pub fn GetTextExtentPointA(
        hdc:       HDC,
        lpString:  *const char,
        c:         i32,
        lpsz:      *SIZE
    ): i32
    @lib pub fn GetTextExtentPointW(
        hdc:       HDC,
        lpString:  *const u16,
        c:         i32,
        lpsz:      *SIZE
    ): i32

    // ── Helper: GdiTransparentBlt lives in msimg32, not gdi32 ────
}
