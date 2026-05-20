/// SDL2 surface + pixel-format API. Surfaces are CPU-side
/// pixel buffers — used as scratch images, BMP-format I/O, the
/// source for `Texture.fromSurface`, etc. For GPU-accelerated draw
/// targets, prefer `Renderer` / `Texture` from `sdl_renderer.il`.
/// Mirrors SDL_surface.h + SDL_pixels.h.

use sdl_core { Rect, Color }

/// `SDL_PIXELFORMAT_*` — common pixel formats. The numeric values
/// are SDL's macro encodings (type / order / layout / bits / bytes
/// packed into 32 bits). Use these with `createRGBSurfaceWithFormat`
/// or for `Texture` creation.
pub enum PixelFormat: u32 {
    unknown        = 0
    index1lsb      = 0x11100100
    index1msb      = 0x11200100
    index4lsb      = 0x12100400
    index4msb      = 0x12200400
    index8         = 0x13000801
    rgb332         = 0x14110801
    rgb444         = 0x15120c02   // XRGB4444 in SDL 2.0.5+
    rgb555         = 0x15130f02
    bgr555         = 0x15530f02
    argb4444       = 0x15321002
    rgba4444       = 0x15421002
    abgr4444       = 0x15721002
    bgra4444       = 0x15821002
    argb1555       = 0x15331002
    rgba5551       = 0x15441002
    abgr1555       = 0x15731002
    bgra5551       = 0x15841002
    rgb565         = 0x15151002
    bgr565         = 0x15551002
    rgb24          = 0x17101803
    bgr24          = 0x17401803
    rgb888         = 0x16161804
    rgbx8888       = 0x16261804
    bgr888         = 0x16561804
    bgrx8888       = 0x16661804
    argb8888       = 0x16362004
    rgba8888       = 0x16462004
    abgr8888       = 0x16762004
    bgra8888       = 0x16862004
    yv12           = 0x32315659
    iyuv           = 0x56555949
    yuy2           = 0x32595559
    uyvy           = 0x59565955
    yvyu           = 0x55595659
}

/// `SDL_BlendMode` — the four canned blend modes plus user-defined
/// modes assembled via `Renderer.composeCustomBlendMode(...)`.
/// Lives here (in `sdl_surface.il`) so both surface- and
/// renderer/texture-side accessors can share it without circular
/// imports — the umbrella loads `sdl_surface` before
/// `sdl_renderer`.
pub enum BlendMode: i32 {
    none    = 0
    blend   = 1            // alpha blending
    add     = 2            // additive
    mod     = 4            // colour-modulated
    mul     = 8            // SDL 2.0.12+
    invalid = 0x7FFFFFFF   // sentinel returned by SDL on error
}

@extern(C, "SDL2") {
    /// Opaque SDL_PixelFormat — pointer-typed in C. We never need
    /// to peek inside, so an empty struct suffices.
    pub struct SDL_PixelFormat {}

    /// Mirror of C `SDL_Surface` (about 88 B; padding to 96 for
    /// safety on every platform). We expose just the fields most
    /// callers reach for; the rest stay in the tail buffer.
    pub struct SDL_Surface {
        flags: u32
        format: i64           // SDL_PixelFormat *
        w: i32
        h: i32
        pitch: i32
        _pad0: i32
        pixels: i64           // void *
        userdata: i64
        locked: i32
        list_blitmap: i32
        clip_rect: Rect
        _pad_tail: u8[40]
    }

    // ─── lifecycle ───────────────────────────────────────────────
    @lib pub fn SDL_CreateRGBSurface(
        flags: u32, w: i32, h: i32, depth: i32,
        rmask: u32, gmask: u32, bmask: u32, amask: u32
    ): *SDL_Surface
    @lib pub fn SDL_CreateRGBSurfaceWithFormat(
        flags: u32, w: i32, h: i32, depth: i32, format: u32
    ): *SDL_Surface
    @lib pub fn SDL_CreateRGBSurfaceFrom(
        pixels: *const void, w: i32, h: i32, depth: i32, pitch: i32,
        rmask: u32, gmask: u32, bmask: u32, amask: u32
    ): *SDL_Surface
    @lib pub fn SDL_CreateRGBSurfaceWithFormatFrom(
        pixels: *const void, w: i32, h: i32, depth: i32, pitch: i32,
        format: u32
    ): *SDL_Surface
    @lib pub fn SDL_FreeSurface(surface: *SDL_Surface)

    // ─── BMP I/O ─────────────────────────────────────────────────
    @lib pub fn SDL_LoadBMP_RW(
        src: *const void, freesrc: i32
    ): *SDL_Surface
    @lib pub fn SDL_SaveBMP_RW(
        surface: *SDL_Surface, dst: *const void, freedst: i32
    ): i32

    // ─── locking (for direct pixel access) ───────────────────────
    @lib pub fn SDL_LockSurface(surface: *SDL_Surface): i32
    @lib pub fn SDL_UnlockSurface(surface: *SDL_Surface)
    // SDL_MUSTLOCK is a C macro, not a real symbol —
    // implemented as the ilang `mustLock` helper below.

    // ─── blits + fills ───────────────────────────────────────────
    @lib pub fn SDL_UpperBlit(
        src: *SDL_Surface, srcrect: *const Rect,
        dst: *SDL_Surface, dstrect: *Rect
    ): i32
    @lib pub fn SDL_UpperBlitScaled(
        src: *SDL_Surface, srcrect: *const Rect,
        dst: *SDL_Surface, dstrect: *Rect
    ): i32
    @lib pub fn SDL_FillRect(
        dst: *SDL_Surface, rect: *const Rect, color: u32
    ): i32
    @lib pub fn SDL_FillRects(
        dst: *SDL_Surface, rects: *const Rect, count: i32, color: u32
    ): i32

    // ─── modulation / blending ───────────────────────────────────
    @lib pub fn SDL_SetSurfaceBlendMode(
        surface: *SDL_Surface, blend_mode: i32
    ): i32
    @lib pub fn SDL_GetSurfaceBlendMode(
        surface: *SDL_Surface, blend_mode: *i32
    ): i32
    @lib pub fn SDL_SetSurfaceColorMod(
        surface: *SDL_Surface, r: u8, g: u8, b: u8
    ): i32
    @lib pub fn SDL_SetSurfaceAlphaMod(
        surface: *SDL_Surface, alpha: u8
    ): i32
    @lib pub fn SDL_SetColorKey(
        surface: *SDL_Surface, flag: i32, key: u32
    ): i32
    @lib pub fn SDL_GetColorKey(
        surface: *SDL_Surface, key: *u32
    ): i32

    // ─── clipping ────────────────────────────────────────────────
    @lib pub fn SDL_SetClipRect(
        surface: *SDL_Surface, rect: *const Rect
    ): i32
    @lib pub fn SDL_GetClipRect(
        surface: *SDL_Surface, rect: *Rect
    )

    // ─── pixel-format helpers ────────────────────────────────────
    @lib pub fn SDL_AllocFormat(pixel_format: u32): *SDL_PixelFormat
    @lib pub fn SDL_FreeFormat(format: *SDL_PixelFormat)
    @lib pub fn SDL_GetPixelFormatName(format: u32): *const char
    @lib pub fn SDL_MapRGB(
        format: *SDL_PixelFormat, r: u8, g: u8, b: u8
    ): u32
    @lib pub fn SDL_MapRGBA(
        format: *SDL_PixelFormat, r: u8, g: u8, b: u8, a: u8
    ): u32
    @lib pub fn SDL_GetRGB(
        pixel: u32, format: *SDL_PixelFormat,
        r: *u8, g: *u8, b: *u8
    )
    @lib pub fn SDL_GetRGBA(
        pixel: u32, format: *SDL_PixelFormat,
        r: *u8, g: *u8, b: *u8, a: *u8
    )

    // ─── palette (8-bit indexed images) ──────────────────────────
    /// Opaque `SDL_Palette *`. Construct via `allocPalette`,
    /// release with `freePalette`.
    pub struct SDL_Palette {}

    @lib pub fn SDL_AllocPalette(ncolors: i32): *SDL_Palette
    @lib pub fn SDL_FreePalette(palette: *SDL_Palette)
    @lib pub fn SDL_SetPaletteColors(
        palette: *SDL_Palette, colors: *const Color,
        firstcolor: i32, ncolors: i32
    ): i32
    @lib pub fn SDL_SetSurfacePalette(
        surface: *SDL_Surface, palette: *SDL_Palette
    ): i32
    @lib pub fn SDL_SetPixelFormatPalette(
        format: *SDL_PixelFormat, palette: *SDL_Palette
    ): i32

    // ─── format conversion ───────────────────────────────────────
    @lib pub fn SDL_ConvertSurface(
        src: *SDL_Surface, fmt: *SDL_PixelFormat, flags: u32
    ): *SDL_Surface
    @lib pub fn SDL_ConvertSurfaceFormat(
        src: *SDL_Surface, pixel_format: u32, flags: u32
    ): *SDL_Surface

    // ─── ARC-managed wrappers ────────────────────────────────────

    /// CPU-side pixel buffer. Wraps an `*SDL_Surface` with ARC so
    /// the underlying allocation is freed automatically when the
    /// last reference goes out of scope. Construct with the
    /// (`w, h, format`) init for a blank surface, or
    /// `Surface.fromHandle(...)` to take ownership of a raw handle
    /// returned by another binding (`sdl_image.loadImage`, the
    /// `sdl_ttf` rendering fns, etc.).
    pub class Surface {
        /// `*SDL_Surface` cast to i64. `0` if construction failed
        /// (out-of-memory, BMP load error, …) — every method
        /// no-ops gracefully against `0`.
        pub handle: i64

        /// Allocate a blank CPU-side surface in the given pixel
        /// format. The depth (24 / 16 / 32) is picked from the
        /// format; uncommon formats default to 32-bit.
        ///
        /// **Args:**
        /// - `w`, `h` — dimensions in pixels.
        /// - `format` — `PixelFormat.argb8888` etc.
        pub init(w: i32, h: i32, format: PixelFormat) {
            let depth: i32 = if format == PixelFormat.rgb24
                             || format == PixelFormat.bgr24 {
                24
            } elif format == PixelFormat.rgb565
                      || format == PixelFormat.bgr565
                      || format == PixelFormat.rgb555
                      || format == PixelFormat.bgr555 {
                16
            } else {
                32
            }
            this.handle = SDL_CreateRGBSurfaceWithFormat(
                0 as u32, w, h, depth, format as u32
            ) as i64
        }

        deinit() {
            if this.handle != 0 {
                SDL_FreeSurface(this.handle as *SDL_Surface)
            }
        }

        /// Wrap a raw `*SDL_Surface` (cast to i64) returned by
        /// another module (e.g. `sdl_image.loadImage`,
        /// `sdl_ttf` rendering). Takes ownership: `deinit` will
        /// call `SDL_FreeSurface` on it.
        ///
        /// Implementation: allocate a 1×1 placeholder via the
        /// public init, free that immediately, then overwrite the
        /// `handle` slot with the caller's pointer. The handle
        /// field itself is i64 (no automatic ARC release on
        /// reassign), so we free the placeholder explicitly to
        /// avoid leaking it.
        pub static fromHandle(handle: i64): Surface {
            let s = new Surface(1, 1, PixelFormat.argb8888)
            if s.handle != 0 {
                SDL_FreeSurface(s.handle as *SDL_Surface)
            }
            s.handle = handle
            s
        }

        /// Load a BMP file into a fresh surface.
        ///
        /// **Args:**
        /// - `path` — filesystem path.
        ///
        /// **Returns:**
        ///   a `Surface` whose `handle` is `0` on load failure.
        pub static loadBmp(path: string): Surface {
            let rw = SDL_RWFromFileLocal(
                cstrFromString(path), cstrFromString("rb")
            )
            if rw == 0 { return Surface.fromHandle(0) }
            // freesrc=1 → SDL closes the RWops for us.
            Surface.fromHandle(SDL_LoadBMP_RW(rw as *const void, 1) as i64)
        }

        /// Save this surface to disk as a BMP file.
        ///
        /// **Returns:**
        ///   `0` on success, `< 0` on failure.
        pub saveBmp(path: string): i32 {
            if this.handle == 0 { return -1 }
            let rw = SDL_RWFromFileLocal(
                cstrFromString(path), cstrFromString("wb")
            )
            if rw == 0 { return -1 }
            SDL_SaveBMP_RW(this.handle as *SDL_Surface, rw as *const void, 1)
        }

        /// Fill a sub-rect with `colour`.
        ///
        /// **Args:**
        /// - `rect` — area to fill.
        /// - `colour` — pre-mapped pixel value (`mapRGB` /
        ///   `mapRGBA`).
        ///
        /// **Returns:**
        ///   `0` on success, `< 0` on failure.
        pub fillRect(rect: Rect, colour: u32): i32 {
            if this.handle == 0 { return -1 }
            SDL_FillRect(this.handle as *SDL_Surface, &rect, colour)
        }

        /// Fill the entire surface with `colour`.
        pub fill(colour: u32): i32 {
            if this.handle == 0 { return -1 }
            SDL_FillRect(this.handle as *SDL_Surface, 0 as *const Rect, colour)
        }

        /// Blit a sub-rect of `this` onto `dst`.
        ///
        /// **Returns:**
        ///   `0` on success, `< 0` on failure.
        pub blit(srcRect: Rect, dst: Surface, dstRect: Rect): i32 {
            if this.handle == 0 || dst.handle == 0 { return -1 }
            SDL_UpperBlit(
                this.handle as *SDL_Surface, &srcRect,
                dst.handle as *SDL_Surface, &dstRect
            )
        }

        /// Scaling blit — stretches `this[srcRect]` into
        /// `dst[dstRect]`.
        pub blitScaled(srcRect: Rect, dst: Surface, dstRect: Rect): i32 {
            if this.handle == 0 || dst.handle == 0 { return -1 }
            SDL_UpperBlitScaled(
                this.handle as *SDL_Surface, &srcRect,
                dst.handle as *SDL_Surface, &dstRect
            )
        }

        /// Set the alpha-blend mode for subsequent blits originating
        /// from this surface.
        pub setBlendMode(mode: BlendMode): i32 {
            if this.handle == 0 { return -1 }
            SDL_SetSurfaceBlendMode(this.handle as *SDL_Surface, mode as i32)
        }

        /// Per-blit RGB tint multiplied against the surface pixels.
        pub setColorMod(r: u8, g: u8, b: u8): i32 {
            if this.handle == 0 { return -1 }
            SDL_SetSurfaceColorMod(this.handle as *SDL_Surface, r, g, b)
        }

        /// Per-blit alpha multiplier (0 transparent, 255 opaque).
        pub setAlphaMod(alpha: u8): i32 {
            if this.handle == 0 { return -1 }
            SDL_SetSurfaceAlphaMod(this.handle as *SDL_Surface, alpha)
        }

        /// Mark `key` (a pre-mapped pixel value) as transparent.
        pub setColorKey(enabled: bool, key: u32): i32 {
            if this.handle == 0 { return -1 }
            let flag: i32 = if enabled { 1 } else { 0 }
            SDL_SetColorKey(this.handle as *SDL_Surface, flag, key)
        }

        /// Bind a palette to this surface (mostly relevant for
        /// 8-bit indexed images: `PixelFormat.index8`). Pass
        /// `none` to clear.
        pub setPalette(palette: Palette?): i32 {
            if this.handle == 0 { return -1 }
            let h: i64 = if let some(p) = palette { p.handle } else { 0 }
            SDL_SetSurfacePalette(
                this.handle as *SDL_Surface, h as *SDL_Palette
            )
        }

        /// `SDL_MUSTLOCK` macro reimplemented in ilang. Surfaces
        /// only need an explicit lock around direct `pixels` access
        /// when SDL has flagged them with `SDL_RLEACCEL` (= 2). We
        /// read the `flags` u32 at offset 0 directly via `readU32`
        /// — field access through a raw `*SDL_Surface` would need
        /// pointer-deref syntax ilang doesn't have yet.
        pub mustLock(): bool {
            if this.handle == 0 { return false }
            (readU32(this.handle as *const void, 0) & (2 as u32)) != 0 as u32
        }

        /// Lock the surface for direct pixel access.
        ///
        /// **Returns:**
        ///   `0` on success, `< 0` on failure.
        pub lock(): i32 {
            if this.handle == 0 { return -1 }
            SDL_LockSurface(this.handle as *SDL_Surface)
        }

        pub unlock() {
            if this.handle != 0 {
                SDL_UnlockSurface(this.handle as *SDL_Surface)
            }
        }

        /// Convert this surface to `format`. Returns a fresh
        /// Surface; the original stays valid.
        pub convertTo(format: PixelFormat): Surface {
            if this.handle == 0 {
                return Surface.fromHandle(0)
            }
            let h = SDL_ConvertSurfaceFormat(
                this.handle as *SDL_Surface, format as u32, 0 as u32
            ) as i64
            Surface.fromHandle(h)
        }

        // ─── format-level helpers (static) ───────────────────────
        //
        // Don't operate on a Surface instance; expressed as
        // `Surface.*` statics to keep the format-related helpers
        // namespaced rather than loose at the module level.

        /// Pack an `(r, g, b)` triple as a single pixel value in
        /// `format`. `0` on allocation failure.
        pub static mapRGB(
            format: PixelFormat, r: u8, g: u8, b: u8
        ): u32 {
            let f = SDL_AllocFormat(format as u32)
            if f as i64 == 0 { return 0 as u32 }
            let v = SDL_MapRGB(f, r, g, b)
            SDL_FreeFormat(f)
            v
        }

        /// Pack an `(r, g, b, a)` quadruple as a single pixel
        /// value in `format`.
        pub static mapRGBA(
            format: PixelFormat, r: u8, g: u8, b: u8, a: u8
        ): u32 {
            let f = SDL_AllocFormat(format as u32)
            if f as i64 == 0 { return 0 as u32 }
            let v = SDL_MapRGBA(f, r, g, b, a)
            SDL_FreeFormat(f)
            v
        }

        /// **Returns:**
        ///   the SDL canonical name for `format` (e.g.
        ///   `"SDL_PIXELFORMAT_RGBA8888"`).
        pub static pixelFormatName(format: PixelFormat): string {
            stringFromCstr(SDL_GetPixelFormatName(format as u32))
        }
    }

    /// 8-bit indexed colour palette. Wraps `*SDL_Palette` with ARC.
    pub class Palette {
        pub handle: i64

        /// Allocate a palette with `ncolors` slots, all initialised
        /// to white. Use `setColors` to fill it in.
        pub init(ncolors: i32) {
            this.handle = SDL_AllocPalette(ncolors) as i64
        }

        deinit() {
            if this.handle != 0 {
                SDL_FreePalette(this.handle as *SDL_Palette)
            }
        }

        /// Write `colors` into the palette starting at slot
        /// `firstColor`. `firstColor + colors.length` must stay
        /// within the palette's allocated size.
        ///
        /// **Returns:**
        ///   `0` on success, `< 0` on failure.
        pub setColors(colors: Color[], firstColor: i32): i32 {
            if this.handle == 0 { return -1 }
            let p: *Color = colors
            SDL_SetPaletteColors(
                this.handle as *SDL_Palette, p as *const Color,
                firstColor, colors.length as i32
            )
        }
    }

    // Inline RWFromFile shim — `Surface.loadBmp` / `saveBmp` use
    // `SDL_LoadBMP_RW` / `SDL_SaveBMP_RW`, which need an RWops to
    // wire the path through. Re-declared here under a unique name
    // so this module stays usable without `use sdl_rwops`.
    @lib @symbol("SDL_RWFromFile") pub fn SDL_RWFromFileLocal(
        file: *const char, mode: *const char
    ): i64
}
