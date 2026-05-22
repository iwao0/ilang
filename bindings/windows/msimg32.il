/// msimg32.dll — Microsoft Image Helpers: alpha blending and gradient fills.
/// These functions were split out of gdi32.dll starting with Windows 98 / 2000.

// ─── GradientFill dwMode values ──────────────────────────────────
pub enum GradientFillMode: u32 {
    rectH    = 0   // horizontal gradient rectangle
    rectV    = 1   // vertical gradient rectangle
    triangle = 2   // filled gradient triangle
}

use user32 { HDC }

@extern(C, "msimg32") {
    // ── BLENDFUNCTION — 4 bytes ───────────────────────────────────

    /// Alpha-blending parameters. Pass by value packed into a u32,
    /// or take a pointer.
    /// `BlendOp`   = 0 (AC_SRC_OVER — only supported value).
    /// `BlendFlags`= 0.
    /// `SourceConstantAlpha` = 255 to use per-pixel alpha from the source.
    /// `AlphaFormat`= 1 (AC_SRC_ALPHA) if source has premultiplied alpha;
    ///               = 0 for constant-alpha blending.
    pub struct BLENDFUNCTION {
        BlendOp:             u8
        BlendFlags:          u8
        SourceConstantAlpha: u8
        AlphaFormat:         u8
    }

    // ── TRIVERTEX — 16 bytes ──────────────────────────────────────

    /// One vertex of a GradientFill rectangle or triangle.
    /// Colour channels are 16-bit (range 0x0000–0xFF00); alpha is
    /// only used if the DC supports it.
    pub struct TRIVERTEX {
        x:     i32
        y:     i32
        Red:   u16
        Green: u16
        Blue:  u16
        Alpha: u16
    }

    // ── Alpha blending ────────────────────────────────────────────

    /// Blends `hdcSrc` onto `hdcDest` with per-pixel or constant alpha.
    /// Pass BLENDFUNCTION packed into a u32:
    ///   `let bf = BLENDFUNCTION { BlendOp:0, BlendFlags:0, SourceConstantAlpha:255, AlphaFormat:1 }`
    ///   then `AlphaBlend(..., bf as u32)`.
    @lib pub fn AlphaBlend(
        hdcDest:      HDC,
        xOriginDest:  i32,
        yOriginDest:  i32,
        wDest:        i32,
        hDest:        i32,
        hdcSrc:       HDC,
        xOriginSrc:   i32,
        yOriginSrc:   i32,
        wSrc:         i32,
        hSrc:         i32,
        ftn:          u32     // BLENDFUNCTION cast to u32
    ): i32

    // ── Gradient fill ─────────────────────────────────────────────

    /// Fills rectangles or triangles with smooth colour gradients.
    ///
    /// `pVertex`:  array of `nVertex` TRIVERTEX values.
    /// `pMesh`:    array of `nMesh` index structs:
    ///   - RECT mode:     pairs   `{ u32 UpperLeft, u32 LowerRight }`
    ///   - TRIANGLE mode: triples `{ u32 Vertex1, u32 Vertex2, u32 Vertex3 }`
    /// `dwMode`:   GradientFillMode.*
    @lib pub fn GradientFill(
        hdc:     HDC,
        pVertex: *TRIVERTEX,
        nVertex: u32,
        pMesh:   *void,
        nMesh:   u32,
        dwMode:  u32
    ): i32

    // ── Transparent blit ─────────────────────────────────────────

    /// Copies `hdcSrc` to `hdcDest`, treating pixels with colour
    /// `crTransparent` as transparent (not copied).
    @lib pub fn TransparentBlt(
        hdcDest:     HDC,
        xoriginDest: i32,
        yoriginDest: i32,
        wDest:       i32,
        hDest:       i32,
        hdcSrc:      HDC,
        xoriginSrc:  i32,
        yoriginSrc:  i32,
        wSrc:        i32,
        hSrc:        i32,
        crTransparent: u32
    ): i32
}
