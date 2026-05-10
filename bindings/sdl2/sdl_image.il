// SDL2_image — image format loaders (PNG, JPEG, TIFF, WebP, JXL,
// AVIF, …). Separate library (`libSDL2_image`); not re-exported
// from `sdl.il` so programs that don't need image loading don't
// have to dlopen it. Initialise with `sdl_image.imgInit(flags)`
// after `sdl.init(...)`, tear down with `imgQuit()` before
// `sdl.quit()`.

use sdl_renderer { SDL_Renderer, SDL_Texture }

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

@extern(C) {
    @lib("SDL2_image") pub fn IMG_Init(flags: i32): i32
    @lib("SDL2_image") pub fn IMG_Quit()
    @lib("SDL2_image") pub fn IMG_GetError(): *const char

    /// Load any supported image format into a fresh `SDL_Surface`.
    /// SDL2_image picks the decoder by sniffing magic bytes, then
    /// falls back to extension. Returns `*SDL_Surface` (cast to
    /// `i64` so callers can pass through to `Texture.fromSurface`
    /// / `sdl_surface.freeSurface` without naming the struct).
    @lib("SDL2_image") pub fn IMG_Load(file: *const char): i64

    /// One-step "load image → upload to GPU texture". Returns
    /// `*SDL_Texture` (cast to `i64`). `renderer` is the GPU
    /// renderer the texture binds to. Caller frees with
    /// `Texture.deinit` (or by destroying the wrapping class).
    @lib("SDL2_image") pub fn IMG_LoadTexture(
        renderer: *SDL_Renderer, file: *const char
    ): *SDL_Texture

    /// Encode `surface` as a PNG.
    @lib("SDL2_image") pub fn IMG_SavePNG(
        surface: *const void, file: *const char
    ): i32

    /// Encode `surface` as a JPEG. `quality` is 0..100 (libjpeg
    /// scale; `90` is a good default for general use).
    @lib("SDL2_image") pub fn IMG_SaveJPG(
        surface: *const void, file: *const char, quality: i32
    ): i32

    // ─── friendlier ilang wrappers ───────────────────────────────

    /// Initialise SDL2_image with the requested decoder set.
    ///
    /// Args:
    ///   flags — bitwise OR of `ImgInitFlag.*` values.
    ///
    /// Returns:
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

    /// Returns:
    ///   the latest SDL2_image error message. Most IMG fns route
    ///   through `SDL_SetError`, so `sdl.getError()` works too —
    ///   this exists for the namespaced form.
    pub fn imgGetError(): string {
        stringFromCstr(IMG_GetError())
    }

    /// Load an image file (PNG / JPEG / TIFF / WebP / JXL / AVIF)
    /// into a fresh CPU-side surface.
    ///
    /// Args:
    ///   path — filesystem path.
    ///
    /// Returns:
    ///   raw `*SDL_Surface` cast to i64. `0` on failure (call
    ///   `imgGetError()`). Callers free with
    ///   `sdl_surface.freeSurface(handle)`.
    pub fn imgLoad(path: string): i64 {
        IMG_Load(cstrFromString(path))
    }

    /// Convenience: load an image and upload it as a `Texture` on
    /// `renderer` in one shot — skips the intermediate surface.
    ///
    /// Args:
    ///   rendererHandle — `*SDL_Renderer` cast to i64 (e.g.
    ///                    `r.handle`).
    ///   path           — filesystem path.
    ///
    /// Returns:
    ///   raw `*SDL_Texture` cast to i64. `0` on failure. Callers
    ///   pass into a `Texture` wrapper or free directly with
    ///   `SDL_DestroyTexture` (re-exported from `sdl_renderer.il`).
    pub fn imgLoadTexture(rendererHandle: i64, path: string): i64 {
        if rendererHandle == 0 { return 0 }
        IMG_LoadTexture(
            rendererHandle as *SDL_Renderer, cstrFromString(path)
        ) as i64
    }

    /// Encode an in-memory surface as a PNG file on disk.
    ///
    /// Args:
    ///   surfaceHandle — `*SDL_Surface` cast to i64.
    ///   path          — output filesystem path.
    ///
    /// Returns:
    ///   `0` on success, `< 0` on failure.
    pub fn imgSavePNG(surfaceHandle: i64, path: string): i32 {
        if surfaceHandle == 0 { return -1 }
        IMG_SavePNG(surfaceHandle as *const void, cstrFromString(path))
    }

    /// Encode an in-memory surface as a JPEG file on disk.
    ///
    /// Args:
    ///   surfaceHandle — `*SDL_Surface` cast to i64.
    ///   path          — output filesystem path.
    ///   quality       — 0..100 (libjpeg scale; `90` is a sane
    ///                   default).
    ///
    /// Returns:
    ///   `0` on success, `< 0` on failure.
    pub fn imgSaveJPG(surfaceHandle: i64, path: string, quality: i32): i32 {
        if surfaceHandle == 0 { return -1 }
        IMG_SaveJPG(
            surfaceHandle as *const void,
            cstrFromString(path), quality
        )
    }
}
