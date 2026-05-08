// SDL2 surface + pixel-format API. Surfaces are CPU-side
// pixel buffers — used as scratch images, BMP-format I/O, the
// source for `Texture.fromSurface`, etc. For GPU-accelerated draw
// targets, prefer `Renderer` / `Texture` from `sdl_renderer.il`.
// Mirrors SDL_surface.h + SDL_pixels.h.

use sdl_core { Rect }

/// `SDL_PIXELFORMAT_*` — common pixel formats. The numeric values
/// are SDL's macro encodings (type / order / layout / bits / bytes
/// packed into 32 bits). Use these with `createRGBSurfaceWithFormat`
/// or for `Texture` creation.
enum PixelFormat: u32 {
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

/// `SDL_BlendMode` for `setSurfaceBlendMode` /
/// `setTextureBlendMode`. Same values as in `sdl_renderer.il` but
/// re-declared here so callers don't have to import that module
/// just for the enum.
enum SurfaceBlendMode: i32 {
    none  = 0
    blend = 1   // alpha blending
    add   = 2   // additive
    mod   = 4   // colour-modulated
    mul   = 8   // SDL 2.0.12+
}

@extern(C) {
    /// Opaque SDL_PixelFormat — pointer-typed in C. We never need
    /// to peek inside, so an empty struct suffices.
    struct SDL_PixelFormat {}

    /// Mirror of C `SDL_Surface` (about 88 B; padding to 96 for
    /// safety on every platform). We expose just the fields most
    /// callers reach for; the rest stay in the tail buffer.
    struct SDL_Surface {
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
    @lib("SDL2") fn SDL_CreateRGBSurface(
        flags: u32, w: i32, h: i32, depth: i32,
        rmask: u32, gmask: u32, bmask: u32, amask: u32
    ): *SDL_Surface
    @lib("SDL2") fn SDL_CreateRGBSurfaceWithFormat(
        flags: u32, w: i32, h: i32, depth: i32, format: u32
    ): *SDL_Surface
    @lib("SDL2") fn SDL_CreateRGBSurfaceFrom(
        pixels: *const void, w: i32, h: i32, depth: i32, pitch: i32,
        rmask: u32, gmask: u32, bmask: u32, amask: u32
    ): *SDL_Surface
    @lib("SDL2") fn SDL_CreateRGBSurfaceWithFormatFrom(
        pixels: *const void, w: i32, h: i32, depth: i32, pitch: i32,
        format: u32
    ): *SDL_Surface
    @lib("SDL2") fn SDL_FreeSurface(surface: *SDL_Surface)

    // ─── BMP I/O ─────────────────────────────────────────────────
    @lib("SDL2") fn SDL_LoadBMP_RW(
        src: *const void, freesrc: i32
    ): *SDL_Surface
    @lib("SDL2") fn SDL_SaveBMP_RW(
        surface: *SDL_Surface, dst: *const void, freedst: i32
    ): i32

    // ─── locking (for direct pixel access) ───────────────────────
    @lib("SDL2") fn SDL_LockSurface(surface: *SDL_Surface): i32
    @lib("SDL2") fn SDL_UnlockSurface(surface: *SDL_Surface)
    // SDL_MUSTLOCK is a C macro, not a real symbol —
    // implemented as the ilang `mustLock` helper below.

    // ─── blits + fills ───────────────────────────────────────────
    @lib("SDL2") fn SDL_UpperBlit(
        src: *SDL_Surface, srcrect: *const Rect,
        dst: *SDL_Surface, dstrect: *Rect
    ): i32
    @lib("SDL2") fn SDL_UpperBlitScaled(
        src: *SDL_Surface, srcrect: *const Rect,
        dst: *SDL_Surface, dstrect: *Rect
    ): i32
    @lib("SDL2") fn SDL_FillRect(
        dst: *SDL_Surface, rect: *const Rect, color: u32
    ): i32
    @lib("SDL2") fn SDL_FillRects(
        dst: *SDL_Surface, rects: *const Rect, count: i32, color: u32
    ): i32

    // ─── modulation / blending ───────────────────────────────────
    @lib("SDL2") fn SDL_SetSurfaceBlendMode(
        surface: *SDL_Surface, blend_mode: i32
    ): i32
    @lib("SDL2") fn SDL_GetSurfaceBlendMode(
        surface: *SDL_Surface, blend_mode: *i32
    ): i32
    @lib("SDL2") fn SDL_SetSurfaceColorMod(
        surface: *SDL_Surface, r: u8, g: u8, b: u8
    ): i32
    @lib("SDL2") fn SDL_SetSurfaceAlphaMod(
        surface: *SDL_Surface, alpha: u8
    ): i32
    @lib("SDL2") fn SDL_SetColorKey(
        surface: *SDL_Surface, flag: i32, key: u32
    ): i32
    @lib("SDL2") fn SDL_GetColorKey(
        surface: *SDL_Surface, key: *u32
    ): i32

    // ─── clipping ────────────────────────────────────────────────
    @lib("SDL2") fn SDL_SetClipRect(
        surface: *SDL_Surface, rect: *const Rect
    ): i32
    @lib("SDL2") fn SDL_GetClipRect(
        surface: *SDL_Surface, rect: *Rect
    )

    // ─── pixel-format helpers ────────────────────────────────────
    @lib("SDL2") fn SDL_AllocFormat(pixel_format: u32): *SDL_PixelFormat
    @lib("SDL2") fn SDL_FreeFormat(format: *SDL_PixelFormat)
    @lib("SDL2") fn SDL_GetPixelFormatName(format: u32): *const char
    @lib("SDL2") fn SDL_MapRGB(
        format: *SDL_PixelFormat, r: u8, g: u8, b: u8
    ): u32
    @lib("SDL2") fn SDL_MapRGBA(
        format: *SDL_PixelFormat, r: u8, g: u8, b: u8, a: u8
    ): u32
    @lib("SDL2") fn SDL_GetRGB(
        pixel: u32, format: *SDL_PixelFormat,
        r: *u8, g: *u8, b: *u8
    )
    @lib("SDL2") fn SDL_GetRGBA(
        pixel: u32, format: *SDL_PixelFormat,
        r: *u8, g: *u8, b: *u8, a: *u8
    )

    // ─── format conversion ───────────────────────────────────────
    @lib("SDL2") fn SDL_ConvertSurface(
        src: *SDL_Surface, fmt: *SDL_PixelFormat, flags: u32
    ): *SDL_Surface
    @lib("SDL2") fn SDL_ConvertSurfaceFormat(
        src: *SDL_Surface, pixel_format: u32, flags: u32
    ): *SDL_Surface

    // ─── friendlier ilang wrappers ───────────────────────────────

    /// Allocate a blank CPU-side surface in the given pixel format.
    ///
    /// Args:
    ///   w, h   — surface dimensions in pixels.
    ///   format — `PixelFormat.argb8888` etc.
    ///
    /// Returns:
    ///   raw `*SDL_Surface` cast to i64. `0` on allocation
    ///   failure.
    fn createSurface(w: i32, h: i32, format: PixelFormat): i64 {
        let depth: i32 = if format == PixelFormat.rgb24
                         || format == PixelFormat.bgr24 {
            24
        } elif format == PixelFormat.rgb565
                  || format == PixelFormat.bgr565
                  || format == PixelFormat.rgb555
                  || format == PixelFormat.bgr555 {
            16
        } else {
            // 32-bit ARGB/RGBA/etc. is the common default.
            32
        }
        SDL_CreateRGBSurfaceWithFormat(0 as u32, w, h, depth, format as u32) as i64
    }

    /// Free a surface allocated by `createSurface` /
    /// `loadBmp` / one of the conversion fns.
    fn freeSurface(handle: i64) {
        if handle != 0 {
            SDL_FreeSurface(handle as *SDL_Surface)
        }
    }

    /// Load a BMP file into a fresh surface.
    ///
    /// Args:
    ///   path — filesystem path (the wrapper opens / closes the
    ///          underlying RWops for the caller).
    ///
    /// Returns:
    ///   raw `*SDL_Surface` cast to i64. `0` on failure.
    fn loadBmp(path: string): i64 {
        let mode = cstrFromString("rb")
        // We need an RWops. Use SDL_RWFromFile via the rwops module
        // would introduce a dep cycle (rwops would in turn want
        // surface-friendly errors); inline the call here instead.
        let rw = SDL_RWFromFileLocal(cstrFromString(path), mode)
        if rw == 0 { return 0 }
        // freesrc=1 → SDL closes the RWops for us.
        SDL_LoadBMP_RW(rw as *const void, 1) as i64
    }

    /// Write a surface out to a BMP file.
    ///
    /// Args:
    ///   handle — `*SDL_Surface` cast to i64 (e.g. from
    ///            `createSurface`).
    ///   path   — filesystem path.
    ///
    /// Returns:
    ///   `0` on success, `< 0` on failure.
    fn saveBmp(handle: i64, path: string): i32 {
        if handle == 0 { return -1 }
        let mode = cstrFromString("wb")
        let rw = SDL_RWFromFileLocal(cstrFromString(path), mode)
        if rw == 0 { return -1 }
        SDL_SaveBMP_RW(handle as *SDL_Surface, rw as *const void, 1)
    }

    // Inline RWFromFile shim — we re-declare it here under a unique
    // ilang name so this module stays usable without `use sdl_rwops`.
    @lib("SDL2") @symbol("SDL_RWFromFile") fn SDL_RWFromFileLocal(
        file: *const char, mode: *const char
    ): i64

    /// Fill a (sub-)rect with `colour`. Pass `null` (0) as `rect`
    /// to fill the whole surface.
    ///
    /// Args:
    ///   handle — `*SDL_Surface` cast to i64.
    ///   rect   — area to fill, or null for "everything".
    ///   colour — pre-mapped pixel value (`mapRGB(format, r, g, b)`).
    ///
    /// Returns:
    ///   `0` on success, `< 0` on failure.
    fn fillSurfaceRect(handle: i64, rect: Rect, colour: u32): i32 {
        if handle == 0 { return -1 }
        SDL_FillRect(handle as *SDL_Surface, rect, colour)
    }

    /// Variant of `fillSurfaceRect` that fills the entire surface
    /// (rect = NULL).
    fn fillSurface(handle: i64, colour: u32): i32 {
        if handle == 0 { return -1 }
        SDL_FillRect(handle as *SDL_Surface, 0 as *const Rect, colour)
    }

    /// Blit `src[srcRect]` onto `dst[dstRect]`. Use the float-rect
    /// variant by passing `null` for `srcRect` to mean "whole src".
    ///
    /// Returns:
    ///   `0` on success, `< 0` on failure.
    fn blit(srcHandle: i64, srcRect: Rect, dstHandle: i64, dstRect: Rect): i32 {
        if srcHandle == 0 || dstHandle == 0 { return -1 }
        SDL_UpperBlit(
            srcHandle as *SDL_Surface, srcRect,
            dstHandle as *SDL_Surface, dstRect
        )
    }

    /// Scaling blit — stretches `src` into `dstRect`.
    fn blitScaled(srcHandle: i64, srcRect: Rect, dstHandle: i64, dstRect: Rect): i32 {
        if srcHandle == 0 || dstHandle == 0 { return -1 }
        SDL_UpperBlitScaled(
            srcHandle as *SDL_Surface, srcRect,
            dstHandle as *SDL_Surface, dstRect
        )
    }

    /// Set the alpha-blend mode for subsequent blits originating
    /// from `handle`.
    fn setSurfaceBlendMode(handle: i64, mode: SurfaceBlendMode): i32 {
        if handle == 0 { return -1 }
        SDL_SetSurfaceBlendMode(handle as *SDL_Surface, mode as i32)
    }

    /// Per-blit RGB tint multiplied against the surface pixels.
    fn setSurfaceColorMod(handle: i64, r: u8, g: u8, b: u8): i32 {
        if handle == 0 { return -1 }
        SDL_SetSurfaceColorMod(handle as *SDL_Surface, r, g, b)
    }

    /// Per-blit alpha multiplier (0 transparent, 255 opaque).
    fn setSurfaceAlphaMod(handle: i64, alpha: u8): i32 {
        if handle == 0 { return -1 }
        SDL_SetSurfaceAlphaMod(handle as *SDL_Surface, alpha)
    }

    /// Mark `key` (a pre-mapped pixel value) as transparent for
    /// blits from `handle`.
    ///
    /// Args:
    ///   handle  — `*SDL_Surface` cast to i64.
    ///   enabled — true turns the colour key on, false off.
    ///   key     — pixel value to treat as transparent.
    fn setColorKey(handle: i64, enabled: bool, key: u32): i32 {
        if handle == 0 { return -1 }
        let flag: i32 = if enabled { 1 } else { 0 }
        SDL_SetColorKey(handle as *SDL_Surface, flag, key)
    }

    /// Pack an `(r, g, b)` triple as a single pixel value in the
    /// given format.
    fn mapRGB(format: PixelFormat, r: u8, g: u8, b: u8): u32 {
        let f = SDL_AllocFormat(format as u32)
        if f as i64 == 0 { return 0 as u32 }
        let v = SDL_MapRGB(f, r, g, b)
        SDL_FreeFormat(f)
        v
    }

    /// Pack an `(r, g, b, a)` quadruple as a single pixel value.
    fn mapRGBA(format: PixelFormat, r: u8, g: u8, b: u8, a: u8): u32 {
        let f = SDL_AllocFormat(format as u32)
        if f as i64 == 0 { return 0 as u32 }
        let v = SDL_MapRGBA(f, r, g, b, a)
        SDL_FreeFormat(f)
        v
    }

    /// Returns:
    ///   the SDL canonical name for `format` (e.g.
    ///   `"SDL_PIXELFORMAT_RGBA8888"`).
    fn pixelFormatName(format: PixelFormat): string {
        stringFromCstr(SDL_GetPixelFormatName(format as u32))
    }

    /// `SDL_MUSTLOCK` macro reimplemented in ilang. Surfaces only
    /// need an explicit lock around direct `pixels` access when
    /// SDL has flagged them with `SDL_RLEACCEL` (= 2). We read the
    /// `flags` u32 at offset 0 directly via `readU32` — field
    /// access through a raw `*SDL_Surface` would need pointer-deref
    /// syntax ilang doesn't have yet.
    ///
    /// Args:
    ///   handle — `*SDL_Surface` cast to i64.
    ///
    /// Returns:
    ///   true if the surface needs a `SDL_LockSurface` call before
    ///   touching its `pixels` buffer.
    fn mustLock(handle: i64): bool {
        if handle == 0 { return false }
        (readU32(handle as *const void, 0) & (2 as u32)) != 0 as u32
    }
}
