// gdi32.dll — GDI drawing: device contexts, pens, brushes, fonts,
// bitmaps, regions, paths, and coordinate transforms.
//
// All handles below are opaque empty structs; cast integers to them
// when using stock-object IDs.  The Win64 ABI treats `*T` and `i64`
// identically in registers, so unsafe casts are safe at the boundary.

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
    // ── Opaque GDI handle types ───────────────────────────────────

    /// Generic GDI object handle (HGDIOBJ). Returned by SelectObject
    /// and used by DeleteObject. Cast from HPEN, HBRUSH, HFONT, etc.
    pub struct HGDIOBJ {}

    /// GDI pen handle.
    pub struct HPEN {}

    /// GDI solid or hatch brush handle.
    pub struct HBRUSHGDI {}

    /// GDI font handle.
    pub struct HFONT {}

    /// GDI bitmap handle.
    pub struct HBITMAP {}

    /// GDI region handle.
    pub struct HRGN {}

    /// GDI palette handle.
    pub struct HPALETTE {}

    // ── Geometry / layout structs ─────────────────────────────────

    /// `SIZE { cx: LONG, cy: LONG }` — 8 bytes.
    pub struct SIZE {
        cx: i32
        cy: i32
    }

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
        // lfFaceName[LF_FACESIZE=32] as 8 × u32
        _fn0: u32
        _fn1: u32
        _fn2: u32
        _fn3: u32
        _fn4: u32
        _fn5: u32
        _fn6: u32
        _fn7: u32
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
    /// Pass `0 as *HDC` to create a screen-compatible DC.
    @lib pub fn CreateCompatibleDC(hdc: *void): *void

    @lib pub fn DeleteDC(hdc: *void): i32

    /// Saves the current state of `hdc` and returns a save-state
    /// index. Restored with `RestoreDC(hdc, savedIndex)` or
    /// `RestoreDC(hdc, -1)` to restore the most recent save.
    @lib pub fn SaveDC(hdc: *void): i32
    @lib pub fn RestoreDC(hdc: *void, nSavedDC: i32): i32

    // ── Object selection / deletion ───────────────────────────────

    /// Selects `hObject` into `hdc`, returning the previously
    /// selected object of the same type (or NULL on error).
    /// Returned HGDIOBJ must be selected back before DeleteObject.
    @lib pub fn SelectObject(hdc: *void, hObject: *HGDIOBJ): *HGDIOBJ

    /// Deletes a GDI object (pen, brush, font, bitmap, region,
    /// palette). Never delete an object while it is selected.
    @lib pub fn DeleteObject(hObject: *HGDIOBJ): i32

    /// Returns a predefined stock GDI object. Cast result to the
    /// appropriate handle type.
    @lib pub fn GetStockObject(fnObject: i32): *HGDIOBJ

    /// Fills `lpObject` with data about a GDI object. `nCount` is
    /// the byte size of the buffer (e.g. sizeof LOGFONTA for fonts).
    @lib pub fn GetObjectA(hObject: *HGDIOBJ, nCount: i32, lpObject: *void): i32

    // ── Pen ───────────────────────────────────────────────────────

    /// Creates a cosmetic pen. `iStyle`: PenStyle.*, `cWidth`: pixel
    /// width (0 = 1 px), `color`: COLORREF (0x00BBGGRR).
    @lib pub fn CreatePen(iStyle: i32, cWidth: i32, color: u32): *HPEN

    // ── Brush ─────────────────────────────────────────────────────

    /// Creates a solid-color brush. `color`: COLORREF (0x00BBGGRR).
    @lib pub fn CreateSolidBrush(color: u32): *HBRUSHGDI

    /// Creates a hatch brush. `iHatch`: HatchStyle.*, `color`: COLORREF.
    @lib pub fn CreateHatchBrush(iHatch: i32, color: u32): *HBRUSHGDI

    /// Creates a pattern brush from a bitmap handle.
    @lib pub fn CreatePatternBrush(hbm: *HBITMAP): *HBRUSHGDI

    // ── Color / background ────────────────────────────────────────

    /// Sets the foreground text color. Returns the previous value.
    @lib pub fn SetTextColor(hdc: *void, color: u32): u32
    @lib pub fn GetTextColor(hdc: *void): u32

    /// Sets the background color used to fill gaps in hatched brushes,
    /// character cells, and dashed lines.
    @lib pub fn SetBkColor(hdc: *void, color: u32): u32
    @lib pub fn GetBkColor(hdc: *void): u32

    /// Sets background mix mode (BackgroundMode.transparent or .opaque).
    @lib pub fn SetBkMode(hdc: *void, iBkMode: i32): i32
    @lib pub fn GetBkMode(hdc: *void): i32

    // ── Drawing ───────────────────────────────────────────────────

    /// Moves the current position to `(X, Y)`. `lpPoint` receives
    /// the previous position; pass `0 as *void` to discard.
    @lib pub fn MoveToEx(hdc: *void, X: i32, Y: i32, lpPoint: *void): i32

    /// Draws a line from the current position to `(nXEnd, nYEnd)`,
    /// then updates the current position to `(nXEnd, nYEnd)`.
    @lib pub fn LineTo(hdc: *void, nXEnd: i32, nYEnd: i32): i32

    /// Draws a filled rectangle using the current pen and brush.
    @lib pub fn Rectangle(hdc: *void, left: i32, top: i32, right: i32, bottom: i32): i32

    /// Draws a filled ellipse inscribed in the given bounding rect.
    @lib pub fn Ellipse(hdc: *void, left: i32, top: i32, right: i32, bottom: i32): i32

    /// Draws a filled rectangle with rounded corners.
    @lib pub fn RoundRect(
        hdc: *void,
        left: i32, top: i32, right: i32, bottom: i32,
        width: i32, height: i32
    ): i32

    /// Draws a filled polygon through `cPoints` vertices in `lpPoints`.
    /// `lpPoints` is a pointer to an array of POINT structs.
    @lib pub fn Polygon(hdc: *void, lpPoints: *void, cPoints: i32): i32

    /// Draws connected line segments through `cPoints` vertices.
    @lib pub fn Polyline(hdc: *void, lpPoints: *void, cPoints: i32): i32

    /// Draws an arc. `(nXStartArc,nYStartArc)` and `(nXEndArc,nYEndArc)`
    /// are radial endpoints of the arc (not on the ellipse itself).
    @lib pub fn Arc(
        hdc: *void,
        left: i32, top: i32, right: i32, bottom: i32,
        nXStartArc: i32, nYStartArc: i32,
        nXEndArc:   i32, nYEndArc:   i32
    ): i32

    /// Sets a single pixel to `color` (COLORREF). Returns the
    /// previous color at that position.
    @lib pub fn SetPixel(hdc: *void, X: i32, Y: i32, color: u32): u32

    /// Returns the COLORREF of the pixel at `(X, Y)`, or
    /// `0xFFFFFFFF` (CLR_INVALID) on error.
    @lib pub fn GetPixel(hdc: *void, X: i32, Y: i32): u32

    // ── Text output ───────────────────────────────────────────────

    /// Writes `cbString` bytes of `lpString` at `(nXStart, nYStart)`.
    @lib pub fn TextOutA(
        hdc:      *void,
        nXStart:  i32,
        nYStart:  i32,
        lpString: *const char,
        cbString: i32
    ): i32

    /// Draws formatted text within `lpRect`. `nCount` = −1 to use
    /// the NUL-terminated length. Returns height of the text in
    /// logical units, or 0 on error.
    @lib pub fn DrawTextA(
        hdc:     *void,
        lpchText: *const char,
        nCount:  i32,
        lpRect:  *void,   // *RECT
        uFormat: u32
    ): i32

    /// Fills `lpSize` with the width and height of `lpString` using
    /// the DC's current font.
    @lib pub fn GetTextExtentPoint32A(
        hdc:      *void,
        lpString: *const char,
        cbString: i32,
        lpSize:   *SIZE
    ): i32

    @lib pub fn SetTextAlign(hdc: *void, fMode: u32): u32
    @lib pub fn GetTextAlign(hdc: *void): u32

    /// Fills `lptm` with font metrics for the DC's current font.
    @lib pub fn GetTextMetricsA(hdc: *void, lptm: *TEXTMETRICA): i32

    // ── Font ──────────────────────────────────────────────────────

    /// Creates a logical font from explicit parameters. Most
    /// callers prefer `CreateFontIndirectA` with a filled LOGFONTA.
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
    ): *HFONT

    @lib pub fn CreateFontIndirectA(lplf: *const LOGFONTA): *HFONT

    // ── Bitmap ────────────────────────────────────────────────────

    /// Creates a device-compatible bitmap with the same colour format
    /// as `hdc`, sized `nWidth` × `nHeight` pixels.
    @lib pub fn CreateCompatibleBitmap(hdc: *void, nWidth: i32, nHeight: i32): *HBITMAP

    /// Creates a DDB with explicitly specified planes and bits-per-pixel.
    @lib pub fn CreateBitmap(
        nWidth:     i32,
        nHeight:    i32,
        nPlanes:    u32,
        nBitCount:  u32,
        lpBits:     *const void
    ): *HBITMAP

    /// Block-transfers `nWidth`×`nHeight` pixels from `hdcSrc`
    /// to `hdcDest`. `dwRop`: RasterOp.srcCopy for a plain copy.
    @lib pub fn BitBlt(
        hdcDest: *void,
        nXDest: i32, nYDest: i32,
        nWidth: i32, nHeight: i32,
        hdcSrc: *void,
        nXSrc: i32, nYSrc: i32,
        dwRop: u32
    ): i32

    /// Block-transfers with scaling. `iStretchMode`: StretchBltMode.*.
    @lib pub fn StretchBlt(
        hdcDest: *void,
        nXOriginDest: i32, nYOriginDest: i32,
        nWidthDest: i32, nHeightDest: i32,
        hdcSrc: *void,
        nXOriginSrc: i32, nYOriginSrc: i32,
        nWidthSrc: i32, nHeightSrc: i32,
        dwRop: u32
    ): i32

    @lib pub fn SetStretchBltMode(hdc: *void, iStretchMode: i32): i32

    // ── Region ────────────────────────────────────────────────────

    @lib pub fn CreateRectRgn(left: i32, top: i32, right: i32, bottom: i32): *HRGN
    @lib pub fn CreateEllipticRgn(left: i32, top: i32, right: i32, bottom: i32): *HRGN
    @lib pub fn CreateRoundRectRgn(
        left: i32, top: i32, right: i32, bottom: i32,
        widthEllipse: i32, heightEllipse: i32
    ): *HRGN
    @lib pub fn CreatePolygonRgn(lpPoints: *void, nCount: i32, fnPolyFillMode: i32): *HRGN

    /// Combines `hrgnSrc1` and `hrgnSrc2` into `hrgnDest` using
    /// `fnCombineMode` (CombineRgnMode.*). Returns RegionResult.*.
    @lib pub fn CombineRgn(hrgnDest: *HRGN, hrgnSrc1: *HRGN, hrgnSrc2: *HRGN, fnCombineMode: i32): i32

    /// Replaces the DC clipping region with `hRgn`.
    /// Pass `0 as *HRGN` to remove clipping.
    @lib pub fn SelectClipRgn(hdc: *void, hRgn: *HRGN): i32

    @lib pub fn OffsetRgn(hRgn: *HRGN, nXOffset: i32, nYOffset: i32): i32

    // ── Path ──────────────────────────────────────────────────────

    @lib pub fn BeginPath(hdc: *void): i32
    @lib pub fn EndPath(hdc: *void): i32
    @lib pub fn StrokePath(hdc: *void): i32
    @lib pub fn FillPath(hdc: *void): i32
    @lib pub fn StrokeAndFillPath(hdc: *void): i32
    @lib pub fn PathToRegion(hdc: *void): *HRGN

    // ── Coordinate transforms ─────────────────────────────────────

    @lib pub fn SetMapMode(hdc: *void, fnMapMode: i32): i32
    @lib pub fn GetMapMode(hdc: *void): i32

    @lib pub fn SetViewportOrgEx(hdc: *void, X: i32, Y: i32, lpPoint: *void): i32
    @lib pub fn SetWindowOrgEx(hdc: *void, X: i32, Y: i32, lpPoint: *void): i32
    @lib pub fn SetViewportExtEx(hdc: *void, nXExtent: i32, nYExtent: i32, lpSize: *void): i32
    @lib pub fn SetWindowExtEx(hdc: *void, nXExtent: i32, nYExtent: i32, lpSize: *void): i32
    @lib pub fn GetViewportOrgEx(hdc: *void, lpPoint: *void): i32
    @lib pub fn GetWindowOrgEx(hdc: *void, lpPoint: *void): i32

    // ── Device capabilities ───────────────────────────────────────

    /// Returns a capability value for the DC.  Common `nIndex` values:
    ///   HORZRES=8, VERTRES=10, BITSPIXEL=12, PLANES=14,
    ///   LOGPIXELSX=88, LOGPIXELSY=90, SIZEPALETTE=104.
    @lib pub fn GetDeviceCaps(hdc: *void, nIndex: i32): i32
}
