// SDL2 shaped (non-rectangular) windows. Mirrors SDL_shape.h.
// Use case: HUD overlays, splash screens with transparent regions
// that cut through the window decoration. The shape itself is a
// `*SDL_Surface` whose alpha channel (or a colour key) defines
// which pixels are window vs. cut-out.
//
// Lifecycle:
//   sdl.init(InitFlag.video)
//   let win = sdl.createShapedWindow("Title", 100, 100, 640, 480, 0)
//   let shape = sdl_image.imgLoad("mask.png")  // or any SDL_Surface
//   sdl.setWindowShape(win, shape, sdl.binarizeMode(128))
//   ... event loop ...

use sdl_window { SDL_Window }
use sdl_core { Color }
use sdl_surface { SDL_Surface }

/// `WindowShapeMode` — how the shape surface's pixels translate
/// to opaque vs. cut-out regions.
pub enum WindowShapeModeKind: i32 {
    /// Binarise the surface's alpha channel at cutoff = 1
    /// (default).
    default               = 0
    /// Binarise the alpha channel at the user-supplied cutoff.
    binarizeAlpha         = 1
    /// Same as `binarizeAlpha` but the test is inverted.
    reverseBinarizeAlpha  = 2
    /// Treat pixels matching `colorKey` as transparent.
    colorKey              = 3
}

@extern(C) {
    /// Mirror of C `SDL_WindowShapeMode`. `mode` picks one of the
    /// `WindowShapeModeKind` values; `params0`..`params3` are the
    /// 4-byte union: either `binarizationCutoff` (in `params0`)
    /// or `colorKey` r/g/b/a in params0/1/2/3. Use the helper
    /// constructors `binarizeMode` / `reverseBinarizeMode` /
    /// `colorKeyMode` rather than building one by hand.
    pub struct WindowShapeMode {
        mode: i32
        params0: u8
        params1: u8
        params2: u8
        params3: u8
    }

    @lib("SDL2") pub fn SDL_CreateShapedWindow(
        title: *const char,
        x: u32, y: u32, w: u32, h: u32,
        flags: u32
    ): *SDL_Window
    @lib("SDL2") pub fn SDL_IsShapedWindow(window: *SDL_Window): u8
    @lib("SDL2") pub fn SDL_SetWindowShape(
        window: *SDL_Window, shape: *SDL_Surface,
        shape_mode: *WindowShapeMode
    ): i32
    @lib("SDL2") pub fn SDL_GetShapedWindowMode(
        window: *SDL_Window, shape_mode: *WindowShapeMode
    ): i32

    // ─── friendlier ilang wrappers ───────────────────────────────

    /// Build a `WindowShapeMode` for "binarize the alpha channel
    /// at `cutoff`". Pixels with α ≥ cutoff are window; the rest
    /// are cut-out.
    pub fn binarizeMode(cutoff: u8): WindowShapeMode {
        let m = new WindowShapeMode()
        m.mode = WindowShapeModeKind.binarizeAlpha as i32
        m.params0 = cutoff
        m
    }

    /// Inverse of `binarizeMode` — pixels with α < cutoff are
    /// window.
    pub fn reverseBinarizeMode(cutoff: u8): WindowShapeMode {
        let m = new WindowShapeMode()
        m.mode = WindowShapeModeKind.reverseBinarizeAlpha as i32
        m.params0 = cutoff
        m
    }

    /// Build a `WindowShapeMode` for "treat `colorKey` pixels as
    /// transparent". Useful for paletted / indexed shape masks
    /// where alpha isn't the right discriminator.
    pub fn colorKeyMode(colorKey: Color): WindowShapeMode {
        let m = new WindowShapeMode()
        m.mode = WindowShapeModeKind.colorKey as i32
        m.params0 = colorKey.r
        m.params1 = colorKey.g
        m.params2 = colorKey.b
        m.params3 = colorKey.a
        m
    }

    /// Create a shaped window. Same as `SDL_CreateWindow` except
    /// the resulting window can hold a non-rectangular silhouette
    /// once `setWindowShape` is called. Returns the raw
    /// `*SDL_Window` cast to i64 (use it with the `Window` class
    /// or pass to other SDL APIs).
    ///
    /// Args:
    ///   title         — window title.
    ///   x, y          — top-left corner. `0x1FFF0000` =
    ///                   `SDL_WINDOWPOS_CENTERED`.
    ///   w, h          — initial dimensions.
    ///   flags         — `WindowFlag.*` mask (see `sdl_window.il`).
    pub fn createShapedWindow(
        title: string, x: u32, y: u32, w: u32, h: u32, flags: u32
    ): i64 {
        SDL_CreateShapedWindow(
            cstrFromString(title), x, y, w, h, flags
        ) as i64
    }

    /// Returns:
    ///   true if `windowHandle` was created via
    ///   `createShapedWindow`. Plain `SDL_CreateWindow` results
    ///   are *not* shapeable.
    pub fn isShapedWindow(windowHandle: i64): bool {
        if windowHandle == 0 { return false }
        SDL_IsShapedWindow(windowHandle as *SDL_Window) != 0 as u8
    }

    /// Apply a shape mask to a shaped window.
    ///
    /// Args:
    ///   windowHandle  — `*SDL_Window` cast to i64.
    ///   shapeHandle   — `*SDL_Surface` cast to i64. The surface's
    ///                   pixels (filtered through `mode`) define
    ///                   which window pixels stay visible.
    ///   mode          — see `binarizeMode` / `reverseBinarizeMode`
    ///                   / `colorKeyMode`.
    ///
    /// Returns:
    ///   `0` on success, `< 0` on failure (call `sdl.getError()`).
    pub fn setWindowShape(
        windowHandle: i64, shapeHandle: i64, mode: WindowShapeMode
    ): i32 {
        if windowHandle == 0 || shapeHandle == 0 { return -1 }
        SDL_SetWindowShape(
            windowHandle as *SDL_Window,
            shapeHandle as *SDL_Surface,
            mode
        )
    }

    /// Read back the shape mode in effect for `windowHandle`.
    /// The mode is written into `out` (caller-allocated). Returns
    /// `0` on success, `< 0` if `windowHandle` isn't shaped or
    /// hasn't had a shape set yet.
    pub fn getShapedWindowMode(windowHandle: i64, out: WindowShapeMode): i32 {
        if windowHandle == 0 { return -1 }
        SDL_GetShapedWindowMode(windowHandle as *SDL_Window, out)
    }
}
