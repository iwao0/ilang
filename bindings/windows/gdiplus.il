// gdiplus.dll — GDI+ flat C API.
//
// Higher-level than GDI: alpha-blended drawing, image codecs
// (PNG/JPG/GIF/TIFF/BMP), gradient brushes, anti-aliased paths.
//
// Usage:
//   let mut tok: u64 = 0
//   let mut input = GdiplusStartupInput { GdiplusVersion: 1, ... }
//   GdiplusStartup(&tok, &input, 0 as *void)
//   ...
//   GdiplusShutdown(tok)
//
// All `Gdip*` functions return a GpStatus (i32). 0 = Ok.

// ─── GpStatus return codes ───────────────────────────────────────
pub enum GpStatus: i32 {
    ok                = 0
    genericError      = 1
    invalidParameter  = 2
    outOfMemory       = 3
    objectBusy        = 4
    insufficientBuffer = 5
    notImplemented    = 6
    win32Error        = 7
    wrongState        = 8
    aborted           = 9
    fileNotFound      = 10
    valueOverflow     = 11
    accessDenied      = 12
    unknownImageFormat = 13
    fontFamilyNotFound = 14
    fontStyleNotFound = 15
    notTrueTypeFont   = 16
    unsupportedGdiplusVersion = 17
    gdiplusNotInitialized = 18
    propertyNotFound  = 19
    propertyNotSupported = 20
    profileNotFound   = 21
}

// ─── SmoothingMode ───────────────────────────────────────────────
pub enum SmoothingMode: i32 {
    invalid     = -1
    default     = 0
    highSpeed   = 1
    highQuality = 2
    none_       = 3
    antiAlias   = 4
}

// ─── InterpolationMode ───────────────────────────────────────────
pub enum InterpolationMode: i32 {
    invalid       = -1
    default       = 0
    lowQuality    = 1
    highQuality   = 2
    bilinear      = 3
    bicubic       = 4
    nearestNeighbor = 5
    hqBilinear    = 6
    hqBicubic     = 7
}

// ─── CompositingMode ─────────────────────────────────────────────
pub enum CompositingMode: i32 {
    sourceOver = 0
    sourceCopy = 1
}

// ─── CompositingQuality ──────────────────────────────────────────
pub enum CompositingQuality: i32 {
    invalid      = -1
    default      = 0
    highSpeed    = 1
    highQuality  = 2
    gammaCorrected = 3
    assumeLinear = 4
}

// ─── PixelOffsetMode ─────────────────────────────────────────────
pub enum PixelOffsetMode: i32 {
    invalid    = -1
    default    = 0
    highSpeed  = 1
    highQuality = 2
    none_      = 3
    half_      = 4
}

// ─── TextRenderingHint ───────────────────────────────────────────
pub enum TextRenderingHint: i32 {
    systemDefault           = 0
    singleBitPerPixelGridFit = 1
    singleBitPerPixel       = 2
    antiAliasGridFit        = 3
    antiAlias               = 4
    clearTypeGridFit        = 5
}

// ─── FillMode ────────────────────────────────────────────────────
pub enum FillMode: i32 {
    alternate = 0
    winding   = 1
}

// ─── ImageFormat GUIDs are required for SaveAdd; for SaveImageToFile
// they are passed via CLSID of the encoder. See GdipGetImageEncodersSize.

// ─── Unit ────────────────────────────────────────────────────────
pub enum GpUnit: i32 {
    world     = 0
    display   = 1
    pixel     = 2
    point     = 3
    inch      = 4
    document  = 5
    millimeter = 6
}

@extern(C, "gdiplus") {
    // ── Opaque handles ────────────────────────────────────────────

    /// Graphics object — drawing context (analogous to HDC).
    pub struct GpGraphics {}
    pub struct GpImage {}
    pub struct GpBitmap {}
    pub struct GpPen {}
    pub struct GpBrush {}
    pub struct GpSolidFill {}
    pub struct GpLineGradient {}
    pub struct GpPath {}
    pub struct GpFontFamily {}
    pub struct GpFont {}
    pub struct GpStringFormat {}
    pub struct GpMatrix {}
    pub struct GpRegion {}

    // ── GdiplusStartupInput — 16 bytes ───────────────────────────

    /// Pass to GdiplusStartup. Set `GdiplusVersion` = 1.
    pub struct GdiplusStartupInput {
        GdiplusVersion:           u32   // = 1
        DebugEventCallback:       *void
        SuppressBackgroundThread: i32
        SuppressExternalCodecs:   i32
    }

    // ── Startup / shutdown ────────────────────────────────────────

    /// Initialises GDI+ for the calling thread. Pair with GdiplusShutdown.
    /// `output`: only used if SuppressBackgroundThread is non-zero.
    @lib pub fn GdiplusStartup(
        token:  *u64,
        input:  *const GdiplusStartupInput,
        output: *void
    ): i32

    @lib pub fn GdiplusShutdown(token: u64)

    // ── Graphics object lifecycle ─────────────────────────────────

    /// Creates a Graphics object from an HDC.
    @lib pub fn GdipCreateFromHDC(hdc: *void, graphics: **GpGraphics): i32
    @lib pub fn GdipCreateFromHWND(hwnd: i64, graphics: **GpGraphics): i32
    @lib pub fn GdipDeleteGraphics(graphics: *GpGraphics): i32

    @lib pub fn GdipGetDC(graphics: *GpGraphics, hdc: **void): i32
    @lib pub fn GdipReleaseDC(graphics: *GpGraphics, hdc: *void): i32

    /// Sets quality / smoothing modes.
    @lib pub fn GdipSetSmoothingMode(graphics: *GpGraphics, mode: i32): i32
    @lib pub fn GdipSetInterpolationMode(graphics: *GpGraphics, mode: i32): i32
    @lib pub fn GdipSetCompositingMode(graphics: *GpGraphics, mode: i32): i32
    @lib pub fn GdipSetCompositingQuality(graphics: *GpGraphics, q: i32): i32
    @lib pub fn GdipSetPixelOffsetMode(graphics: *GpGraphics, mode: i32): i32
    @lib pub fn GdipSetTextRenderingHint(graphics: *GpGraphics, mode: i32): i32

    // ── Image (PNG/JPG/GIF/TIFF/BMP/EMF) ─────────────────────────

    /// Loads an image from a file path. Supports PNG/JPG/GIF/TIFF/BMP/EMF.
    @lib pub fn GdipLoadImageFromFile(filename: *const u16, image: **GpImage): i32

    /// Loads from an IStream (e.g. CreateStreamOnHGlobal).
    @lib pub fn GdipLoadImageFromStream(stream: *void, image: **GpImage): i32

    @lib pub fn GdipDisposeImage(image: *GpImage): i32

    @lib pub fn GdipGetImageWidth(image: *GpImage, width: *u32): i32
    @lib pub fn GdipGetImageHeight(image: *GpImage, height: *u32): i32

    /// Saves to a file. `clsidEncoder`: pointer to a CLSID GUID
    /// (use GdipGetImageEncoders to discover encoders).
    @lib pub fn GdipSaveImageToFile(
        image:         *GpImage,
        filename:      *const u16,
        clsidEncoder:  *const void,    // *CLSID
        encoderParams: *const void
    ): i32

    /// Enumerates registered image encoders.
    @lib pub fn GdipGetImageEncodersSize(numEncoders: *u32, size: *u32): i32
    @lib pub fn GdipGetImageEncoders(
        numEncoders: u32,
        size:        u32,
        encoders:    *void        // *ImageCodecInfo array
    ): i32

    // ── Drawing images ───────────────────────────────────────────

    /// Draws `image` at `(x, y)` in graphics-coordinate space.
    @lib pub fn GdipDrawImage(graphics: *GpGraphics, image: *GpImage, x: f32, y: f32): i32

    /// Draws `image` scaled to `(x, y, width, height)`.
    @lib pub fn GdipDrawImageRect(
        graphics: *GpGraphics,
        image:    *GpImage,
        x:        f32,
        y:        f32,
        width:    f32,
        height:   f32
    ): i32

    /// Most general draw: source rect → dest rect.
    @lib pub fn GdipDrawImageRectRect(
        graphics:  *GpGraphics,
        image:     *GpImage,
        dstx:      f32, dsty: f32, dstwidth: f32, dstheight: f32,
        srcx:      f32, srcy: f32, srcwidth: f32, srcheight: f32,
        srcUnit:   i32,    // GpUnit.*
        imageAttr: *const void,
        callback:  *void,
        cbData:    *void
    ): i32

    // ── Bitmaps ───────────────────────────────────────────────────

    /// Creates an empty bitmap with the given pixel format.
    /// `format`: 0x21808 (PixelFormat32bppARGB), 0x22009 (32bppRGB), etc.
    @lib pub fn GdipCreateBitmapFromScan0(
        width:    i32,
        height:   i32,
        stride:   i32,
        format:   i32,
        scan0:    *u8,
        bitmap:   **GpBitmap
    ): i32

    /// Loads a bitmap from a file (same formats as GdipLoadImageFromFile).
    @lib pub fn GdipCreateBitmapFromFile(filename: *const u16, bitmap: **GpBitmap): i32

    /// Wraps a Win32 HBITMAP. `hpal` can be NULL.
    @lib pub fn GdipCreateBitmapFromHBITMAP(hbm: *void, hpal: *void, bitmap: **GpBitmap): i32

    /// Returns an HBITMAP copy (caller must DeleteObject it).
    @lib pub fn GdipCreateHBITMAPFromBitmap(
        bitmap:    *GpBitmap,
        hbmReturn: **void,
        background: u32
    ): i32

    // ── Pen / brush ───────────────────────────────────────────────

    @lib pub fn GdipCreatePen1(color: u32, width: f32, unit: i32, pen: **GpPen): i32
    @lib pub fn GdipDeletePen(pen: *GpPen): i32
    @lib pub fn GdipSetPenColor(pen: *GpPen, color: u32): i32
    @lib pub fn GdipSetPenWidth(pen: *GpPen, width: f32): i32

    @lib pub fn GdipCreateSolidFill(color: u32, brush: **GpSolidFill): i32
    @lib pub fn GdipDeleteBrush(brush: *GpBrush): i32

    /// Creates a linear gradient brush between two ARGB colors.
    @lib pub fn GdipCreateLineBrushFromRectI(
        rect:     *const void,    // *Rect (i32 ×4)
        color1:   u32,
        color2:   u32,
        mode:     i32,            // 0=horizontal, 1=vertical, 2=fwd diag, 3=back diag
        wrapMode: i32,            // 0=tile, 1=tileFlipX, ...
        brush:    **GpLineGradient
    ): i32

    // ── Drawing primitives ────────────────────────────────────────

    @lib pub fn GdipDrawLine(
        graphics: *GpGraphics, pen: *GpPen,
        x1: f32, y1: f32, x2: f32, y2: f32
    ): i32

    @lib pub fn GdipDrawRectangle(
        graphics: *GpGraphics, pen: *GpPen,
        x: f32, y: f32, width: f32, height: f32
    ): i32

    @lib pub fn GdipFillRectangle(
        graphics: *GpGraphics, brush: *GpBrush,
        x: f32, y: f32, width: f32, height: f32
    ): i32

    @lib pub fn GdipDrawEllipse(
        graphics: *GpGraphics, pen: *GpPen,
        x: f32, y: f32, width: f32, height: f32
    ): i32

    @lib pub fn GdipFillEllipse(
        graphics: *GpGraphics, brush: *GpBrush,
        x: f32, y: f32, width: f32, height: f32
    ): i32

    @lib pub fn GdipDrawArc(
        graphics: *GpGraphics, pen: *GpPen,
        x: f32, y: f32, width: f32, height: f32,
        startAngle: f32, sweepAngle: f32
    ): i32

    @lib pub fn GdipDrawBezier(
        graphics: *GpGraphics, pen: *GpPen,
        x1: f32, y1: f32, x2: f32, y2: f32,
        x3: f32, y3: f32, x4: f32, y4: f32
    ): i32

    @lib pub fn GdipDrawLines(
        graphics: *GpGraphics, pen: *GpPen,
        points: *const void,    // *PointF array (f32 ×2 ×count)
        count:  i32
    ): i32

    @lib pub fn GdipFillPolygon(
        graphics: *GpGraphics, brush: *GpBrush,
        points: *const void,
        count:  i32,
        fillMode: i32     // FillMode.*
    ): i32

    // ── Graphics path ─────────────────────────────────────────────

    @lib pub fn GdipCreatePath(brushMode: i32, path: **GpPath): i32
    @lib pub fn GdipDeletePath(path: *GpPath): i32
    @lib pub fn GdipResetPath(path: *GpPath): i32
    @lib pub fn GdipClosePathFigure(path: *GpPath): i32
    @lib pub fn GdipAddPathLine(path: *GpPath, x1: f32, y1: f32, x2: f32, y2: f32): i32
    @lib pub fn GdipAddPathArc(
        path: *GpPath,
        x: f32, y: f32, width: f32, height: f32,
        startAngle: f32, sweepAngle: f32
    ): i32
    @lib pub fn GdipAddPathBezier(
        path: *GpPath,
        x1: f32, y1: f32, x2: f32, y2: f32,
        x3: f32, y3: f32, x4: f32, y4: f32
    ): i32

    @lib pub fn GdipDrawPath(graphics: *GpGraphics, pen: *GpPen, path: *GpPath): i32
    @lib pub fn GdipFillPath(graphics: *GpGraphics, brush: *GpBrush, path: *GpPath): i32

    // ── Fonts / text ──────────────────────────────────────────────

    @lib pub fn GdipCreateFontFamilyFromName(
        name:   *const u16,
        fontCollection: *void,
        family: **GpFontFamily
    ): i32

    @lib pub fn GdipDeleteFontFamily(family: *GpFontFamily): i32

    @lib pub fn GdipCreateFont(
        family:   *GpFontFamily,
        emSize:   f32,
        style:    i32,
        unit:     i32,
        font:     **GpFont
    ): i32

    @lib pub fn GdipDeleteFont(font: *GpFont): i32

    /// Draws a UTF-16 string. `layoutRect`: *RectF (f32 ×4).
    @lib pub fn GdipDrawString(
        graphics:    *GpGraphics,
        text:        *const u16,
        length:      i32,
        font:        *GpFont,
        layoutRect:  *const void,
        stringFormat: *const void,
        brush:       *GpBrush
    ): i32

    /// Measures a UTF-16 string. `boundingBox`: receives layout.
    @lib pub fn GdipMeasureString(
        graphics:    *GpGraphics,
        text:        *const u16,
        length:      i32,
        font:        *GpFont,
        layoutRect:  *const void,
        stringFormat: *const void,
        boundingBox: *void,
        codepointsFitted: *i32,
        linesFilled: *i32
    ): i32

    // ── Transformations ───────────────────────────────────────────

    @lib pub fn GdipTranslateWorldTransform(graphics: *GpGraphics, dx: f32, dy: f32, order: i32): i32
    @lib pub fn GdipRotateWorldTransform(graphics: *GpGraphics, angle: f32, order: i32): i32
    @lib pub fn GdipScaleWorldTransform(graphics: *GpGraphics, sx: f32, sy: f32, order: i32): i32
    @lib pub fn GdipResetWorldTransform(graphics: *GpGraphics): i32
}
