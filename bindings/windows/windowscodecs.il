/// windowscodecs.dll — WIC (Windows Imaging Component).
///
/// Universal decoder / encoder for PNG / JPG / GIF / BMP / TIFF /
/// DDS / WEBP / HEIF / JPEG XR.
///
/// Decode pattern:
///   CoInitializeEx(NULL, COINIT_MULTITHREADED)
///   CoCreateInstance(&CLSID_WICImagingFactory, NULL,
///                    CLSCTX_INPROC_SERVER, &IID_IWICImagingFactory,
///                    &factory)
///   factory.CreateDecoderFromFilename(L"img.png", NULL,
///                                     GENERIC_READ,
///                                     WICDecodeMetadataCacheOnDemand,
///                                     &decoder)
///   decoder.GetFrame(0, &frame)
///   frame.GetSize(&w, &h)
///   factory.CreateFormatConverter(&conv)
///   conv.Initialize(frame, &GUID_WICPixelFormat32bppPBGRA,
///                   WICBitmapDitherTypeNone, NULL, 0.0,
///                   WICBitmapPaletteTypeCustom)
///   conv.CopyPixels(NULL, stride, bufSize, buf)
///
/// COM interface methods (IWICImagingFactory etc.) aren't bound at
/// the ilang call level — call them via the vtable struct.

// ─── WICDecodeOptions ────────────────────────────────────────────
pub enum WicDecodeOptions: u32 {
    cacheOnDemand = 0
    cacheOnLoad   = 1
}

// ─── WICBitmapDitherType ─────────────────────────────────────────
pub enum WicBitmapDitherType: u32 {
    none            = 0
    solid           = 0
    ordered4x4      = 1
    ordered8x8      = 2
    ordered16x16    = 3
    spiral4x4       = 4
    spiral8x8       = 5
    dualSpiral4x4   = 6
    dualSpiral8x8   = 7
    errorDiffusion  = 8
}

// ─── WICBitmapPaletteType ────────────────────────────────────────
pub enum WicBitmapPaletteType: u32 {
    custom            = 0
    medianCut         = 1
    fixedBW           = 2
    fixedHalftone8    = 3
    fixedHalftone27   = 4
    fixedHalftone64   = 5
    fixedHalftone125  = 6
    fixedHalftone216  = 7
    fixedWebPalette   = 7
    fixedHalftone252  = 8
    fixedHalftone256  = 9
    fixedGray4        = 10
    fixedGray16       = 11
    fixedGray256      = 12
}

// ─── WICBitmapCreateCacheOption ──────────────────────────────────
pub enum WicBitmapCreateCacheOption: u32 {
    noCache    = 0
    cacheOnDemand = 1
    cacheOnLoad   = 2
}

// ─── WICBitmapTransformOptions (flags) ───────────────────────────
@flags
pub enum WicBitmapTransformOptions: u32 {
    rotate0       = 0
    rotate90      = 1
    rotate180     = 2
    rotate270     = 3
    flipHorizontal = 8
    flipVertical   = 16
}

// ─── WICBitmapLockFlags ──────────────────────────────────────────
@flags
pub enum WicBitmapLockFlags: u32 {
    read  = 0x1
    write = 0x2
}

@extern(C, "windowscodecs") {
    /// 8-byte (width, height) helper struct (returned by methods
    /// that take separate u32 out-pointers in the C surface).
    pub struct WICRect {
        x:      i32
        y:      i32
        width:  i32
        height: i32
    }

    /// Color context creation helper — most callers route through
    /// the factory instead of calling these directly.
    @lib pub fn WICConvertBitmapSource(
        dstFormat:    *const void,   // *GUID — WICPixelFormat*
        pISrc:        *void,         // *IWICBitmapSource
        ppIDst:       **void
    ): i32

    /// Get the bits-per-pixel of a `WICPixelFormat*` GUID.
    @lib pub fn WICGetMetadataContentSize(
        guidContainerFormat: *const void,
        pIWriter:            *void,
        pcbSize:             *u64
    ): i32

    /// One-shot create of the imaging factory — wraps the
    /// CoCreateInstance call. Prefer this when you don't need
    /// CLSCTX customisation.
    @lib pub fn WICCreateImagingFactory_Proxy(
        SDKVersion: u32,            // WINCODEC_SDK_VERSION = 0x0236
        ppIImagingFactory: **void
    ): i32
}
