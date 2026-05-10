// SDL2_ttf — TrueType / OpenType font loading and text rendering.
// Separate library (`libSDL2_ttf`) — must be installed alongside
// SDL2. Initialise with `sdl.ttfInit()` after `sdl.init(...)`,
// tear down with `sdl.ttfQuit()` before `sdl.quit()`. The `ttf`
// prefix on the lifecycle / error fns avoids collisions with the
// `sdl_core` `init` / `quit` / `getError` of the same name.
//
// Render fns produce `*SDL_Surface` (CPU-side bitmaps). Convert
// to a GPU `Texture` for fast on-screen drawing via
// `Texture.fromSurface(renderer, surfaceHandle)`, or wrap the
// raw handle in `sdl_surface.Surface.fromHandle(...)` for
// ARC-managed lifetime.

use sdl_core { Color, Size }

/// `TTF_STYLE_*` — text-decoration bitflags. AND-combine with `|`
/// when passing to `Font.setStyle`.
@flags
pub enum FontStyle: i32 {
    normal        = 0x00
    bold          = 0x01
    italic        = 0x02
    underline     = 0x04
    strikethrough = 0x08
}

/// `TTF_HINTING_*` — TrueType hinting modes. `lightSubpixel` is
/// SDL2_ttf 2.0.18+ only.
pub enum FontHinting: i32 {
    normal        = 0
    light         = 1
    mono          = 2
    none          = 3
    lightSubpixel = 4
}

/// Outcome of `Font.glyphMetrics`. All values in pixels.
pub class GlyphMetrics {
    pub minX:    i32
    pub maxX:    i32
    pub minY:    i32
    pub maxY:    i32
    pub advance: i32
}

@extern(C) {
    /// Opaque `TTF_Font *`. Callers see `Font.handle` cast to i64.
    pub struct TTF_Font {}

    // ─── lifecycle ───────────────────────────────────────────────
    @lib("SDL2_ttf") pub fn TTF_Init(): i32
    @lib("SDL2_ttf") pub fn TTF_Quit()
    @lib("SDL2_ttf") pub fn TTF_WasInit(): i32
    @lib("SDL2_ttf") pub fn TTF_GetError(): *const char

    // ─── font open / close ───────────────────────────────────────
    @lib("SDL2_ttf") pub fn TTF_OpenFont(file: *const char, ptsize: i32): *TTF_Font
    @lib("SDL2_ttf") pub fn TTF_OpenFontIndex(
        file: *const char, ptsize: i32, index: i64
    ): *TTF_Font
    @lib("SDL2_ttf") pub fn TTF_CloseFont(font: *TTF_Font)

    // ─── font metrics ────────────────────────────────────────────
    @lib("SDL2_ttf") pub fn TTF_FontHeight(font: *TTF_Font): i32
    @lib("SDL2_ttf") pub fn TTF_FontAscent(font: *TTF_Font): i32
    @lib("SDL2_ttf") pub fn TTF_FontDescent(font: *TTF_Font): i32
    @lib("SDL2_ttf") pub fn TTF_FontLineSkip(font: *TTF_Font): i32
    @lib("SDL2_ttf") pub fn TTF_FontFaces(font: *TTF_Font): i64
    @lib("SDL2_ttf") pub fn TTF_FontFaceIsFixedWidth(font: *TTF_Font): i32
    @lib("SDL2_ttf") pub fn TTF_FontFaceFamilyName(font: *TTF_Font): *const char
    @lib("SDL2_ttf") pub fn TTF_FontFaceStyleName(font: *TTF_Font): *const char

    // ─── font configuration ──────────────────────────────────────
    @lib("SDL2_ttf") pub fn TTF_GetFontStyle(font: *TTF_Font): i32
    @lib("SDL2_ttf") pub fn TTF_SetFontStyle(font: *TTF_Font, style: i32)
    @lib("SDL2_ttf") pub fn TTF_GetFontOutline(font: *TTF_Font): i32
    @lib("SDL2_ttf") pub fn TTF_SetFontOutline(font: *TTF_Font, outline: i32)
    @lib("SDL2_ttf") pub fn TTF_GetFontHinting(font: *TTF_Font): i32
    @lib("SDL2_ttf") pub fn TTF_SetFontHinting(font: *TTF_Font, hinting: i32)
    @lib("SDL2_ttf") pub fn TTF_GetFontKerning(font: *TTF_Font): i32
    @lib("SDL2_ttf") pub fn TTF_SetFontKerning(font: *TTF_Font, allowed: i32)

    // ─── glyph queries ───────────────────────────────────────────
    @lib("SDL2_ttf") pub fn TTF_GlyphIsProvided(font: *TTF_Font, ch: u16): i32
    @lib("SDL2_ttf") pub fn TTF_GlyphMetrics(
        font: *TTF_Font, ch: u16,
        minx: *i32, maxx: *i32, miny: *i32, maxy: *i32, advance: *i32
    ): i32

    // ─── text size ───────────────────────────────────────────────
    @lib("SDL2_ttf") pub fn TTF_SizeUTF8(
        font: *TTF_Font, text: *const char, w: *i32, h: *i32
    ): i32

    // ─── rendering (each returns a fresh SDL_Surface*) ───────────
    // Returning `i64` (not `*SDL_Surface`) keeps this module
    // independent of `sdl_surface.il`'s struct layout — callers
    // either pass the i64 straight to `Texture.fromSurface(...)`
    // or wrap with `sdl_surface.Surface.fromHandle(...)` for ARC
    // lifetime management.
    @lib("SDL2_ttf") pub fn TTF_RenderUTF8_Solid(
        font: *TTF_Font, text: *const char, fg: Color
    ): i64
    @lib("SDL2_ttf") pub fn TTF_RenderUTF8_Shaded(
        font: *TTF_Font, text: *const char, fg: Color, bg: Color
    ): i64
    @lib("SDL2_ttf") pub fn TTF_RenderUTF8_Blended(
        font: *TTF_Font, text: *const char, fg: Color
    ): i64
    @lib("SDL2_ttf") pub fn TTF_RenderUTF8_Blended_Wrapped(
        font: *TTF_Font, text: *const char, fg: Color, wrapLength: u32
    ): i64

    // ─── friendlier ilang wrappers ───────────────────────────────

    /// Initialise SDL2_ttf. Call exactly once after `sdl.init(...)`.
    ///
    /// Returns:
    ///   `0` on success, `< 0` on failure (call `ttfGetError()`).
    pub fn ttfInit(): i32 {
        TTF_Init()
    }

    /// Tear down SDL2_ttf. Call exactly once before `sdl.quit()`.
    pub fn ttfQuit() {
        TTF_Quit()
    }

    /// Returns:
    ///   true when `ttfInit()` has been called and `ttfQuit()`
    ///   hasn't undone it yet.
    pub fn ttfWasInit(): bool {
        TTF_WasInit() != 0
    }

    /// Returns:
    ///   the latest SDL2_ttf error message. SDL_ttf routes most of
    ///   its errors through `SDL_SetError`, so `sdl.getError()`
    ///   works too — this exists for callers that prefer the
    ///   namespaced form.
    pub fn ttfGetError(): string {
        stringFromCstr(TTF_GetError())
    }

    /// ARC-managed `TTF_Font`. Construct via `new TtfFont(path,
    /// ptsize)`; `deinit` closes the underlying handle.
    pub class TtfFont {
        /// Raw `TTF_Font *` cast to i64. `0` when loading failed.
        pub handle: i64

        /// Open a font from disk.
        ///
        /// Args:
        ///   path   — filesystem path to a .ttf / .otf file.
        ///   ptsize — point size at which to rasterise.
        pub init(path: string, ptsize: i32) {
            this.handle = TTF_OpenFont(cstrFromString(path), ptsize) as i64
        }

        deinit() {
            if this.handle != 0 {
                TTF_CloseFont(this.handle as *TTF_Font)
            }
        }

        /// Returns:
        ///   pixel height of a tall character (`TTF_FontHeight`).
        pub height(): i32 {
            if this.handle == 0 { return 0 }
            TTF_FontHeight(this.handle as *TTF_Font)
        }

        /// Returns:
        ///   the ascent (positive distance above the baseline).
        pub ascent(): i32 {
            if this.handle == 0 { return 0 }
            TTF_FontAscent(this.handle as *TTF_Font)
        }

        /// Returns:
        ///   the descent (negative distance below the baseline).
        pub descent(): i32 {
            if this.handle == 0 { return 0 }
            TTF_FontDescent(this.handle as *TTF_Font)
        }

        /// Returns:
        ///   recommended pixel distance between baselines for
        ///   stacked lines of text (`TTF_FontLineSkip`).
        pub lineSkip(): i32 {
            if this.handle == 0 { return 0 }
            TTF_FontLineSkip(this.handle as *TTF_Font)
        }

        /// Returns:
        ///   number of font faces in the file (TTC collections can
        ///   carry multiple — use `TTF_OpenFontIndex` raw to pick
        ///   one). `1` for ordinary single-face fonts.
        pub faces(): i64 {
            if this.handle == 0 { return 0 }
            TTF_FontFaces(this.handle as *TTF_Font)
        }

        /// Returns:
        ///   true when every glyph in the font is the same width
        ///   (typewriter / monospace fonts).
        pub isFixedWidth(): bool {
            if this.handle == 0 { return false }
            TTF_FontFaceIsFixedWidth(this.handle as *TTF_Font) != 0
        }

        /// Returns:
        ///   the font's family name as embedded in the file
        ///   (e.g. `"DejaVu Sans"`).
        pub familyName(): string {
            if this.handle == 0 { return "" }
            stringFromCstr(TTF_FontFaceFamilyName(
                this.handle as *TTF_Font
            ))
        }

        /// Returns:
        ///   the font's style name (e.g. `"Regular"`, `"Bold Italic"`).
        pub styleName(): string {
            if this.handle == 0 { return "" }
            stringFromCstr(TTF_FontFaceStyleName(
                this.handle as *TTF_Font
            ))
        }

        /// Returns:
        ///   bitwise OR of `FontStyle` flags currently active.
        pub getStyle(): i32 {
            if this.handle == 0 { return 0 }
            TTF_GetFontStyle(this.handle as *TTF_Font)
        }

        /// Args:
        ///   style — bitwise OR of `FontStyle` flags.
        pub setStyle(style: i32) {
            if this.handle != 0 {
                TTF_SetFontStyle(this.handle as *TTF_Font, style)
            }
        }

        /// Returns:
        ///   outline thickness in pixels. `0` = no outline.
        pub getOutline(): i32 {
            if this.handle == 0 { return 0 }
            TTF_GetFontOutline(this.handle as *TTF_Font)
        }

        pub setOutline(outline: i32) {
            if this.handle != 0 {
                TTF_SetFontOutline(this.handle as *TTF_Font, outline)
            }
        }

        pub getHinting(): FontHinting {
            if this.handle == 0 { return FontHinting.normal }
            TTF_GetFontHinting(this.handle as *TTF_Font) as FontHinting
        }

        pub setHinting(hinting: FontHinting) {
            if this.handle != 0 {
                TTF_SetFontHinting(this.handle as *TTF_Font, hinting as i32)
            }
        }

        /// Returns:
        ///   true when kerning (per-pair spacing) is enabled.
        pub getKerning(): bool {
            if this.handle == 0 { return false }
            TTF_GetFontKerning(this.handle as *TTF_Font) != 0
        }

        pub setKerning(enabled: bool) {
            if this.handle != 0 {
                let v: i32 = if enabled { 1 } else { 0 }
                TTF_SetFontKerning(this.handle as *TTF_Font, v)
            }
        }

        /// Returns:
        ///   true if the font has a glyph for the given codepoint
        ///   (BMP only — `ch` is u16). False for `0` handle.
        pub hasGlyph(ch: u16): bool {
            if this.handle == 0 { return false }
            TTF_GlyphIsProvided(this.handle as *TTF_Font, ch) != 0
        }

        /// Returns:
        ///   pixel-level glyph metrics for `ch`. All-zero on
        ///   missing glyph or no font.
        pub glyphMetrics(ch: u16): GlyphMetrics {
            let m = new GlyphMetrics()
            if this.handle == 0 { return m }
            let xs0: i32[] = [0]
            let xs1: i32[] = [0]
            let ys0: i32[] = [0]
            let ys1: i32[] = [0]
            let adv: i32[] = [0]
            let pmx0: *i32 = xs0
            let pmx1: *i32 = xs1
            let pmy0: *i32 = ys0
            let pmy1: *i32 = ys1
            let pad: *i32 = adv
            TTF_GlyphMetrics(
                this.handle as *TTF_Font, ch,
                pmx0, pmx1, pmy0, pmy1, pad
            )
            m.minX = xs0[0]
            m.maxX = xs1[0]
            m.minY = ys0[0]
            m.maxY = ys1[0]
            m.advance = adv[0]
            m
        }

        /// Measure the rendered size of `text` without producing a
        /// surface. Faster than rendering when you only need to lay
        /// out a UI.
        ///
        /// Args:
        ///   text — UTF-8 input.
        ///
        /// Returns:
        ///   `(w, h)` in pixels, both `0` on error or empty handle.
        pub size(text: string): Size {
            let s = new Size()
            if this.handle == 0 { return s }
            let ws: i32[] = [0]
            let hs: i32[] = [0]
            let pw: *i32 = ws
            let ph: *i32 = hs
            TTF_SizeUTF8(this.handle as *TTF_Font, cstrFromString(text), pw, ph)
            s.w = ws[0]
            s.h = hs[0]
            s
        }

        /// Render `text` as a fast, jaggy 8-bit-paletted surface
        /// (foreground only, no anti-aliasing).
        ///
        /// Args:
        ///   text — UTF-8 input.
        ///   fg   — foreground colour.
        ///
        /// Returns:
        ///   raw `*SDL_Surface` cast to i64, ready for
        ///   `Texture.fromSurface(...)` or
        ///   `sdl_surface.Surface.fromHandle(...)`. `0` on failure
        ///   (call `getError()`).
        pub renderSolid(text: string, fg: Color): i64 {
            if this.handle == 0 { return 0 }
            TTF_RenderUTF8_Solid(
                this.handle as *TTF_Font, cstrFromString(text), fg
            )
        }

        /// Render `text` as an anti-aliased, opaque-background
        /// surface — fastest of the AA modes.
        ///
        /// Args:
        ///   text — UTF-8 input.
        ///   fg   — foreground colour.
        ///   bg   — solid background colour painted under the text.
        pub renderShaded(text: string, fg: Color, bg: Color): i64 {
            if this.handle == 0 { return 0 }
            TTF_RenderUTF8_Shaded(
                this.handle as *TTF_Font, cstrFromString(text), fg, bg
            )
        }

        /// Render `text` as a 32-bit ARGB surface with alpha-blended
        /// edges. Highest quality, slowest of the three modes.
        pub renderBlended(text: string, fg: Color): i64 {
            if this.handle == 0 { return 0 }
            TTF_RenderUTF8_Blended(
                this.handle as *TTF_Font, cstrFromString(text), fg
            )
        }

        /// Word-wrapping variant of `renderBlended` — breaks lines
        /// so each line fits in `wrapLength` pixels (use `0` to
        /// wrap only on `\n`).
        pub renderBlendedWrapped(text: string, fg: Color, wrapLength: u32): i64 {
            if this.handle == 0 { return 0 }
            TTF_RenderUTF8_Blended_Wrapped(
                this.handle as *TTF_Font, cstrFromString(text), fg, wrapLength
            )
        }
    }
}
