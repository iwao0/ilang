/// d2d1.dll — Direct2D rendering: hardware-accelerated 2D drawing.
///
/// Bring-up:
///   1. D2D1CreateFactory(SingleThreaded, &IID_ID2D1Factory, NULL, &factory)
///   2. factory.CreateHwndRenderTarget(...)   or
///      factory.CreateDxgiSurfaceRenderTarget(...)
///   3. rt.BeginDraw / rt.DrawXxx / rt.EndDraw
///
/// Pair with `dwrite` for text. Heavy COM surface is deferred —
/// this file exposes the factory entry point + structs / enums
/// needed to feed the descriptors.

// ─── D2D1_FACTORY_TYPE ───────────────────────────────────────────
pub enum D2dFactoryType: i32 {
    singleThreaded = 0
    multiThreaded  = 1
}

// ─── D2D1_RENDER_TARGET_TYPE ─────────────────────────────────────
pub enum D2dRenderTargetType: i32 {
    default_ = 0
    software = 1
    hardware = 2
}

// ─── D2D1_RENDER_TARGET_USAGE ────────────────────────────────────
@flags
pub enum D2dRenderTargetUsage: u32 {
    none            = 0
    forceBitmapRemoting = 1
    gdiCompatible   = 2
}

// ─── D2D1_FEATURE_LEVEL ──────────────────────────────────────────
pub enum D2dFeatureLevel: u32 {
    default_  = 0
    level_9   = 0x9100
    level_10  = 0xA000
}

// ─── D2D1_ALPHA_MODE ─────────────────────────────────────────────
pub enum D2dAlphaMode: i32 {
    unknown       = 0
    premultiplied = 1
    straight      = 2
    ignore        = 3
}

// ─── D2D1_ANTIALIAS_MODE ─────────────────────────────────────────
pub enum D2dAntialiasMode: i32 {
    perPrimitive = 0
    aliased      = 1
}

// ─── D2D1_CAP_STYLE ──────────────────────────────────────────────
pub enum D2dCapStyle: i32 {
    flat      = 0
    square    = 1
    round     = 2
    triangle  = 3
}

// ─── D2D1_LINE_JOIN ──────────────────────────────────────────────
pub enum D2dLineJoin: i32 {
    miter        = 0
    bevel        = 1
    round        = 2
    miterOrBevel = 3
}

// ─── D2D1_PRESENT_OPTIONS ────────────────────────────────────────
@flags
pub enum D2dPresentOptions: u32 {
    none            = 0
    retainContents  = 1
    immediately     = 2
}

use windef { HWND }
@extern(C, "d2d1") {
    /// 32-bpp pixel format spec used by render targets / bitmaps.
    pub struct D2D1_PIXEL_FORMAT {
        format:    u32      // DXGI_FORMAT
        alphaMode: i32      // D2dAlphaMode
    }

    pub struct D2D1_POINT_2F {
        x: f32
        y: f32
    }

    pub struct D2D1_SIZE_F {
        width:  f32
        height: f32
    }

    pub struct D2D1_SIZE_U {
        width:  u32
        height: u32
    }

    pub struct D2D1_RECT_F {
        left:   f32
        top:    f32
        right:  f32
        bottom: f32
    }

    pub struct D2D1_COLOR_F {
        r: f32
        g: f32
        b: f32
        a: f32
    }

    pub struct D2D1_MATRIX_3X2_F {
        _11: f32
        _12: f32
        _21: f32
        _22: f32
        _31: f32
        _32: f32
    }

    /// Render-target descriptor. Used in CreateHwndRenderTarget,
    /// CreateDxgiSurfaceRenderTarget, CreateWicBitmapRenderTarget.
    pub struct D2D1_RENDER_TARGET_PROPERTIES {
        type_:        i32                  // D2dRenderTargetType
        pixelFormat:  D2D1_PIXEL_FORMAT
        dpiX:         f32
        dpiY:         f32
        usage:        u32                  // D2dRenderTargetUsage
        minLevel:     u32                  // D2dFeatureLevel
    }

    /// HWND-specific render-target properties.
    pub struct D2D1_HWND_RENDER_TARGET_PROPERTIES {
        hwnd:           HWND
        pixelSize:      D2D1_SIZE_U
        presentOptions: u32                // D2dPresentOptions
    }

    /// `D2D1CreateFactory(type, iid, options, factory)`. `iid`
    /// points to the IID of the requested factory interface; pass
    /// an `ole32.GUID` for IID_ID2D1Factory.
    /// `options` may be NULL.
    @lib pub fn D2D1CreateFactory(
        factoryType: i32,
        riid:        *const void,
        pOptions:    *const void,
        factory:     **void
    ): i32

    /// Convenience entry to convert sRGB float → 32-bit ARGB.
    @lib pub fn D2D1MakeRotateMatrix(
        angle: f32,
        center: D2D1_POINT_2F,
        matrix: *D2D1_MATRIX_3X2_F
    )

    @lib pub fn D2D1MakeSkewMatrix(
        angleX: f32,
        angleY: f32,
        center: D2D1_POINT_2F,
        matrix: *D2D1_MATRIX_3X2_F
    )

    @lib pub fn D2D1IsMatrixInvertible(matrix: *const D2D1_MATRIX_3X2_F): i32
    @lib pub fn D2D1InvertMatrix(matrix: *D2D1_MATRIX_3X2_F): i32
}
