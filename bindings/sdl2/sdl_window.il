// SDL2 window + video subsystem. Mirrors SDL_video.h: window
// CRUD, display queries, video driver enumeration, the screen
// saver, and the OpenGL attribute / context API.
//
// `*SDL_Window`, `*SDL_DisplayMode`, etc. only appear inside the
// `@extern(C) {}` block. Out-params are wrapped behind plain
// classes (`Point` / `Size` / `Rect` / `DisplayMode`)
// so the public API hands back ordinary ilang values.
//
// Bindings deliberately skipped (callbacks / opaque void*):
//   SDL_SetWindowHitTest, SDL_GetWindowICCProfile,
//   SDL_GetWindowData / SDL_SetWindowData,
//   SDL_GetWindowGammaRamp / SDL_SetWindowGammaRamp,
//   SDL_GL_GetProcAddress.

use sdl_core { Rect, Point, Size, Color }
use sdl_surface { Surface, SDL_Surface }

/// `SDL_WindowFlags` — bitflags passed to `SDL_CreateWindow` / read
/// back via `Window.flags`.
@flags
pub enum WindowFlag: u32 {
    fullscreen        = 0x00000001
    opengl            = 0x00000002
    shown             = 0x00000004
    hidden            = 0x00000008
    borderless        = 0x00000010
    resizable         = 0x00000020
    minimized         = 0x00000040
    maximized         = 0x00000080
    mouseGrabbed      = 0x00000100
    inputFocus        = 0x00000200
    mouseFocus        = 0x00000400
    fullscreenDesktop = 0x00001001
    foreign           = 0x00000800
    allowHighDpi      = 0x00002000
    mouseCapture      = 0x00004000
    alwaysOnTop       = 0x00008000
    skipTaskbar       = 0x00010000
    utility           = 0x00020000
    tooltip           = 0x00040000
    popupMenu         = 0x00080000
    keyboardGrabbed   = 0x00100000
    vulkan            = 0x10000000
    metal             = 0x20000000
}

/// `SDL_WindowEventID` — sub-events of `EventType.windowEvent`.
pub enum WindowEvent: i32 {
    none            = 0
    shown           = 1
    hidden          = 2
    exposed         = 3
    moved           = 4
    resized         = 5
    sizeChanged     = 6
    minimized       = 7
    maximized       = 8
    restored        = 9
    enter           = 10
    leave           = 11
    focusGained     = 12
    focusLost       = 13
    close           = 14
    takeFocus       = 15
    hitTest         = 16
    iccprofChanged  = 17
    displayChanged  = 18
}

/// `SDL_WINDOWPOS_*` helpers for `SDL_CreateWindow` x/y.
pub enum WindowPos: i32 {
    undefined = 0x1FFF0000
    centered  = 0x2FFF0000
}

/// `SDL_DisplayOrientation`.
pub enum Orientation: i32 {
    unknown          = 0
    landscape        = 1
    landscapeFlipped = 2
    portrait         = 3
    portraitFlipped  = 4
}

/// `SDL_FlashOperation` passed to `Window.flash`.
pub enum FlashOp: i32 {
    cancel       = 0
    briefly      = 1
    untilFocused = 2
}

/// `SDL_GLattr` — keys for `glSetAttribute` / `glGetAttribute`.
pub enum GLAttr: i32 {
    redSize                  = 0
    greenSize                = 1
    blueSize                 = 2
    alphaSize                = 3
    bufferSize               = 4
    doubleBuffer             = 5
    depthSize                = 6
    stencilSize              = 7
    accumRedSize             = 8
    accumGreenSize           = 9
    accumBlueSize            = 10
    accumAlphaSize           = 11
    stereo                   = 12
    multisampleBuffers       = 13
    multisampleSamples       = 14
    acceleratedVisual        = 15
    retainedBacking          = 16
    contextMajorVersion      = 17
    contextMinorVersion      = 18
    contextEgl               = 19
    contextFlags             = 20
    contextProfileMask       = 21
    shareWithCurrentContext  = 22
    framebufferSrgbCapable   = 23
    contextReleaseBehavior   = 24
    contextResetNotification = 25
    contextNoError           = 26
    floatbuffers             = 27
}

/// `SDL_GLcontextFlag` — bitflag values for `GLAttr.contextFlags`.
@flags
pub enum GLContextFlag: i32 {
    debug             = 0x0001
    forwardCompatible = 0x0002
    robustAccess      = 0x0004
    resetIsolation    = 0x0008
}

/// `SDL_GLprofile` — values for `GLAttr.contextProfileMask`.
@flags
pub enum GLProfile: i32 {
    core          = 0x0001
    compatibility = 0x0002
    es            = 0x0004
}

/// `WindowShapeModeKind` — how a shape surface's pixels translate
/// to opaque vs. cut-out regions for `Window.setShape`. Mirror of
/// C's `WindowShapeMode` enum (SDL_shape.h).
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

/// `SDL_SYSWM_TYPE` — which platform / windowing system a
/// `SysWMinfo` payload was filled in for. Read back from
/// `SysWMinfo.subsystem` after `Window.wmInfo()`.
pub enum SubSystem: u32 {
    unknown   = 0
    windows   = 1
    x11       = 2
    directfb  = 3
    cocoa     = 4
    uikit     = 5
    wayland   = 6
    mir       = 7
    winrt     = 8
    android   = 9
    vivante   = 10
    os2       = 11
    haiku     = 12
    kmsdrm    = 13
    riscos    = 14
}

@extern(C) {
    /// Opaque SDL handle.
    pub struct SDL_Window {}
    /// Opaque OpenGL context handle.
    pub struct SDL_GLContext {}
    // (`SDL_Surface` is imported from `sdl_surface.il` so the
    // `*SDL_Surface`-taking fns below resolve to the canonical
    // type — avoids a duplicate `struct SDL_Surface {}` decl that
    // would clash through the `sdl.il` umbrella.)

    /// Filled by `SDL_GetDisplayMode` / `GetCurrentDisplayMode`
    /// etc. `driverdata` is opaque — kept as i64 so we don't touch
    /// it.
    pub struct SDL_DisplayMode {
        format: u32
        w: i32
        h: i32
        refreshRate: i32
        driverdata: i64
    }

    // ─── window CRUD ─────────────────────────────────────────────
    @lib("SDL2") pub fn SDL_CreateWindow(
        title: *const char,
        x: i32, y: i32, w: i32, h: i32,
        flags: u32
    ): *SDL_Window
    @lib("SDL2") pub fn SDL_CreateWindowFrom(data: *const void): *SDL_Window
    @lib("SDL2") pub fn SDL_DestroyWindow(window: *SDL_Window)
    @lib("SDL2") pub fn SDL_GetWindowID(window: *SDL_Window): u32
    @lib("SDL2") pub fn SDL_GetWindowFromID(id: u32): *SDL_Window
    @lib("SDL2") pub fn SDL_GetWindowFlags(window: *SDL_Window): u32
    @lib("SDL2") pub fn SDL_GetWindowPixelFormat(window: *SDL_Window): u32

    // ─── window title / icon ────────────────────────────────────
    @lib("SDL2") pub fn SDL_SetWindowTitle(window: *SDL_Window, title: *const char)
    @lib("SDL2") pub fn SDL_GetWindowTitle(window: *SDL_Window): *const char
    @lib("SDL2") pub fn SDL_SetWindowIcon(window: *SDL_Window, icon: *SDL_Surface)

    // ─── geometry ────────────────────────────────────────────────
    @lib("SDL2") pub fn SDL_SetWindowPosition(window: *SDL_Window, x: i32, y: i32)
    @lib("SDL2") pub fn SDL_GetWindowPosition(window: *SDL_Window, x: *i32, y: *i32)
    @lib("SDL2") pub fn SDL_SetWindowSize(window: *SDL_Window, w: i32, h: i32)
    @lib("SDL2") pub fn SDL_GetWindowSize(window: *SDL_Window, w: *i32, h: *i32)
    @lib("SDL2") pub fn SDL_GetWindowSizeInPixels(window: *SDL_Window, w: *i32, h: *i32)
    @lib("SDL2") pub fn SDL_SetWindowMinimumSize(window: *SDL_Window, min_w: i32, min_h: i32)
    @lib("SDL2") pub fn SDL_GetWindowMinimumSize(window: *SDL_Window, w: *i32, h: *i32)
    @lib("SDL2") pub fn SDL_SetWindowMaximumSize(window: *SDL_Window, max_w: i32, max_h: i32)
    @lib("SDL2") pub fn SDL_GetWindowMaximumSize(window: *SDL_Window, w: *i32, h: *i32)
    @lib("SDL2") pub fn SDL_GetWindowBordersSize(
        window: *SDL_Window, top: *i32, left: *i32, bottom: *i32, right: *i32
    ): i32

    // ─── decorations / state flags ───────────────────────────────
    @lib("SDL2") pub fn SDL_SetWindowBordered(window: *SDL_Window, bordered: i32)
    @lib("SDL2") pub fn SDL_SetWindowResizable(window: *SDL_Window, resizable: i32)
    @lib("SDL2") pub fn SDL_SetWindowAlwaysOnTop(window: *SDL_Window, on_top: i32)
    @lib("SDL2") pub fn SDL_SetWindowFullscreen(window: *SDL_Window, flags: u32): i32
    @lib("SDL2") pub fn SDL_SetWindowOpacity(window: *SDL_Window, opacity: f32): i32
    @lib("SDL2") pub fn SDL_GetWindowOpacity(window: *SDL_Window, out: *f32): i32

    // ─── show / hide / focus / minimise / maximise ───────────────
    @lib("SDL2") pub fn SDL_ShowWindow(window: *SDL_Window)
    @lib("SDL2") pub fn SDL_HideWindow(window: *SDL_Window)
    @lib("SDL2") pub fn SDL_RaiseWindow(window: *SDL_Window)
    @lib("SDL2") pub fn SDL_MaximizeWindow(window: *SDL_Window)
    @lib("SDL2") pub fn SDL_MinimizeWindow(window: *SDL_Window)
    @lib("SDL2") pub fn SDL_RestoreWindow(window: *SDL_Window)
    @lib("SDL2") pub fn SDL_SetWindowInputFocus(window: *SDL_Window): i32
    @lib("SDL2") pub fn SDL_FlashWindow(window: *SDL_Window, operation: i32): i32
    // Re-declared here so the Window class can expose a getter
    // without a cross-module @lib call. dlsym resolves both
    // declarations to the same C function.
    @lib("SDL2") pub fn SDL_IsScreenKeyboardShown(window: *SDL_Window): i32

    // ─── grab / capture ──────────────────────────────────────────
    @lib("SDL2") pub fn SDL_SetWindowGrab(window: *SDL_Window, grabbed: i32)
    @lib("SDL2") pub fn SDL_GetWindowGrab(window: *SDL_Window): i32
    @lib("SDL2") pub fn SDL_SetWindowKeyboardGrab(window: *SDL_Window, grabbed: i32)
    @lib("SDL2") pub fn SDL_GetWindowKeyboardGrab(window: *SDL_Window): i32
    @lib("SDL2") pub fn SDL_SetWindowMouseGrab(window: *SDL_Window, grabbed: i32)
    @lib("SDL2") pub fn SDL_GetWindowMouseGrab(window: *SDL_Window): i32
    @lib("SDL2") pub fn SDL_GetGrabbedWindow(): *SDL_Window
    @lib("SDL2") pub fn SDL_SetWindowMouseRect(window: *SDL_Window, rect: *Rect): i32
    @lib("SDL2") pub fn SDL_GetWindowMouseRect(window: *SDL_Window): *Rect
    @lib("SDL2") pub fn SDL_CaptureMouse(enabled: i32): i32

    // ─── modal / parent ──────────────────────────────────────────
    @lib("SDL2") pub fn SDL_SetWindowModalFor(modal: *SDL_Window, parent: *SDL_Window): i32

    // ─── display info ────────────────────────────────────────────
    @lib("SDL2") pub fn SDL_GetWindowDisplayIndex(window: *SDL_Window): i32
    @lib("SDL2") pub fn SDL_SetWindowDisplayMode(window: *SDL_Window, mode: *SDL_DisplayMode): i32
    @lib("SDL2") pub fn SDL_GetWindowDisplayMode(window: *SDL_Window, mode: *SDL_DisplayMode): i32
    @lib("SDL2") pub fn SDL_GetNumVideoDisplays(): i32
    @lib("SDL2") pub fn SDL_GetDisplayName(displayIndex: i32): *const char
    @lib("SDL2") pub fn SDL_GetDisplayBounds(displayIndex: i32, rect: *Rect): i32
    @lib("SDL2") pub fn SDL_GetDisplayUsableBounds(displayIndex: i32, rect: *Rect): i32
    @lib("SDL2") pub fn SDL_GetDisplayDPI(
        displayIndex: i32, ddpi: *f32, hdpi: *f32, vdpi: *f32
    ): i32
    @lib("SDL2") pub fn SDL_GetDisplayOrientation(displayIndex: i32): i32
    @lib("SDL2") pub fn SDL_GetNumDisplayModes(displayIndex: i32): i32
    @lib("SDL2") pub fn SDL_GetDisplayMode(
        displayIndex: i32, modeIndex: i32, mode: *SDL_DisplayMode
    ): i32
    @lib("SDL2") pub fn SDL_GetDesktopDisplayMode(
        displayIndex: i32, mode: *SDL_DisplayMode
    ): i32
    @lib("SDL2") pub fn SDL_GetCurrentDisplayMode(
        displayIndex: i32, mode: *SDL_DisplayMode
    ): i32
    @lib("SDL2") pub fn SDL_GetClosestDisplayMode(
        displayIndex: i32, mode: *SDL_DisplayMode, closest: *SDL_DisplayMode
    ): *SDL_DisplayMode

    // ─── video subsystem ─────────────────────────────────────────
    @lib("SDL2") pub fn SDL_VideoInit(driver_name: *const char): i32
    @lib("SDL2") pub fn SDL_VideoQuit()
    @lib("SDL2") pub fn SDL_GetCurrentVideoDriver(): *const char
    @lib("SDL2") pub fn SDL_GetVideoDriver(index: i32): *const char
    @lib("SDL2") pub fn SDL_GetNumVideoDrivers(): i32

    // ─── screen saver ────────────────────────────────────────────
    @lib("SDL2") pub fn SDL_EnableScreenSaver()
    @lib("SDL2") pub fn SDL_DisableScreenSaver()
    @lib("SDL2") pub fn SDL_IsScreenSaverEnabled(): i32

    // ─── window surface (software path) ──────────────────────────
    @lib("SDL2") pub fn SDL_GetWindowSurface(window: *SDL_Window): *SDL_Surface
    @lib("SDL2") pub fn SDL_HasWindowSurface(window: *SDL_Window): i32
    @lib("SDL2") pub fn SDL_DestroyWindowSurface(window: *SDL_Window): i32
    @lib("SDL2") pub fn SDL_UpdateWindowSurface(window: *SDL_Window): i32
    @lib("SDL2") pub fn SDL_UpdateWindowSurfaceRects(
        window: *SDL_Window, rects: *Rect, numrects: i32
    ): i32

    // ─── OpenGL ──────────────────────────────────────────────────
    @lib("SDL2") pub fn SDL_GL_LoadLibrary(path: *const char): i32
    @lib("SDL2") pub fn SDL_GL_UnloadLibrary()
    @lib("SDL2") pub fn SDL_GL_GetProcAddress(proc: *const char): *void
    @lib("SDL2") pub fn SDL_GL_ExtensionSupported(extension: *const char): i32
    @lib("SDL2") pub fn SDL_GL_ResetAttributes()
    @lib("SDL2") pub fn SDL_GL_SetAttribute(attr: i32, value: i32): i32
    @lib("SDL2") pub fn SDL_GL_GetAttribute(attr: i32, value: *i32): i32
    @lib("SDL2") pub fn SDL_GL_CreateContext(window: *SDL_Window): *SDL_GLContext
    @lib("SDL2") pub fn SDL_GL_MakeCurrent(window: *SDL_Window, context: *SDL_GLContext): i32
    @lib("SDL2") pub fn SDL_GL_GetCurrentWindow(): *SDL_Window
    @lib("SDL2") pub fn SDL_GL_GetCurrentContext(): *SDL_GLContext
    @lib("SDL2") pub fn SDL_GL_GetDrawableSize(window: *SDL_Window, w: *i32, h: *i32)
    @lib("SDL2") pub fn SDL_GL_SetSwapInterval(interval: i32): i32
    @lib("SDL2") pub fn SDL_GL_GetSwapInterval(): i32
    @lib("SDL2") pub fn SDL_GL_SwapWindow(window: *SDL_Window)
    @lib("SDL2") pub fn SDL_GL_DeleteContext(context: *SDL_GLContext)

    // ─── shaped windows (SDL_shape.h) ────────────────────────────
    //
    // Shaped windows hold a non-rectangular silhouette: the
    // shape mask is a `Surface` whose alpha (or colour-key)
    // selects which window pixels stay visible. Built via
    // `Window.createShaped(...)`; the shape is applied with
    // `win.setShape(surface, mode)`.

    /// Mirror of C `SDL_WindowShapeMode`. `mode` picks one of the
    /// `WindowShapeModeKind` values; `params0`..`params3` are the
    /// 4-byte union: either `binarizationCutoff` (in `params0`)
    /// or `colorKey` r/g/b/a in params0/1/2/3. Use the
    /// `WindowShape.binarize(...)` / `.reverseBinarize(...)` /
    /// `.colorKey(...)` constructors rather than building one by
    /// hand.
    pub struct WindowShapeMode {
        mode: WindowShapeModeKind
        params0: u8
        params1: u8
        params2: u8
        params3: u8
    }

    /// Namespace class for the `WindowShapeMode` constructors.
    /// All methods are `static` — they return a freshly-built
    /// `WindowShapeMode` to hand to `Window.setShape(...)`.
    pub class WindowShape {
        /// `WindowShapeMode` for "binarize the alpha channel at
        /// `cutoff`". Pixels with α ≥ cutoff are window; the rest
        /// are cut-out.
        pub static binarize(cutoff: u8): WindowShapeMode {
            let m = new WindowShapeMode()
            m.mode = WindowShapeModeKind.binarizeAlpha
            m.params0 = cutoff
            m
        }

        /// Inverse of `binarize` — pixels with α < cutoff are
        /// window.
        pub static reverseBinarize(cutoff: u8): WindowShapeMode {
            let m = new WindowShapeMode()
            m.mode = WindowShapeModeKind.reverseBinarizeAlpha
            m.params0 = cutoff
            m
        }

        /// `WindowShapeMode` for "treat `colorKey` pixels as
        /// transparent". Useful for paletted / indexed shape
        /// masks where alpha isn't the right discriminator.
        pub static colorKey(colorKey: Color): WindowShapeMode {
            let m = new WindowShapeMode()
            m.mode = WindowShapeModeKind.colorKey
            m.params0 = colorKey.r
            m.params1 = colorKey.g
            m.params2 = colorKey.b
            m.params3 = colorKey.a
            m
        }
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

    // ─── system window manager info (SDL_syswm.h) ────────────────
    //
    // Pulls native handles (`HWND`, `NSWindow*`, `wl_surface*`,
    // …) for interop with non-SDL toolkits (Vulkan surface
    // creation, dear-imgui platform backends, native menus, …).
    // SDL's `SysWMinfo` is a tagged union whose layout differs
    // per build platform; we model it as a fixed-size struct
    // (8-byte header + 4 i64 handle slots + 216-byte tail) and
    // read individual handles through the per-platform helpers
    // below. They guard each read on the recorded `subsystem`.

    /// 256-byte buffer matching the layout C's `SDL_SysWMinfo`
    /// uses on every supported platform:
    ///
    ///   offset  size  meaning
    ///   ──────  ────  ─────────────────────────────────────────
    ///        0     3  SDL_version (major, minor, patch as Uint8)
    ///        3     1  ABI padding to align `subsystem`
    ///        4     4  subsystem (matches `SubSystem.*`)
    ///        8     8  handle1 — primary native handle
    ///       16     8  handle2 — secondary handle
    ///       24     8  handle3 — tertiary handle
    ///       32     8  handle4 — quaternary (Wayland egl_window etc.)
    ///       40   216  tail padding (forward-compat for new SDL
    ///                 union variants — Wayland xdg_surface,
    ///                 xdg_toplevel, etc.)
    pub struct SysWMinfo {
        versionMajor: u8
        versionMinor: u8
        versionPatch: u8
        _pad0: u8
        subsystem: u32
        handle1: i64
        handle2: i64
        handle3: i64
        handle4: i64
        _padTail: u8[216]
    }

    @lib("SDL2") pub fn SDL_GetWindowWMInfo(
        window: *SDL_Window, info: *SysWMinfo
    ): u8

    // ─── public ilang return types (no `init`) ───────────────────

    /// Display mode info — pixel format, resolution, refresh rate.
    pub class DisplayMode {
        pub format: u32
        pub w: i32
        pub h: i32
        pub refreshRate: i32
    }

    /// DPI triple — diagonal / horizontal / vertical dots-per-inch.
    pub class DisplayDPI {
        pub diagonal: f32
        pub horizontal: f32
        pub vertical: f32
    }

    /// Window decoration thicknesses in pixels.
    pub class WindowBorders {
        pub top: i32
        pub left: i32
        pub bottom: i32
        pub right: i32
    }

    // ─── friendly Window class (ARC-managed) ────────────────────

    /// ARC-managed wrapper around `SDL_Window`. `deinit` calls
    /// `SDL_DestroyWindow` automatically.
    pub class Window {
        /// Pointer to the underlying `SDL_Window`, cast to `i64`.
        /// `0` when creation failed (check `getError()` for why).
        pub handle: i64

        /// Convenience: undefined position on the default display,
        /// no flags.
        ///
        /// Args:
        ///   title — window caption (UTF-8).
        ///   w, h  — initial size in pixels.
        pub init(title: string, w: i32, h: i32) {
            let flags: u32 = 0
            // SDL_WINDOWPOS_UNDEFINED = 0x1FFF0000. The module-level
            // `WindowPos` enum isn't visible from inside this @extern
            // block once the file is imported through the umbrella,
            // so the literal is inlined here.
            let undef: i32 = 0x1FFF0000
            let win = SDL_CreateWindow(
                cstrFromString(title), undef, undef, w, h, flags
            )
            this.handle = win as i64
        }

        /// Full `SDL_CreateWindow` surface.
        ///
        /// Args:
        ///   title — window caption.
        ///   x, y  — initial position; use `WindowPos.undefined` /
        ///           `WindowPos.centered` (cast to `i32`) for the
        ///           "let SDL pick" sentinels.
        ///   w, h  — initial size in pixels.
        ///   flags — `WindowFlag` bitmask (resizable, hidden,
        ///           fullscreen, …).
        pub init(title: string, x: i32, y: i32, w: i32, h: i32, flags: WindowFlag) {
            let win = SDL_CreateWindow(
                cstrFromString(title), x, y, w, h, flags as u32
            )
            this.handle = win as i64
        }

        deinit() {
            if this.handle != 0 {
                SDL_DestroyWindow(this.handle as *SDL_Window)
            }
        }

        /// Internal "wrap an existing handle" init. Distinguished
        /// from the public ones by its single-i64 signature — the
        /// other inits all start with the title string. Used only
        /// by `Window.fromHandle(...)`.
        pub init(handle: i64) {
            this.handle = handle
        }

        /// Wrap a raw `*SDL_Window` (cast to i64) returned by
        /// another module (e.g. `sdl_shape.createShapedWindow`).
        /// Takes ownership: `deinit` will call `SDL_DestroyWindow`.
        pub static fromHandle(handle: i64): Window {
            new Window(handle)
        }

        // ─── identity / state ────────────────────────────────────

        /// Per-instance window id (used by `SDL_Event.windowID`).
        pub get id(): u32 {
            SDL_GetWindowID(this.handle as *SDL_Window)
        }

        /// Current `WindowFlag` bitmask.
        pub get flags(): u32 {
            SDL_GetWindowFlags(this.handle as *SDL_Window)
        }

        /// Pixel format of the window's backing surface
        /// (`SDL_PIXELFORMAT_*`).
        pub get pixelFormat(): u32 {
            SDL_GetWindowPixelFormat(this.handle as *SDL_Window)
        }

        /// Index of the display the window is currently on
        /// (passes through to display-info module fns).
        pub get displayIndex(): i32 {
            SDL_GetWindowDisplayIndex(this.handle as *SDL_Window)
        }

        // ─── title ───────────────────────────────────────────────

        /// Current window caption.
        pub get title(): string {
            stringFromCstr(SDL_GetWindowTitle(this.handle as *SDL_Window))
        }
        pub set title(t: string) {
            SDL_SetWindowTitle(this.handle as *SDL_Window, cstrFromString(t))
        }

        // ─── geometry ────────────────────────────────────────────

        /// Top-left corner in screen coordinates.
        pub get position(): Point {
            let p = new Point()
            SDL_GetWindowPosition(this.handle as *SDL_Window, &p.x, &p.y)
            p
        }

        /// Move the window. `x` / `y` may be `WindowPos.undefined`
        /// or `WindowPos.centered` (cast to `i32`) sentinels.
        pub setPosition(x: i32, y: i32) {
            SDL_SetWindowPosition(this.handle as *SDL_Window, x, y)
        }

        /// Logical size (matches `init`'s `w` / `h` arguments).
        pub get size(): Size {
            let s = new Size()
            SDL_GetWindowSize(this.handle as *SDL_Window, &s.w, &s.h)
            s
        }
        pub setSize(w: i32, h: i32) {
            SDL_SetWindowSize(this.handle as *SDL_Window, w, h)
        }

        /// Drawable size in physical pixels (differs from `size` on
        /// HiDPI displays).
        pub get sizeInPixels(): Size {
            let s = new Size()
            SDL_GetWindowSizeInPixels(this.handle as *SDL_Window, &s.w, &s.h)
            s
        }

        /// Minimum size the user can resize to.
        pub get minimumSize(): Size {
            let s = new Size()
            SDL_GetWindowMinimumSize(this.handle as *SDL_Window, &s.w, &s.h)
            s
        }
        pub setMinimumSize(w: i32, h: i32) {
            SDL_SetWindowMinimumSize(this.handle as *SDL_Window, w, h)
        }

        /// Maximum size the user can resize to.
        pub get maximumSize(): Size {
            let s = new Size()
            SDL_GetWindowMaximumSize(this.handle as *SDL_Window, &s.w, &s.h)
            s
        }
        pub setMaximumSize(w: i32, h: i32) {
            SDL_SetWindowMaximumSize(this.handle as *SDL_Window, w, h)
        }

        /// Decoration thicknesses (title-bar / borders) in pixels.
        pub get borders(): WindowBorders {
            let wb = new WindowBorders()
            SDL_GetWindowBordersSize(
                this.handle as *SDL_Window,
                &wb.top, &wb.left, &wb.bottom, &wb.right
            )
            wb
        }

        // ─── decorations ─────────────────────────────────────────
        //
        // SDL2 only exposes setters for these — the read side is
        // recovered by bit-testing `SDL_GetWindowFlags()` against
        // the corresponding `WindowFlag.*` so the property always
        // reflects SDL's current state (including changes made
        // outside our setter, e.g. WM keyboard shortcut, OS
        // fullscreen toggle).

        /// Whether the window has a border / title bar.
        pub get isBordered(): bool {
            if this.handle == 0 { return false }
            let flags = SDL_GetWindowFlags(this.handle as *SDL_Window)
            (flags & WindowFlag.borderless as u32) == 0 as u32
        }
        pub set isBordered(on: bool) {
            SDL_SetWindowBordered(
                this.handle as *SDL_Window, if on { 1 } else { 0 }
            )
        }

        /// Whether the user can resize the window by dragging an
        /// edge.
        pub get isResizable(): bool {
            if this.handle == 0 { return false }
            let flags = SDL_GetWindowFlags(this.handle as *SDL_Window)
            (flags & WindowFlag.resizable as u32) != 0 as u32
        }
        pub set isResizable(on: bool) {
            SDL_SetWindowResizable(
                this.handle as *SDL_Window, if on { 1 } else { 0 }
            )
        }

        /// Whether the window is pinned above all others.
        pub get isAlwaysOnTop(): bool {
            if this.handle == 0 { return false }
            let flags = SDL_GetWindowFlags(this.handle as *SDL_Window)
            (flags & WindowFlag.alwaysOnTop as u32) != 0 as u32
        }
        pub set isAlwaysOnTop(on: bool) {
            SDL_SetWindowAlwaysOnTop(
                this.handle as *SDL_Window, if on { 1 } else { 0 }
            )
        }

        // ─── fullscreen / opacity ────────────────────────────────

        /// Whether the window is in fullscreen mode (either
        /// exclusive `fullscreen` or borderless `fullscreenDesktop`
        /// — both report true). The setter switches to / from
        /// borderless-fullscreen specifically; for exclusive mode
        /// use `fullscreenFlag = WindowFlag.fullscreen`.
        ///
        /// SDL2 has no dedicated `Get` for the fullscreen state;
        /// we bit-test `SDL_GetWindowFlags()` instead so the
        /// property reflects external changes (OS hotkeys, etc.).
        pub get isFullscreen(): bool {
            if this.handle == 0 { return false }
            let flags = SDL_GetWindowFlags(this.handle as *SDL_Window)
            (flags & WindowFlag.fullscreen as u32) != 0 as u32
        }
        pub set isFullscreen(on: bool) {
            let flag: u32 = if on { 0x1001 as u32 } else { 0 as u32 }
            SDL_SetWindowFullscreen(this.handle as *SDL_Window, flag)
            return
        }

        /// Raw fullscreen flag — pick `WindowFlag.fullscreen` for
        /// exclusive mode or `WindowFlag.fullscreenDesktop` for
        /// borderless. The boolean `set fullscreen` above uses the
        /// latter.
        pub set fullscreenFlag(flag: WindowFlag) {
            SDL_SetWindowFullscreen(this.handle as *SDL_Window, flag as u32)
            return
        }

        /// Window opacity in `[0.0, 1.0]`.
        pub get opacity(): f32 {
            let o: f32 = 0.0 as f32
            SDL_GetWindowOpacity(this.handle as *SDL_Window, &o)
            o
        }
        pub set opacity(o: f32) {
            SDL_SetWindowOpacity(this.handle as *SDL_Window, o)
            return
        }

        // ─── visibility / focus ──────────────────────────────────

        /// Make the window visible.
        pub show() { SDL_ShowWindow(this.handle as *SDL_Window) }
        /// Hide the window without destroying it.
        pub hide() { SDL_HideWindow(this.handle as *SDL_Window) }
        /// Bring the window to the front and request focus.
        pub raise() { SDL_RaiseWindow(this.handle as *SDL_Window) }
        /// Maximise the window.
        pub maximize() { SDL_MaximizeWindow(this.handle as *SDL_Window) }
        /// Minimise the window to the taskbar / dock.
        pub minimize() { SDL_MinimizeWindow(this.handle as *SDL_Window) }
        /// Restore from minimised / maximised back to its saved size.
        pub restore() { SDL_RestoreWindow(this.handle as *SDL_Window) }

        /// Forcibly move keyboard focus to this window.
        ///
        /// Returns:
        ///   `0` on success, `< 0` on failure.
        pub setInputFocus(): i32 {
            SDL_SetWindowInputFocus(this.handle as *SDL_Window)
        }

        /// Flash the window / taskbar entry to attract attention.
        ///
        /// Args:
        ///   operation — `cancel`, `briefly`, or `untilFocused`.
        ///
        /// Returns:
        ///   `0` on success, `< 0` if unsupported on this platform.
        pub flash(operation: FlashOp): i32 {
            SDL_FlashWindow(this.handle as *SDL_Window, operation as i32)
        }

        // ─── grab ────────────────────────────────────────────────

        /// Whether either keyboard or mouse is grabbed by this
        /// window.
        pub get isGrabbed(): bool {
            SDL_GetWindowGrab(this.handle as *SDL_Window) != 0
        }
        pub set isGrabbed(on: bool) {
            SDL_SetWindowGrab(
                this.handle as *SDL_Window, if on { 1 } else { 0 }
            )
        }

        /// Keyboard-only grab.
        pub get isKeyboardGrabbed(): bool {
            SDL_GetWindowKeyboardGrab(this.handle as *SDL_Window) != 0
        }
        pub set isKeyboardGrabbed(on: bool) {
            SDL_SetWindowKeyboardGrab(
                this.handle as *SDL_Window, if on { 1 } else { 0 }
            )
        }

        /// Mouse-only grab.
        pub get isMouseGrabbed(): bool {
            SDL_GetWindowMouseGrab(this.handle as *SDL_Window) != 0
        }
        pub set isMouseGrabbed(on: bool) {
            SDL_SetWindowMouseGrab(
                this.handle as *SDL_Window, if on { 1 } else { 0 }
            )
        }

        // ─── on-screen keyboard ──────────────────────────────────

        /// Returns:
        ///   true when the OS's on-screen keyboard is currently
        ///   visible for this window (mainly relevant on touch
        ///   devices / iOS / Android — desktop platforms always
        ///   return false).
        pub get isScreenKeyboardShown(): bool {
            SDL_IsScreenKeyboardShown(this.handle as *SDL_Window) != 0
        }

        // ─── display mode ────────────────────────────────────────

        /// Display mode the window currently uses (most useful for
        /// fullscreen windows; in windowed mode it's effectively
        /// the desktop mode).
        pub get currentDisplayMode(): DisplayMode {
            let raw = new SDL_DisplayMode()
            SDL_GetWindowDisplayMode(this.handle as *SDL_Window, raw)
            let m = new DisplayMode()
            m.format = raw.format
            m.w = raw.w
            m.h = raw.h
            m.refreshRate = raw.refreshRate
            m
        }

        // ─── shape (SDL_shape.h) ─────────────────────────────────

        /// Create a shaped window. Same as `new Window(...)` except
        /// the resulting window can hold a non-rectangular
        /// silhouette once `setShape(...)` is called.
        ///
        /// Args:
        ///   title — window title.
        ///   x, y  — top-left corner. `0x1FFF0000` =
        ///           `SDL_WINDOWPOS_CENTERED`.
        ///   w, h  — initial dimensions.
        ///   flags — `WindowFlag.*` mask.
        ///
        /// Returns:
        ///   a `Window` whose `handle` is `0` if creation failed.
        ///   ARC frees it with `SDL_DestroyWindow`.
        pub static createShaped(
            title: string, x: u32, y: u32, w: u32, h: u32, flags: u32
        ): Window {
            let raw = SDL_CreateShapedWindow(
                cstrFromString(title), x, y, w, h, flags
            ) as i64
            Window.fromHandle(raw)
        }

        /// Returns:
        ///   true if this window was created via
        ///   `Window.createShaped(...)`. Plain `new Window(...)`
        ///   results are *not* shapeable.
        pub get isShaped(): bool {
            if this.handle == 0 { return false }
            SDL_IsShapedWindow(this.handle as *SDL_Window) != 0 as u8
        }

        /// Apply a shape mask.
        ///
        /// Args:
        ///   shape — surface whose pixels (filtered through
        ///           `mode`) define which window pixels stay
        ///           visible.
        ///   mode  — see `WindowShape.binarize(...)` /
        ///           `.reverseBinarize(...)` / `.colorKey(...)`.
        ///
        /// Returns:
        ///   `0` on success, `< 0` on failure (call
        ///   `sdl.getError()`).
        pub setShape(shape: Surface, mode: WindowShapeMode): i32 {
            if this.handle == 0 || shape.handle == 0 { return -1 }
            SDL_SetWindowShape(
                this.handle as *SDL_Window,
                shape.handle as *SDL_Surface,
                mode
            )
        }

        /// Read back the shape mode in effect for this window.
        /// Writes into `out` (caller-allocated). Returns `0` on
        /// success, `< 0` if this window isn't shaped or hasn't
        /// had a shape set yet.
        pub shapedMode(out: WindowShapeMode): i32 {
            if this.handle == 0 { return -1 }
            SDL_GetShapedWindowMode(this.handle as *SDL_Window, out)
        }

        // ─── system window manager (SDL_syswm.h) ─────────────────

        /// Pull the native window-manager info for this window.
        /// Initialises the `version` field with `2.0.0` so SDL
        /// agrees to populate the rest.
        ///
        /// Returns:
        ///   a freshly populated `SysWMinfo`, or one with
        ///   `subsystem == SubSystem.unknown` if the call failed
        ///   (typically because the window's handle is `0` or
        ///   SDL's binary-compat check rejected our version).
        pub wmInfo(): SysWMinfo {
            let info = new SysWMinfo()
            if this.handle == 0 {
                return info
            }
            // SDL's compat gate accepts any major == 2 build; the
            // minor / patch are informational. Use 2.0.0 as a
            // stable floor that every SDL2 release honours.
            info.versionMajor = 2 as u8
            info.versionMinor = 0 as u8
            info.versionPatch = 0 as u8
            let ok = SDL_GetWindowWMInfo(this.handle as *SDL_Window, info)
            if ok == 0 as u8 {
                // Reset subsystem so callers see `unknown` rather
                // than whatever stale bytes happened to be in
                // `info`.
                info.subsystem = SubSystem.unknown as u32
            }
            info
        }

        // ─── platform native-handle accessors ────────────────────
        //
        // Each instance method calls `wmInfo()` internally, gates
        // on the recorded subsystem, and returns the matching
        // native handle cast to `i64` (or `0` when the platform
        // doesn't match / the wmInfo call failed).
        //
        // For batch reads (multiple handles in one go) call
        // `wmInfo()` once and read `info.handle1`/`handle2`/
        // `handle3`/`handle4` directly to avoid re-issuing
        // `SDL_GetWindowWMInfo` per accessor.

        /// macOS Cocoa: underlying `NSWindow *`.
        pub get cocoaWindow(): i64 {
            let info = this.wmInfo()
            if info.subsystem != SubSystem.cocoa as u32 { return 0 }
            info.handle1
        }

        /// Windows: native `HWND`.
        pub get win32Hwnd(): i64 {
            let info = this.wmInfo()
            if info.subsystem != SubSystem.windows as u32 { return 0 }
            info.handle1
        }

        /// Windows: `HDC`.
        pub get win32Hdc(): i64 {
            let info = this.wmInfo()
            if info.subsystem != SubSystem.windows as u32 { return 0 }
            info.handle2
        }

        /// Windows: `HINSTANCE`.
        pub get win32Hinstance(): i64 {
            let info = this.wmInfo()
            if info.subsystem != SubSystem.windows as u32 { return 0 }
            info.handle3
        }

        /// X11: `Display *`.
        pub get x11Display(): i64 {
            let info = this.wmInfo()
            if info.subsystem != SubSystem.x11 as u32 { return 0 }
            info.handle1
        }

        /// X11: `Window` (an unsigned long XID).
        pub get x11Window(): i64 {
            let info = this.wmInfo()
            if info.subsystem != SubSystem.x11 as u32 { return 0 }
            info.handle2
        }

        /// Wayland: `wl_display *`.
        pub get waylandDisplay(): i64 {
            let info = this.wmInfo()
            if info.subsystem != SubSystem.wayland as u32 { return 0 }
            info.handle1
        }

        /// Wayland: `wl_surface *`.
        pub get waylandSurface(): i64 {
            let info = this.wmInfo()
            if info.subsystem != SubSystem.wayland as u32 { return 0 }
            info.handle2
        }

        /// Wayland: legacy `wl_shell_surface *`. Newer compositors
        /// use xdg_surface / xdg_toplevel which live in the tail
        /// padding — extract them with `readU64` against the raw
        /// struct address if needed.
        pub get waylandShellSurface(): i64 {
            let info = this.wmInfo()
            if info.subsystem != SubSystem.wayland as u32 { return 0 }
            info.handle3
        }

        /// UIKit (iOS / tvOS): `UIWindow *`.
        pub get uiKitWindow(): i64 {
            let info = this.wmInfo()
            if info.subsystem != SubSystem.uikit as u32 { return 0 }
            info.handle1
        }

        /// Android: native `ANativeWindow *`.
        pub get androidWindow(): i64 {
            let info = this.wmInfo()
            if info.subsystem != SubSystem.android as u32 { return 0 }
            info.handle1
        }
    }

    /// Reference to a single display (monitor) by its `0`-based
    /// index. Construct via `new Display(idx)` after checking
    /// `Display.count()`. Methods here wrap the per-display SDL
    /// queries; the underlying API is index-keyed (no SDL handle
    /// to free), so this class is a thin façade rather than an
    /// ARC-managed resource.
    pub class Display {
        pub index: i32

        pub init(index: i32) {
            this.index = index
        }

        /// Returns:
        ///   number of displays connected to the system.
        pub static count(): i32 {
            SDL_GetNumVideoDisplays()
        }

        /// Returns:
        ///   human-readable display name.
        pub get name(): string {
            stringFromCstr(SDL_GetDisplayName(this.index))
        }

        /// Display bounds in screen coordinates.
        pub get bounds(): Rect {
            let r = new Rect()
            SDL_GetDisplayBounds(this.index, r)
            r
        }

        /// Display bounds excluding system UI (taskbar / dock).
        pub get usableBounds(): Rect {
            let r = new Rect()
            SDL_GetDisplayUsableBounds(this.index, r)
            r
        }

        /// Diagonal / horizontal / vertical DPI for the display.
        pub get dpi(): DisplayDPI {
            let d = new DisplayDPI()
            SDL_GetDisplayDPI(this.index, &d.diagonal, &d.horizontal, &d.vertical)
            d
        }

        /// Returns:
        ///   `Orientation.*` value as a raw `i32`.
        pub get orientation(): i32 {
            SDL_GetDisplayOrientation(this.index)
        }

        /// Returns:
        ///   number of distinct display modes the display supports.
        pub get numModes(): i32 {
            SDL_GetNumDisplayModes(this.index)
        }

        /// Args:
        ///   modeIndex — `0` is the largest / highest refresh.
        ///
        /// Returns:
        ///   the corresponding `DisplayMode`.
        pub mode(modeIndex: i32): DisplayMode {
            let raw = new SDL_DisplayMode()
            SDL_GetDisplayMode(this.index, modeIndex, raw)
            let m = new DisplayMode()
            m.format = raw.format
            m.w = raw.w
            m.h = raw.h
            m.refreshRate = raw.refreshRate
            m
        }

        /// Mode the OS desktop is configured for.
        pub get desktopMode(): DisplayMode {
            let raw = new SDL_DisplayMode()
            SDL_GetDesktopDisplayMode(this.index, raw)
            let m = new DisplayMode()
            m.format = raw.format
            m.w = raw.w
            m.h = raw.h
            m.refreshRate = raw.refreshRate
            m
        }

        /// Mode currently active (matches desktop unless a
        /// fullscreen app changed it).
        pub get currentMode(): DisplayMode {
            let raw = new SDL_DisplayMode()
            SDL_GetCurrentDisplayMode(this.index, raw)
            let m = new DisplayMode()
            m.format = raw.format
            m.w = raw.w
            m.h = raw.h
            m.refreshRate = raw.refreshRate
            m
        }
    }

    /// Namespace class for SDL's video subsystem — driver
    /// enumeration, screen-saver toggle, and the rare
    /// re-initialise path. All methods are `static`; there is no
    /// per-instance Video object to construct.
    pub class Video {
        /// Re-initialise the video subsystem with a different driver.
        ///
        /// Args:
        ///   driver — name as returned by `Video.driverName(idx)`
        ///            (e.g. `"cocoa"`, `"x11"`, `"wayland"`).
        ///
        /// Returns:
        ///   `0` on success, `< 0` on failure.
        pub static init(driver: string): i32 {
            SDL_VideoInit(cstrFromString(driver))
        }

        /// Tear down the video subsystem.
        pub static quit() {
            SDL_VideoQuit()
        }

        /// Returns:
        ///   name of the active video driver.
        pub static currentDriver(): string {
            stringFromCstr(SDL_GetCurrentVideoDriver())
        }

        /// Args:
        ///   index — `0`-based, `< Video.numDrivers()`.
        pub static driverName(index: i32): string {
            stringFromCstr(SDL_GetVideoDriver(index))
        }

        /// Returns:
        ///   number of compiled-in video drivers.
        pub static numDrivers(): i32 {
            SDL_GetNumVideoDrivers()
        }

        /// Allow the screen saver to kick in.
        pub static enableScreenSaver() {
            SDL_EnableScreenSaver()
        }

        /// Suppress the screen saver (matches "video is playing"
        /// behaviour).
        pub static disableScreenSaver() {
            SDL_DisableScreenSaver()
        }

        /// Returns:
        ///   true when the screen saver is currently allowed.
        pub static isScreenSaverEnabled(): bool {
            SDL_IsScreenSaverEnabled() != 0
        }
    }

    /// Namespace class for SDL's OpenGL helpers — library
    /// loading, attribute negotiation, context lifecycle, and
    /// per-frame swap. All methods are `static` (the GL context
    /// itself is an opaque `i64` handle returned by
    /// `GL.createContext(window)` and freed by
    /// `GL.deleteContext(context)`).
    pub class GL {
        /// Load an OpenGL library by file name (or the system
        /// default when `path` is empty).
        pub static loadLibrary(path: string): i32 {
            SDL_GL_LoadLibrary(cstrFromString(path))
        }

        pub static unloadLibrary() {
            SDL_GL_UnloadLibrary()
        }

        /// Args:
        ///   name — extension name (e.g.
        ///          `"GL_ARB_compute_shader"`).
        ///
        /// Returns:
        ///   true when the extension is reported by the GL
        ///   driver.
        pub static extensionSupported(name: string): bool {
            SDL_GL_ExtensionSupported(cstrFromString(name)) != 0
        }

        /// Reset every GL attribute to its SDL default before
        /// context creation.
        pub static resetAttributes() {
            SDL_GL_ResetAttributes()
        }

        /// Set a GL attribute used at context creation.
        ///
        /// Args:
        ///   attr  — attribute key.
        ///   value — attribute value (interpretation depends on
        ///           `attr`).
        pub static setAttribute(attr: GLAttr, value: i32): i32 {
            SDL_GL_SetAttribute(attr as i32, value)
        }

        /// Read back a GL attribute.
        pub static getAttribute(attr: GLAttr): i32 {
            let buf: i32 = 0
            SDL_GL_GetAttribute(attr as i32, &buf)
            buf
        }

        /// Args:
        ///   interval — `0` = immediate, `1` = vsync,
        ///              `-1` = adaptive vsync.
        pub static setSwapInterval(interval: i32): i32 {
            SDL_GL_SetSwapInterval(interval)
        }

        /// Returns:
        ///   the current swap-interval setting.
        pub static getSwapInterval(): i32 {
            SDL_GL_GetSwapInterval()
        }

        /// Look up a GL function pointer by name. Caller invokes
        /// the pointer through `fnAddr`-style FFI plumbing.
        ///
        /// Returns:
        ///   raw `void *` cast to i64. `0` if the GL driver
        ///   doesn't expose `proc`.
        pub static getProcAddress(proc: string): i64 {
            SDL_GL_GetProcAddress(cstrFromString(proc)) as i64
        }

        /// Create a GL context bound to `window`. The window must
        /// have been created with `WindowFlag.openGL`.
        ///
        /// Returns:
        ///   raw `SDL_GLContext` cast to i64. `0` on failure.
        pub static createContext(window: Window): i64 {
            if window.handle == 0 { return 0 }
            SDL_GL_CreateContext(window.handle as *SDL_Window) as i64
        }

        /// Bind `context` as the current GL context for `window`.
        /// Call once per window after creation, and again whenever
        /// switching between contexts.
        ///
        /// Returns:
        ///   `0` on success, `< 0` on failure.
        pub static makeCurrent(window: Window, context: i64): i32 {
            if window.handle == 0 { return -1 }
            SDL_GL_MakeCurrent(
                window.handle as *SDL_Window, context as *SDL_GLContext
            )
        }

        /// Returns:
        ///   `*SDL_Window` of the currently-bound GL context cast
        ///   to i64. `0` when no context is current on this
        ///   thread.
        pub static currentWindow(): i64 {
            SDL_GL_GetCurrentWindow() as i64
        }

        pub static currentContext(): i64 {
            SDL_GL_GetCurrentContext() as i64
        }

        /// Read the drawable size in *pixels* — distinct from the
        /// window's logical size on HiDPI displays.
        ///
        /// Args:
        ///   window     — the GL window.
        ///   wOut, hOut — 1-element i32 arrays to receive the
        ///                width / height.
        pub static drawableSize(window: Window, wOut: i32[], hOut: i32[]) {
            if window.handle == 0 { return }
            let pw: *i32 = wOut
            let ph: *i32 = hOut
            SDL_GL_GetDrawableSize(window.handle as *SDL_Window, pw, ph)
        }

        /// Present the back buffer to the screen. Call once per
        /// rendered frame.
        pub static swapWindow(window: Window) {
            if window.handle != 0 {
                SDL_GL_SwapWindow(window.handle as *SDL_Window)
            }
        }

        /// Destroy a GL context.
        pub static deleteContext(context: i64) {
            if context != 0 {
                SDL_GL_DeleteContext(context as *SDL_GLContext)
            }
        }
    }
}
