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

use sdl_core { Rect, Point, Size }
use sdl_surface { SDL_Surface }

/// `SDL_WindowFlags` — bitflags passed to `SDL_CreateWindow` / read
/// back via `Window.flags`.
@flags
enum WindowFlag: u32 {
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
enum WindowEvent: i32 {
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
enum WindowPos: i32 {
    undefined = 0x1FFF0000
    centered  = 0x2FFF0000
}

/// `SDL_DisplayOrientation`.
enum Orientation: i32 {
    unknown          = 0
    landscape        = 1
    landscapeFlipped = 2
    portrait         = 3
    portraitFlipped  = 4
}

/// `SDL_FlashOperation` passed to `Window.flash`.
enum FlashOp: i32 {
    cancel       = 0
    briefly      = 1
    untilFocused = 2
}

/// `SDL_GLattr` — keys for `glSetAttribute` / `glGetAttribute`.
enum GLAttr: i32 {
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
enum GLContextFlag: i32 {
    debug             = 0x0001
    forwardCompatible = 0x0002
    robustAccess      = 0x0004
    resetIsolation    = 0x0008
}

/// `SDL_GLprofile` — values for `GLAttr.contextProfileMask`.
@flags
enum GLProfile: i32 {
    core          = 0x0001
    compatibility = 0x0002
    es            = 0x0004
}

@extern(C) {
    /// Opaque SDL handle.
    struct SDL_Window {}
    /// Opaque OpenGL context handle.
    struct SDL_GLContext {}
    // (`SDL_Surface` is imported from `sdl_surface.il` so the
    // `*SDL_Surface`-taking fns below resolve to the canonical
    // type — avoids a duplicate `struct SDL_Surface {}` decl that
    // would clash through the `sdl.il` umbrella.)

    /// Filled by `SDL_GetDisplayMode` / `GetCurrentDisplayMode`
    /// etc. `driverdata` is opaque — kept as i64 so we don't touch
    /// it.
    struct SDL_DisplayMode {
        format: u32
        w: i32
        h: i32
        refreshRate: i32
        driverdata: i64
    }

    // ─── window CRUD ─────────────────────────────────────────────
    @lib("SDL2") fn SDL_CreateWindow(
        title: *const char,
        x: i32, y: i32, w: i32, h: i32,
        flags: u32
    ): *SDL_Window
    @lib("SDL2") fn SDL_CreateWindowFrom(data: *const void): *SDL_Window
    @lib("SDL2") fn SDL_DestroyWindow(window: *SDL_Window)
    @lib("SDL2") fn SDL_GetWindowID(window: *SDL_Window): u32
    @lib("SDL2") fn SDL_GetWindowFromID(id: u32): *SDL_Window
    @lib("SDL2") fn SDL_GetWindowFlags(window: *SDL_Window): u32
    @lib("SDL2") fn SDL_GetWindowPixelFormat(window: *SDL_Window): u32

    // ─── window title / icon ────────────────────────────────────
    @lib("SDL2") fn SDL_SetWindowTitle(window: *SDL_Window, title: *const char)
    @lib("SDL2") fn SDL_GetWindowTitle(window: *SDL_Window): *const char
    @lib("SDL2") fn SDL_SetWindowIcon(window: *SDL_Window, icon: *SDL_Surface)

    // ─── geometry ────────────────────────────────────────────────
    @lib("SDL2") fn SDL_SetWindowPosition(window: *SDL_Window, x: i32, y: i32)
    @lib("SDL2") fn SDL_GetWindowPosition(window: *SDL_Window, x: *i32, y: *i32)
    @lib("SDL2") fn SDL_SetWindowSize(window: *SDL_Window, w: i32, h: i32)
    @lib("SDL2") fn SDL_GetWindowSize(window: *SDL_Window, w: *i32, h: *i32)
    @lib("SDL2") fn SDL_GetWindowSizeInPixels(window: *SDL_Window, w: *i32, h: *i32)
    @lib("SDL2") fn SDL_SetWindowMinimumSize(window: *SDL_Window, min_w: i32, min_h: i32)
    @lib("SDL2") fn SDL_GetWindowMinimumSize(window: *SDL_Window, w: *i32, h: *i32)
    @lib("SDL2") fn SDL_SetWindowMaximumSize(window: *SDL_Window, max_w: i32, max_h: i32)
    @lib("SDL2") fn SDL_GetWindowMaximumSize(window: *SDL_Window, w: *i32, h: *i32)
    @lib("SDL2") fn SDL_GetWindowBordersSize(
        window: *SDL_Window, top: *i32, left: *i32, bottom: *i32, right: *i32
    ): i32

    // ─── decorations / state flags ───────────────────────────────
    @lib("SDL2") fn SDL_SetWindowBordered(window: *SDL_Window, bordered: i32)
    @lib("SDL2") fn SDL_SetWindowResizable(window: *SDL_Window, resizable: i32)
    @lib("SDL2") fn SDL_SetWindowAlwaysOnTop(window: *SDL_Window, on_top: i32)
    @lib("SDL2") fn SDL_SetWindowFullscreen(window: *SDL_Window, flags: u32): i32
    @lib("SDL2") fn SDL_SetWindowOpacity(window: *SDL_Window, opacity: f32): i32
    @lib("SDL2") fn SDL_GetWindowOpacity(window: *SDL_Window, out: *f32): i32

    // ─── show / hide / focus / minimise / maximise ───────────────
    @lib("SDL2") fn SDL_ShowWindow(window: *SDL_Window)
    @lib("SDL2") fn SDL_HideWindow(window: *SDL_Window)
    @lib("SDL2") fn SDL_RaiseWindow(window: *SDL_Window)
    @lib("SDL2") fn SDL_MaximizeWindow(window: *SDL_Window)
    @lib("SDL2") fn SDL_MinimizeWindow(window: *SDL_Window)
    @lib("SDL2") fn SDL_RestoreWindow(window: *SDL_Window)
    @lib("SDL2") fn SDL_SetWindowInputFocus(window: *SDL_Window): i32
    @lib("SDL2") fn SDL_FlashWindow(window: *SDL_Window, operation: i32): i32
    // Re-declared here so the Window class can expose a getter
    // without a cross-module @lib call. dlsym resolves both
    // declarations to the same C function.
    @lib("SDL2") fn SDL_IsScreenKeyboardShown(window: *SDL_Window): i32

    // ─── grab / capture ──────────────────────────────────────────
    @lib("SDL2") fn SDL_SetWindowGrab(window: *SDL_Window, grabbed: i32)
    @lib("SDL2") fn SDL_GetWindowGrab(window: *SDL_Window): i32
    @lib("SDL2") fn SDL_SetWindowKeyboardGrab(window: *SDL_Window, grabbed: i32)
    @lib("SDL2") fn SDL_GetWindowKeyboardGrab(window: *SDL_Window): i32
    @lib("SDL2") fn SDL_SetWindowMouseGrab(window: *SDL_Window, grabbed: i32)
    @lib("SDL2") fn SDL_GetWindowMouseGrab(window: *SDL_Window): i32
    @lib("SDL2") fn SDL_GetGrabbedWindow(): *SDL_Window
    @lib("SDL2") fn SDL_SetWindowMouseRect(window: *SDL_Window, rect: *Rect): i32
    @lib("SDL2") fn SDL_GetWindowMouseRect(window: *SDL_Window): *Rect
    @lib("SDL2") fn SDL_CaptureMouse(enabled: i32): i32

    // ─── modal / parent ──────────────────────────────────────────
    @lib("SDL2") fn SDL_SetWindowModalFor(modal: *SDL_Window, parent: *SDL_Window): i32

    // ─── display info ────────────────────────────────────────────
    @lib("SDL2") fn SDL_GetWindowDisplayIndex(window: *SDL_Window): i32
    @lib("SDL2") fn SDL_SetWindowDisplayMode(window: *SDL_Window, mode: *SDL_DisplayMode): i32
    @lib("SDL2") fn SDL_GetWindowDisplayMode(window: *SDL_Window, mode: *SDL_DisplayMode): i32
    @lib("SDL2") fn SDL_GetNumVideoDisplays(): i32
    @lib("SDL2") fn SDL_GetDisplayName(displayIndex: i32): *const char
    @lib("SDL2") fn SDL_GetDisplayBounds(displayIndex: i32, rect: *Rect): i32
    @lib("SDL2") fn SDL_GetDisplayUsableBounds(displayIndex: i32, rect: *Rect): i32
    @lib("SDL2") fn SDL_GetDisplayDPI(
        displayIndex: i32, ddpi: *f32, hdpi: *f32, vdpi: *f32
    ): i32
    @lib("SDL2") fn SDL_GetDisplayOrientation(displayIndex: i32): i32
    @lib("SDL2") fn SDL_GetNumDisplayModes(displayIndex: i32): i32
    @lib("SDL2") fn SDL_GetDisplayMode(
        displayIndex: i32, modeIndex: i32, mode: *SDL_DisplayMode
    ): i32
    @lib("SDL2") fn SDL_GetDesktopDisplayMode(
        displayIndex: i32, mode: *SDL_DisplayMode
    ): i32
    @lib("SDL2") fn SDL_GetCurrentDisplayMode(
        displayIndex: i32, mode: *SDL_DisplayMode
    ): i32
    @lib("SDL2") fn SDL_GetClosestDisplayMode(
        displayIndex: i32, mode: *SDL_DisplayMode, closest: *SDL_DisplayMode
    ): *SDL_DisplayMode

    // ─── video subsystem ─────────────────────────────────────────
    @lib("SDL2") fn SDL_VideoInit(driver_name: *const char): i32
    @lib("SDL2") fn SDL_VideoQuit()
    @lib("SDL2") fn SDL_GetCurrentVideoDriver(): *const char
    @lib("SDL2") fn SDL_GetVideoDriver(index: i32): *const char
    @lib("SDL2") fn SDL_GetNumVideoDrivers(): i32

    // ─── screen saver ────────────────────────────────────────────
    @lib("SDL2") fn SDL_EnableScreenSaver()
    @lib("SDL2") fn SDL_DisableScreenSaver()
    @lib("SDL2") fn SDL_IsScreenSaverEnabled(): i32

    // ─── window surface (software path) ──────────────────────────
    @lib("SDL2") fn SDL_GetWindowSurface(window: *SDL_Window): *SDL_Surface
    @lib("SDL2") fn SDL_HasWindowSurface(window: *SDL_Window): i32
    @lib("SDL2") fn SDL_DestroyWindowSurface(window: *SDL_Window): i32
    @lib("SDL2") fn SDL_UpdateWindowSurface(window: *SDL_Window): i32
    @lib("SDL2") fn SDL_UpdateWindowSurfaceRects(
        window: *SDL_Window, rects: *Rect, numrects: i32
    ): i32

    // ─── OpenGL ──────────────────────────────────────────────────
    @lib("SDL2") fn SDL_GL_LoadLibrary(path: *const char): i32
    @lib("SDL2") fn SDL_GL_UnloadLibrary()
    @lib("SDL2") fn SDL_GL_GetProcAddress(proc: *const char): *void
    @lib("SDL2") fn SDL_GL_ExtensionSupported(extension: *const char): i32
    @lib("SDL2") fn SDL_GL_ResetAttributes()
    @lib("SDL2") fn SDL_GL_SetAttribute(attr: i32, value: i32): i32
    @lib("SDL2") fn SDL_GL_GetAttribute(attr: i32, value: *i32): i32
    @lib("SDL2") fn SDL_GL_CreateContext(window: *SDL_Window): *SDL_GLContext
    @lib("SDL2") fn SDL_GL_MakeCurrent(window: *SDL_Window, context: *SDL_GLContext): i32
    @lib("SDL2") fn SDL_GL_GetCurrentWindow(): *SDL_Window
    @lib("SDL2") fn SDL_GL_GetCurrentContext(): *SDL_GLContext
    @lib("SDL2") fn SDL_GL_GetDrawableSize(window: *SDL_Window, w: *i32, h: *i32)
    @lib("SDL2") fn SDL_GL_SetSwapInterval(interval: i32): i32
    @lib("SDL2") fn SDL_GL_GetSwapInterval(): i32
    @lib("SDL2") fn SDL_GL_SwapWindow(window: *SDL_Window)
    @lib("SDL2") fn SDL_GL_DeleteContext(context: *SDL_GLContext)

    // ─── public ilang return types (no `init`) ───────────────────

    /// Display mode info — pixel format, resolution, refresh rate.
    class DisplayMode {
        format: u32
        w: i32
        h: i32
        refreshRate: i32
    }

    /// DPI triple — diagonal / horizontal / vertical dots-per-inch.
    class DisplayDPI {
        diagonal: f32
        horizontal: f32
        vertical: f32
    }

    /// Window decoration thicknesses in pixels.
    class WindowBorders {
        top: i32
        left: i32
        bottom: i32
        right: i32
    }

    // ─── friendly Window class (ARC-managed) ────────────────────

    /// ARC-managed wrapper around `SDL_Window`. `deinit` calls
    /// `SDL_DestroyWindow` automatically.
    class Window {
        /// Pointer to the underlying `SDL_Window`, cast to `i64`.
        /// `0` when creation failed (check `getError()` for why).
        handle: i64

        /// Convenience: undefined position on the default display,
        /// no flags.
        ///
        /// Args:
        ///   title — window caption (UTF-8).
        ///   w, h  — initial size in pixels.
        init(title: string, w: i32, h: i32) {
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
        init(title: string, x: i32, y: i32, w: i32, h: i32, flags: WindowFlag) {
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

        // ─── identity / state ────────────────────────────────────

        /// Per-instance window id (used by `SDL_Event.windowID`).
        get id(): u32 {
            SDL_GetWindowID(this.handle as *SDL_Window)
        }

        /// Current `WindowFlag` bitmask.
        get flags(): u32 {
            SDL_GetWindowFlags(this.handle as *SDL_Window)
        }

        /// Pixel format of the window's backing surface
        /// (`SDL_PIXELFORMAT_*`).
        get pixelFormat(): u32 {
            SDL_GetWindowPixelFormat(this.handle as *SDL_Window)
        }

        /// Index of the display the window is currently on
        /// (passes through to display-info module fns).
        get displayIndex(): i32 {
            SDL_GetWindowDisplayIndex(this.handle as *SDL_Window)
        }

        // ─── title ───────────────────────────────────────────────

        /// Current window caption.
        get title(): string {
            stringFromCstr(SDL_GetWindowTitle(this.handle as *SDL_Window))
        }
        set title(t: string) {
            SDL_SetWindowTitle(this.handle as *SDL_Window, cstrFromString(t))
        }

        // ─── geometry ────────────────────────────────────────────

        /// Top-left corner in screen coordinates.
        get position(): Point {
            let xx: i32[] = [0]
            let yy: i32[] = [0]
            SDL_GetWindowPosition(this.handle as *SDL_Window, xx, yy)
            let p = new Point()
            p.x = xx[0]
            p.y = yy[0]
            p
        }

        /// Move the window. `x` / `y` may be `WindowPos.undefined`
        /// or `WindowPos.centered` (cast to `i32`) sentinels.
        setPosition(x: i32, y: i32) {
            SDL_SetWindowPosition(this.handle as *SDL_Window, x, y)
        }

        /// Logical size (matches `init`'s `w` / `h` arguments).
        get size(): Size {
            let ww: i32[] = [0]
            let hh: i32[] = [0]
            SDL_GetWindowSize(this.handle as *SDL_Window, ww, hh)
            let s = new Size()
            s.w = ww[0]
            s.h = hh[0]
            s
        }
        setSize(w: i32, h: i32) {
            SDL_SetWindowSize(this.handle as *SDL_Window, w, h)
        }

        /// Drawable size in physical pixels (differs from `size` on
        /// HiDPI displays).
        get sizeInPixels(): Size {
            let ww: i32[] = [0]
            let hh: i32[] = [0]
            SDL_GetWindowSizeInPixels(this.handle as *SDL_Window, ww, hh)
            let s = new Size()
            s.w = ww[0]
            s.h = hh[0]
            s
        }

        /// Minimum size the user can resize to.
        get minimumSize(): Size {
            let ww: i32[] = [0]
            let hh: i32[] = [0]
            SDL_GetWindowMinimumSize(this.handle as *SDL_Window, ww, hh)
            let s = new Size()
            s.w = ww[0]
            s.h = hh[0]
            s
        }
        setMinimumSize(w: i32, h: i32) {
            SDL_SetWindowMinimumSize(this.handle as *SDL_Window, w, h)
        }

        /// Maximum size the user can resize to.
        get maximumSize(): Size {
            let ww: i32[] = [0]
            let hh: i32[] = [0]
            SDL_GetWindowMaximumSize(this.handle as *SDL_Window, ww, hh)
            let s = new Size()
            s.w = ww[0]
            s.h = hh[0]
            s
        }
        setMaximumSize(w: i32, h: i32) {
            SDL_SetWindowMaximumSize(this.handle as *SDL_Window, w, h)
        }

        /// Decoration thicknesses (title-bar / borders) in pixels.
        get borders(): WindowBorders {
            let t: i32[] = [0]
            let l: i32[] = [0]
            let b: i32[] = [0]
            let r: i32[] = [0]
            SDL_GetWindowBordersSize(this.handle as *SDL_Window, t, l, b, r)
            let wb = new WindowBorders()
            wb.top = t[0]
            wb.left = l[0]
            wb.bottom = b[0]
            wb.right = r[0]
            wb
        }

        // ─── decorations (write-only properties) ─────────────────

        /// Show or hide the window border / title bar.
        set bordered(on: bool) {
            SDL_SetWindowBordered(
                this.handle as *SDL_Window, if on { 1 } else { 0 }
            )
        }

        /// Allow / forbid the user resizing the window.
        set resizable(on: bool) {
            SDL_SetWindowResizable(
                this.handle as *SDL_Window, if on { 1 } else { 0 }
            )
        }

        /// Pin the window above all others (or release).
        set alwaysOnTop(on: bool) {
            SDL_SetWindowAlwaysOnTop(
                this.handle as *SDL_Window, if on { 1 } else { 0 }
            )
        }

        // ─── fullscreen / opacity ────────────────────────────────

        /// Toggle borderless-fullscreen mode (`fullscreenDesktop`).
        ///
        /// Setters drop SDL's `i32` status. Call
        /// `SDL_SetWindowFullscreen` directly through its `@lib`
        /// decl if you need to inspect it.
        set fullscreen(on: bool) {
            let flag: u32 = if on { 0x1001 as u32 } else { 0 as u32 }
            SDL_SetWindowFullscreen(this.handle as *SDL_Window, flag)
            return
        }

        /// Raw fullscreen flag — pick `WindowFlag.fullscreen` for
        /// exclusive mode or `WindowFlag.fullscreenDesktop` for
        /// borderless. The boolean `set fullscreen` above uses the
        /// latter.
        set fullscreenFlag(flag: WindowFlag) {
            SDL_SetWindowFullscreen(this.handle as *SDL_Window, flag as u32)
            return
        }

        /// Window opacity in `[0.0, 1.0]`.
        get opacity(): f32 {
            let o: f32[] = [0.0 as f32]
            SDL_GetWindowOpacity(this.handle as *SDL_Window, o)
            o[0]
        }
        set opacity(o: f32) {
            SDL_SetWindowOpacity(this.handle as *SDL_Window, o)
            return
        }

        // ─── visibility / focus ──────────────────────────────────

        /// Make the window visible.
        show() { SDL_ShowWindow(this.handle as *SDL_Window) }
        /// Hide the window without destroying it.
        hide() { SDL_HideWindow(this.handle as *SDL_Window) }
        /// Bring the window to the front and request focus.
        raise() { SDL_RaiseWindow(this.handle as *SDL_Window) }
        /// Maximise the window.
        maximize() { SDL_MaximizeWindow(this.handle as *SDL_Window) }
        /// Minimise the window to the taskbar / dock.
        minimize() { SDL_MinimizeWindow(this.handle as *SDL_Window) }
        /// Restore from minimised / maximised back to its saved size.
        restore() { SDL_RestoreWindow(this.handle as *SDL_Window) }

        /// Forcibly move keyboard focus to this window.
        ///
        /// Returns:
        ///   `0` on success, `< 0` on failure.
        setInputFocus(): i32 {
            SDL_SetWindowInputFocus(this.handle as *SDL_Window)
        }

        /// Flash the window / taskbar entry to attract attention.
        ///
        /// Args:
        ///   operation — `cancel`, `briefly`, or `untilFocused`.
        ///
        /// Returns:
        ///   `0` on success, `< 0` if unsupported on this platform.
        flash(operation: FlashOp): i32 {
            SDL_FlashWindow(this.handle as *SDL_Window, operation as i32)
        }

        // ─── grab ────────────────────────────────────────────────

        /// Whether either keyboard or mouse is grabbed by this
        /// window.
        get grab(): bool {
            SDL_GetWindowGrab(this.handle as *SDL_Window) != 0
        }
        set grab(on: bool) {
            SDL_SetWindowGrab(
                this.handle as *SDL_Window, if on { 1 } else { 0 }
            )
        }

        /// Keyboard-only grab.
        get keyboardGrab(): bool {
            SDL_GetWindowKeyboardGrab(this.handle as *SDL_Window) != 0
        }
        set keyboardGrab(on: bool) {
            SDL_SetWindowKeyboardGrab(
                this.handle as *SDL_Window, if on { 1 } else { 0 }
            )
        }

        /// Mouse-only grab.
        get mouseGrab(): bool {
            SDL_GetWindowMouseGrab(this.handle as *SDL_Window) != 0
        }
        set mouseGrab(on: bool) {
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
        get isScreenKeyboardShown(): bool {
            SDL_IsScreenKeyboardShown(this.handle as *SDL_Window) != 0
        }

        // ─── display mode ────────────────────────────────────────

        /// Display mode the window currently uses (most useful for
        /// fullscreen windows; in windowed mode it's effectively
        /// the desktop mode).
        get currentDisplayMode(): DisplayMode {
            let raw = new SDL_DisplayMode()
            SDL_GetWindowDisplayMode(this.handle as *SDL_Window, raw)
            let m = new DisplayMode()
            m.format = raw.format
            m.w = raw.w
            m.h = raw.h
            m.refreshRate = raw.refreshRate
            m
        }
    }

    // ─── module-level video / display fns ────────────────────────

    /// Returns:
    ///   number of displays connected to the system.
    fn numVideoDisplays(): i32 {
        SDL_GetNumVideoDisplays()
    }

    /// Args:
    ///   displayIndex — `0`-based, `< numVideoDisplays()`.
    ///
    /// Returns:
    ///   human-readable display name.
    fn displayName(displayIndex: i32): string {
        stringFromCstr(SDL_GetDisplayName(displayIndex))
    }

    /// Display bounds in screen coordinates.
    fn displayBounds(displayIndex: i32): Rect {
        let r = new Rect()
        SDL_GetDisplayBounds(displayIndex, r)
        r
    }

    /// Display bounds excluding system UI (taskbar / dock).
    fn displayUsableBounds(displayIndex: i32): Rect {
        let r = new Rect()
        SDL_GetDisplayUsableBounds(displayIndex, r)
        r
    }

    /// Diagonal / horizontal / vertical DPI for a display.
    fn displayDPI(displayIndex: i32): DisplayDPI {
        let dd: f32[] = [0.0 as f32]
        let hd: f32[] = [0.0 as f32]
        let vd: f32[] = [0.0 as f32]
        SDL_GetDisplayDPI(displayIndex, dd, hd, vd)
        let d = new DisplayDPI()
        d.diagonal = dd[0]
        d.horizontal = hd[0]
        d.vertical = vd[0]
        d
    }

    /// Returns:
    ///   `Orientation.*` value as a raw `i32`.
    fn displayOrientation(displayIndex: i32): i32 {
        SDL_GetDisplayOrientation(displayIndex)
    }

    /// Returns:
    ///   number of distinct display modes the display supports.
    fn numDisplayModes(displayIndex: i32): i32 {
        SDL_GetNumDisplayModes(displayIndex)
    }

    /// Args:
    ///   displayIndex — display.
    ///   modeIndex    — `0` is the largest / highest refresh.
    ///
    /// Returns:
    ///   the corresponding `DisplayMode`.
    fn displayMode(displayIndex: i32, modeIndex: i32): DisplayMode {
        let raw = new SDL_DisplayMode()
        SDL_GetDisplayMode(displayIndex, modeIndex, raw)
        let m = new DisplayMode()
        m.format = raw.format
        m.w = raw.w
        m.h = raw.h
        m.refreshRate = raw.refreshRate
        m
    }

    /// Mode the OS desktop is configured for.
    fn desktopDisplayMode(displayIndex: i32): DisplayMode {
        let raw = new SDL_DisplayMode()
        SDL_GetDesktopDisplayMode(displayIndex, raw)
        let m = new DisplayMode()
        m.format = raw.format
        m.w = raw.w
        m.h = raw.h
        m.refreshRate = raw.refreshRate
        m
    }

    /// Mode currently active (matches desktop unless a fullscreen
    /// app changed it).
    fn currentDisplayMode(displayIndex: i32): DisplayMode {
        let raw = new SDL_DisplayMode()
        SDL_GetCurrentDisplayMode(displayIndex, raw)
        let m = new DisplayMode()
        m.format = raw.format
        m.w = raw.w
        m.h = raw.h
        m.refreshRate = raw.refreshRate
        m
    }

    /// Re-initialise the video subsystem with a different driver.
    ///
    /// Args:
    ///   driver — name as returned by `videoDriver` (e.g.
    ///            `"cocoa"`, `"x11"`, `"wayland"`).
    ///
    /// Returns:
    ///   `0` on success, `< 0` on failure.
    fn videoInit(driver: string): i32 {
        SDL_VideoInit(cstrFromString(driver))
    }

    /// Tear down the video subsystem.
    fn videoQuit() {
        SDL_VideoQuit()
    }

    /// Returns:
    ///   name of the active video driver.
    fn currentVideoDriver(): string {
        stringFromCstr(SDL_GetCurrentVideoDriver())
    }

    /// Args:
    ///   index — `0`-based, `< numVideoDrivers()`.
    fn videoDriver(index: i32): string {
        stringFromCstr(SDL_GetVideoDriver(index))
    }

    /// Returns:
    ///   number of compiled-in video drivers.
    fn numVideoDrivers(): i32 {
        SDL_GetNumVideoDrivers()
    }

    /// Allow the screen saver to kick in.
    fn enableScreenSaver() {
        SDL_EnableScreenSaver()
    }

    /// Suppress the screen saver (matches "video is playing" behaviour).
    fn disableScreenSaver() {
        SDL_DisableScreenSaver()
    }

    /// Returns:
    ///   true when the screen saver is currently allowed.
    fn isScreenSaverEnabled(): bool {
        SDL_IsScreenSaverEnabled() != 0
    }

    // ─── OpenGL helpers ──────────────────────────────────────────

    /// Load an OpenGL library by file name (or system default if
    /// `path` is empty).
    fn glLoadLibrary(path: string): i32 {
        SDL_GL_LoadLibrary(cstrFromString(path))
    }
    fn glUnloadLibrary() {
        SDL_GL_UnloadLibrary()
    }

    /// Args:
    ///   name — extension name (e.g. `"GL_ARB_compute_shader"`).
    ///
    /// Returns:
    ///   true when the extension is reported by the GL driver.
    fn glExtensionSupported(name: string): bool {
        SDL_GL_ExtensionSupported(cstrFromString(name)) != 0
    }

    /// Reset every GL attribute to its SDL default before context
    /// creation.
    fn glResetAttributes() {
        SDL_GL_ResetAttributes()
    }

    /// Set a GL attribute used at context creation.
    ///
    /// Args:
    ///   attr  — attribute key.
    ///   value — attribute value (interpretation depends on `attr`).
    fn glSetAttribute(attr: GLAttr, value: i32): i32 {
        SDL_GL_SetAttribute(attr as i32, value)
    }

    /// Read back a GL attribute.
    fn glGetAttribute(attr: GLAttr): i32 {
        let buf: i32[] = [0]
        SDL_GL_GetAttribute(attr as i32, buf)
        buf[0]
    }

    /// Args:
    ///   interval — `0` = immediate, `1` = vsync,
    ///              `-1` = adaptive vsync.
    fn glSetSwapInterval(interval: i32): i32 {
        SDL_GL_SetSwapInterval(interval)
    }

    /// Returns:
    ///   the current swap-interval setting.
    fn glGetSwapInterval(): i32 {
        SDL_GL_GetSwapInterval()
    }

    /// Look up a GL function pointer by name. Caller invokes the
    /// pointer through `fnAddr`-style FFI plumbing.
    ///
    /// Returns:
    ///   raw `void *` cast to i64. `0` if the GL driver doesn't
    ///   expose `proc`.
    fn glGetProcAddress(proc: string): i64 {
        SDL_GL_GetProcAddress(cstrFromString(proc)) as i64
    }

    /// Create a GL context bound to `windowHandle`. The window
    /// must have been created with `WindowFlag.openGL`.
    ///
    /// Args:
    ///   windowHandle — `*SDL_Window` cast to i64 (e.g. `w.handle`).
    ///
    /// Returns:
    ///   raw `SDL_GLContext` cast to i64. `0` on failure.
    fn glCreateContext(windowHandle: i64): i64 {
        SDL_GL_CreateContext(windowHandle as *SDL_Window) as i64
    }

    /// Bind `context` as the current GL context for
    /// `windowHandle`. Call once per window after creation, and
    /// again whenever switching between contexts.
    ///
    /// Returns:
    ///   `0` on success, `< 0` on failure.
    fn glMakeCurrent(windowHandle: i64, context: i64): i32 {
        SDL_GL_MakeCurrent(
            windowHandle as *SDL_Window, context as *SDL_GLContext
        )
    }

    /// Returns:
    ///   `*SDL_Window` of the currently-bound GL context cast to
    ///   i64. `0` when no context is current on this thread.
    fn glGetCurrentWindow(): i64 {
        SDL_GL_GetCurrentWindow() as i64
    }

    fn glGetCurrentContext(): i64 {
        SDL_GL_GetCurrentContext() as i64
    }

    /// Read the drawable size in *pixels* — distinct from the
    /// window's logical size on HiDPI displays.
    ///
    /// Args:
    ///   windowHandle — `*SDL_Window` cast to i64.
    ///   wOut, hOut    — 1-element i32 arrays to receive the
    ///                  width / height.
    fn glGetDrawableSize(windowHandle: i64, wOut: i32[], hOut: i32[]) {
        let pw: *i32 = wOut
        let ph: *i32 = hOut
        SDL_GL_GetDrawableSize(windowHandle as *SDL_Window, pw, ph)
    }

    /// Present the back buffer to the screen. Call once per
    /// rendered frame.
    fn glSwapWindow(windowHandle: i64) {
        SDL_GL_SwapWindow(windowHandle as *SDL_Window)
    }

    /// Destroy a GL context.
    fn glDeleteContext(context: i64) {
        if context != 0 {
            SDL_GL_DeleteContext(context as *SDL_GLContext)
        }
    }
}
