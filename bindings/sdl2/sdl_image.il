/// SDL2_image — image format loaders (PNG, JPEG, TIFF, WebP, JXL,
/// AVIF, …). Separate library (`libSDL2_image`); not re-exported
/// from `sdl.il` so programs that don't need image loading don't
/// have to dlopen it. Initialise with `sdl_image.imgInit(flags)`
/// after `sdl.init(...)`, tear down with `imgQuit()` before
/// `sdl.quit()`.

use sdl_renderer { Renderer, Texture, SDL_Renderer, SDL_Texture }
use sdl_surface { Surface }
use std.ffi { cstrFromString, stringFromCstr }

/// `IMG_INIT_*` — bitwise OR of the formats whose decoder support
/// to initialise. JPG / PNG / TIFF are the common trio; jxl / avif
/// are SDL2_image 2.6+ and depend on the optional libjxl / libavif
/// being present at build time.
@flags
pub enum ImgInitFlag: i32 {
    jpg  = 0x01
    png  = 0x02
    tif  = 0x04
    webp = 0x08
    jxl  = 0x10
    avif = 0x20
}

@extern(C, "SDL2_image") {
    @lib pub fn IMG_Init(flags: i32): i32
    @lib pub fn IMG_Quit()
    @lib pub fn IMG_GetError(): *const char

    /// Load any supported image format into a fresh `SDL_Surface`.
    /// SDL2_image picks the decoder by sniffing magic bytes, then
    /// falls back to extension. Returns `*SDL_Surface` (cast to
    /// `i64`); pass through to `Texture.fromSurface` or wrap with
    /// `sdl_surface.Surface.fromHandle(...)` for ARC management.
    @lib pub fn IMG_Load(file: *const char): i64

    /// One-step "load image → upload to GPU texture". Returns
    /// `*SDL_Texture` (cast to `i64`). `renderer` is the GPU
    /// renderer the texture binds to. Caller frees with
    /// `Texture.deinit` (or by destroying the wrapping class).
    @lib pub fn IMG_LoadTexture(
        renderer: *SDL_Renderer, file: *const char
    ): *SDL_Texture

    /// Encode `surface` as a PNG.
    @lib pub fn IMG_SavePNG(
        surface: *const void, file: *const char
    ): i32

    /// Encode `surface` as a JPEG. `quality` is 0..100 (libjpeg
    /// scale; `90` is a good default for general use).
    @lib pub fn IMG_SaveJPG(
        surface: *const void, file: *const char, quality: i32
    ): i32

    // ─── friendlier ilang wrappers ───────────────────────────────

    /// Initialise SDL2_image with the requested decoder set.
    ///
    /// **Args:**
    /// - `flags` — bitwise OR of `ImgInitFlag.*` values.
    ///
    /// **Returns:**
    ///   bitmask of decoders that actually initialised — should
    ///   match `flags as i32` on success. Mismatched bits indicate
    ///   missing optional dependencies (`call imgGetError()` for
    ///   the message).
    pub fn imgInit(flags: ImgInitFlag): i32 {
        IMG_Init(flags as i32)
    }

    /// Tear down SDL2_image. Call before `sdl.quit()`.
    pub fn imgQuit() {
        IMG_Quit()
    }

    /// **Returns:**
    ///   the latest SDL2_image error message. Most IMG fns route
    ///   through `SDL_SetError`, so `sdl.getError()` works too —
    ///   this exists for the namespaced form.
    pub fn imgGetError(): string {
        stringFromCstr(IMG_GetError())
    }

    /// Load an image file (PNG / JPEG / TIFF / WebP / JXL / AVIF)
    /// into a fresh CPU-side `Surface`.
    ///
    /// **Args:**
    /// - `path` — filesystem path.
    ///
    /// **Returns:**
    ///   a `Surface` whose `handle` is `0` on failure (call
    ///   `imgGetError()` for the message).
    pub fn imgLoad(path: string): Surface {
        Surface.fromHandle(IMG_Load(cstrFromString(path)))
    }

    /// Convenience: load an image and upload it as a `Texture`
    /// on `renderer` in one shot — skips the intermediate
    /// surface.
    ///
    /// **Args:**
    /// - `renderer` — the parent renderer.
    /// - `path` — filesystem path.
    ///
    /// **Returns:**
    ///   a `Texture` whose `handle` is `0` on failure.
    pub fn imgLoadTexture(renderer: Renderer, path: string): Texture {
        if renderer.handle == 0 {
            return Texture.fromHandle(0)
        }
        let h = IMG_LoadTexture(
            renderer.handle as *SDL_Renderer, cstrFromString(path)
        ) as i64
        Texture.fromHandle(h)
    }

    /// Encode `surface` as a PNG file on disk.
    ///
    /// **Args:**
    /// - `surface` — source surface.
    /// - `path` — output filesystem path.
    ///
    /// **Returns:**
    ///   `0` on success, `< 0` on failure.
    pub fn imgSavePNG(surface: Surface, path: string): i32 {
        if surface.handle == 0 { return -1 }
        IMG_SavePNG(surface.handle as *const void, cstrFromString(path))
    }

    /// Encode `surface` as a JPEG file on disk.
    ///
    /// **Args:**
    /// - `surface` — source surface.
    /// - `path` — output filesystem path.
    /// - `quality` — 0..100 (libjpeg scale; `90` is a sane default).
    ///
    /// **Returns:**
    ///   `0` on success, `< 0` on failure.
    pub fn imgSaveJPG(surface: Surface, path: string, quality: i32): i32 {
        if surface.handle == 0 { return -1 }
        IMG_SaveJPG(
            surface.handle as *const void,
            cstrFromString(path), quality
        )
    }
}
