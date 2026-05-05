// SDL2 window + video subsystem. Mirrors SDL_video.h: window
// CRUD, display queries, video driver enumeration, the screen
// saver, and the OpenGL attribute / context API.
//
// `*SDL_Window`, `*SDL_DisplayMode`, etc. only appear inside the
// `@extern(C) {}` block. Out-params are wrapped behind classes
// (`Point` / `Size` / `Rect` / `DisplayMode`) so the public API
// hands back ordinary ilang values.
//
// Bindings deliberately skipped (callbacks / opaque void*):
//   SDL_SetWindowHitTest, SDL_GetWindowICCProfile,
//   SDL_GetWindowData / SDL_SetWindowData,
//   SDL_GetWindowGammaRamp / SDL_SetWindowGammaRamp,
//   SDL_GL_GetProcAddress.

@extern(C) {
    // Opaque handle types. Only their pointer types escape the
    // block.
    struct SDL_Window {}
    struct SDL_GLContext {}
    struct SDL_Surface {}

    // Filled by SDL_GetDisplayMode / GetCurrentDisplayMode etc.
    // `driverdata` is a void* — opaque, kept as i64 so we don't
    // touch it.
    struct SDL_DisplayMode {
        format: u32
        w: i32
        h: i32
        refresh_rate: i32
        driverdata: i64
    }

    // 16 B rectangle matching C `SDL_Rect`. Distinct ilang name
    // from `sdl_renderer.SDL_Rect` so each module re-exports its
    // own copy without collision.
    struct WindowRect {
        x: i32
        y: i32
        w: i32
        h: i32
    }

    // Single-i32 cell used as an out-param target. SDL's int*
    // out-params can't be reached from ilang directly (no field-
    // address-of), so we declare each `int *` slot as `*IntOut`
    // and the C side just writes 4 bytes at the struct address.
    struct IntOut { v: i32 }
    struct FloatOut { v: f32 }

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
    @lib("SDL2") fn SDL_GetWindowPosition(window: *SDL_Window, x: *IntOut, y: *IntOut)
    @lib("SDL2") fn SDL_SetWindowSize(window: *SDL_Window, w: i32, h: i32)
    @lib("SDL2") fn SDL_GetWindowSize(window: *SDL_Window, w: *IntOut, h: *IntOut)
    @lib("SDL2") fn SDL_GetWindowSizeInPixels(window: *SDL_Window, w: *IntOut, h: *IntOut)
    @lib("SDL2") fn SDL_SetWindowMinimumSize(window: *SDL_Window, min_w: i32, min_h: i32)
    @lib("SDL2") fn SDL_GetWindowMinimumSize(window: *SDL_Window, w: *IntOut, h: *IntOut)
    @lib("SDL2") fn SDL_SetWindowMaximumSize(window: *SDL_Window, max_w: i32, max_h: i32)
    @lib("SDL2") fn SDL_GetWindowMaximumSize(window: *SDL_Window, w: *IntOut, h: *IntOut)
    @lib("SDL2") fn SDL_GetWindowBordersSize(
        window: *SDL_Window, top: *IntOut, left: *IntOut, bottom: *IntOut, right: *IntOut
    ): i32

    // ─── decorations / state flags ───────────────────────────────
    @lib("SDL2") fn SDL_SetWindowBordered(window: *SDL_Window, bordered: i32)
    @lib("SDL2") fn SDL_SetWindowResizable(window: *SDL_Window, resizable: i32)
    @lib("SDL2") fn SDL_SetWindowAlwaysOnTop(window: *SDL_Window, on_top: i32)
    @lib("SDL2") fn SDL_SetWindowFullscreen(window: *SDL_Window, flags: u32): i32
    @lib("SDL2") fn SDL_SetWindowOpacity(window: *SDL_Window, opacity: f32): i32
    @lib("SDL2") fn SDL_GetWindowOpacity(window: *SDL_Window, out: *FloatOut): i32

    // ─── show / hide / focus / minimise / maximise ───────────────
    @lib("SDL2") fn SDL_ShowWindow(window: *SDL_Window)
    @lib("SDL2") fn SDL_HideWindow(window: *SDL_Window)
    @lib("SDL2") fn SDL_RaiseWindow(window: *SDL_Window)
    @lib("SDL2") fn SDL_MaximizeWindow(window: *SDL_Window)
    @lib("SDL2") fn SDL_MinimizeWindow(window: *SDL_Window)
    @lib("SDL2") fn SDL_RestoreWindow(window: *SDL_Window)
    @lib("SDL2") fn SDL_SetWindowInputFocus(window: *SDL_Window): i32
    @lib("SDL2") fn SDL_FlashWindow(window: *SDL_Window, operation: i32): i32

    // ─── grab / capture ──────────────────────────────────────────
    @lib("SDL2") fn SDL_SetWindowGrab(window: *SDL_Window, grabbed: i32)
    @lib("SDL2") fn SDL_GetWindowGrab(window: *SDL_Window): i32
    @lib("SDL2") fn SDL_SetWindowKeyboardGrab(window: *SDL_Window, grabbed: i32)
    @lib("SDL2") fn SDL_GetWindowKeyboardGrab(window: *SDL_Window): i32
    @lib("SDL2") fn SDL_SetWindowMouseGrab(window: *SDL_Window, grabbed: i32)
    @lib("SDL2") fn SDL_GetWindowMouseGrab(window: *SDL_Window): i32
    @lib("SDL2") fn SDL_GetGrabbedWindow(): *SDL_Window
    @lib("SDL2") fn SDL_SetWindowMouseRect(window: *SDL_Window, rect: *WindowRect): i32
    @lib("SDL2") fn SDL_GetWindowMouseRect(window: *SDL_Window): *WindowRect
    @lib("SDL2") fn SDL_CaptureMouse(enabled: i32): i32

    // ─── modal / parent ──────────────────────────────────────────
    @lib("SDL2") fn SDL_SetWindowModalFor(modal: *SDL_Window, parent: *SDL_Window): i32

    // ─── display info ────────────────────────────────────────────
    @lib("SDL2") fn SDL_GetWindowDisplayIndex(window: *SDL_Window): i32
    @lib("SDL2") fn SDL_SetWindowDisplayMode(window: *SDL_Window, mode: *SDL_DisplayMode): i32
    @lib("SDL2") fn SDL_GetWindowDisplayMode(window: *SDL_Window, mode: *SDL_DisplayMode): i32
    @lib("SDL2") fn SDL_GetNumVideoDisplays(): i32
    @lib("SDL2") fn SDL_GetDisplayName(displayIndex: i32): *const char
    @lib("SDL2") fn SDL_GetDisplayBounds(displayIndex: i32, rect: *WindowRect): i32
    @lib("SDL2") fn SDL_GetDisplayUsableBounds(displayIndex: i32, rect: *WindowRect): i32
    @lib("SDL2") fn SDL_GetDisplayDPI(
        displayIndex: i32, ddpi: *FloatOut, hdpi: *FloatOut, vdpi: *FloatOut
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
        window: *SDL_Window, rects: *WindowRect, numrects: i32
    ): i32

    // ─── OpenGL ──────────────────────────────────────────────────
    @lib("SDL2") fn SDL_GL_LoadLibrary(path: *const char): i32
    @lib("SDL2") fn SDL_GL_UnloadLibrary()
    @lib("SDL2") fn SDL_GL_ExtensionSupported(extension: *const char): i32
    @lib("SDL2") fn SDL_GL_ResetAttributes()
    @lib("SDL2") fn SDL_GL_SetAttribute(attr: i32, value: i32): i32
    @lib("SDL2") fn SDL_GL_GetAttribute(attr: i32, value: *IntOut): i32
    @lib("SDL2") fn SDL_GL_CreateContext(window: *SDL_Window): *SDL_GLContext
    @lib("SDL2") fn SDL_GL_MakeCurrent(window: *SDL_Window, context: *SDL_GLContext): i32
    @lib("SDL2") fn SDL_GL_GetCurrentWindow(): *SDL_Window
    @lib("SDL2") fn SDL_GL_GetCurrentContext(): *SDL_GLContext
    @lib("SDL2") fn SDL_GL_GetDrawableSize(window: *SDL_Window, w: *IntOut, h: *IntOut)
    @lib("SDL2") fn SDL_GL_SetSwapInterval(interval: i32): i32
    @lib("SDL2") fn SDL_GL_GetSwapInterval(): i32
    @lib("SDL2") fn SDL_GL_SwapWindow(window: *SDL_Window)
    @lib("SDL2") fn SDL_GL_DeleteContext(context: *SDL_GLContext)

    // ─── public ilang return types (no `init`) ───────────────────
    class Point {
        x: i32
        y: i32
    }

    class Size {
        w: i32
        h: i32
    }

    class Rect {
        x: i32
        y: i32
        w: i32
        h: i32
    }

    class DisplayMode {
        format: u32
        w: i32
        h: i32
        refresh_rate: i32
    }

    class DisplayDPI {
        diagonal: f32
        horizontal: f32
        vertical: f32
    }

    class WindowBorders {
        top: i32
        left: i32
        bottom: i32
        right: i32
    }

    // ─── friendly Window class (ARC-managed) ────────────────────
    class Window {
        handle: i64

        // Convenience: centred (well, undefined) on the default
        // display, no flags.
        init(title: string, w: i32, h: i32) {
            let flags: u32 = 0
            let win = SDL_CreateWindow(
                cstrFromString(title),
                WINDOWPOS_UNDEFINED, WINDOWPOS_UNDEFINED,
                w, h, flags
            )
            this.handle = win as i64
        }

        // Full SDL_CreateWindow surface — caller controls the
        // initial position and the WINDOW_* flag bitmask.
        init(title: string, x: i32, y: i32, w: i32, h: i32, flags: u32) {
            let win = SDL_CreateWindow(
                cstrFromString(title), x, y, w, h, flags
            )
            this.handle = win as i64
        }

        deinit() {
            if this.handle != 0 {
                SDL_DestroyWindow(this.handle as *SDL_Window)
            }
        }

        // ─── identity / state ────────────────────────────────────
        get id(): u32 {
            SDL_GetWindowID(this.handle as *SDL_Window)
        }
        get flags(): u32 {
            SDL_GetWindowFlags(this.handle as *SDL_Window)
        }
        get pixelFormat(): u32 {
            SDL_GetWindowPixelFormat(this.handle as *SDL_Window)
        }
        get displayIndex(): i32 {
            SDL_GetWindowDisplayIndex(this.handle as *SDL_Window)
        }

        // ─── title ───────────────────────────────────────────────
        get title(): string {
            stringFromCstr(SDL_GetWindowTitle(this.handle as *SDL_Window))
        }
        set title(t: string) {
            SDL_SetWindowTitle(this.handle as *SDL_Window, cstrFromString(t))
        }

        // ─── geometry ────────────────────────────────────────────
        get position(): Point {
            let xx = new IntOut()
            let yy = new IntOut()
            SDL_GetWindowPosition(this.handle as *SDL_Window, xx, yy)
            let p = new Point()
            p.x = xx.v
            p.y = yy.v
            p
        }
        setPosition(x: i32, y: i32) {
            SDL_SetWindowPosition(this.handle as *SDL_Window, x, y)
        }
        get size(): Size {
            let ww = new IntOut()
            let hh = new IntOut()
            SDL_GetWindowSize(this.handle as *SDL_Window, ww, hh)
            let s = new Size()
            s.w = ww.v
            s.h = hh.v
            s
        }
        setSize(w: i32, h: i32) {
            SDL_SetWindowSize(this.handle as *SDL_Window, w, h)
        }
        get sizeInPixels(): Size {
            let ww = new IntOut()
            let hh = new IntOut()
            SDL_GetWindowSizeInPixels(this.handle as *SDL_Window, ww, hh)
            let s = new Size()
            s.w = ww.v
            s.h = hh.v
            s
        }
        get minimumSize(): Size {
            let ww = new IntOut()
            let hh = new IntOut()
            SDL_GetWindowMinimumSize(this.handle as *SDL_Window, ww, hh)
            let s = new Size()
            s.w = ww.v
            s.h = hh.v
            s
        }
        setMinimumSize(w: i32, h: i32) {
            SDL_SetWindowMinimumSize(this.handle as *SDL_Window, w, h)
        }
        get maximumSize(): Size {
            let ww = new IntOut()
            let hh = new IntOut()
            SDL_GetWindowMaximumSize(this.handle as *SDL_Window, ww, hh)
            let s = new Size()
            s.w = ww.v
            s.h = hh.v
            s
        }
        setMaximumSize(w: i32, h: i32) {
            SDL_SetWindowMaximumSize(this.handle as *SDL_Window, w, h)
        }
        get borders(): WindowBorders {
            let t = new IntOut()
            let l = new IntOut()
            let b = new IntOut()
            let r = new IntOut()
            SDL_GetWindowBordersSize(this.handle as *SDL_Window, t, l, b, r)
            let wb = new WindowBorders()
            wb.top = t.v
            wb.left = l.v
            wb.bottom = b.v
            wb.right = r.v
            wb
        }

        // ─── decorations (write-only properties) ─────────────────
        set bordered(on: bool) {
            SDL_SetWindowBordered(
                this.handle as *SDL_Window, if on { 1 } else { 0 }
            )
        }
        set resizable(on: bool) {
            SDL_SetWindowResizable(
                this.handle as *SDL_Window, if on { 1 } else { 0 }
            )
        }
        set alwaysOnTop(on: bool) {
            SDL_SetWindowAlwaysOnTop(
                this.handle as *SDL_Window, if on { 1 } else { 0 }
            )
        }

        // ─── fullscreen / opacity ────────────────────────────────
        // Setters drop SDL's i32 status. If you need to inspect it,
        // call SDL_SetWindowFullscreen / SDL_SetWindowOpacity
        // directly through their @lib decls.
        set fullscreen(on: bool) {
            let flag: u32 = if on { 0x1001 as u32 } else { 0 as u32 }
            SDL_SetWindowFullscreen(this.handle as *SDL_Window, flag)
            return
        }
        // Raw fullscreen flag (e.g. SDL_WINDOW_FULLSCREEN exclusive
        // mode vs SDL_WINDOW_FULLSCREEN_DESKTOP). The boolean
        // `set fullscreen` above uses _DESKTOP.
        set fullscreenFlag(flag: u32) {
            SDL_SetWindowFullscreen(this.handle as *SDL_Window, flag)
            return
        }
        get opacity(): f32 {
            let o = new FloatOut()
            SDL_GetWindowOpacity(this.handle as *SDL_Window, o)
            o.v
        }
        set opacity(o: f32) {
            SDL_SetWindowOpacity(this.handle as *SDL_Window, o)
            return
        }

        // ─── visibility / focus ──────────────────────────────────
        show() { SDL_ShowWindow(this.handle as *SDL_Window) }
        hide() { SDL_HideWindow(this.handle as *SDL_Window) }
        raise() { SDL_RaiseWindow(this.handle as *SDL_Window) }
        maximize() { SDL_MaximizeWindow(this.handle as *SDL_Window) }
        minimize() { SDL_MinimizeWindow(this.handle as *SDL_Window) }
        restore() { SDL_RestoreWindow(this.handle as *SDL_Window) }
        setInputFocus(): i32 {
            SDL_SetWindowInputFocus(this.handle as *SDL_Window)
        }
        flash(operation: i32): i32 {
            SDL_FlashWindow(this.handle as *SDL_Window, operation)
        }

        // ─── grab ────────────────────────────────────────────────
        get grab(): bool {
            SDL_GetWindowGrab(this.handle as *SDL_Window) != 0
        }
        set grab(on: bool) {
            SDL_SetWindowGrab(
                this.handle as *SDL_Window, if on { 1 } else { 0 }
            )
        }
        get keyboardGrab(): bool {
            SDL_GetWindowKeyboardGrab(this.handle as *SDL_Window) != 0
        }
        set keyboardGrab(on: bool) {
            SDL_SetWindowKeyboardGrab(
                this.handle as *SDL_Window, if on { 1 } else { 0 }
            )
        }
        get mouseGrab(): bool {
            SDL_GetWindowMouseGrab(this.handle as *SDL_Window) != 0
        }
        set mouseGrab(on: bool) {
            SDL_SetWindowMouseGrab(
                this.handle as *SDL_Window, if on { 1 } else { 0 }
            )
        }

        // ─── display mode ────────────────────────────────────────
        get currentDisplayMode(): DisplayMode {
            let raw = new SDL_DisplayMode()
            SDL_GetWindowDisplayMode(this.handle as *SDL_Window, raw)
            let m = new DisplayMode()
            m.format = raw.format
            m.w = raw.w
            m.h = raw.h
            m.refresh_rate = raw.refresh_rate
            m
        }
    }

    // ─── module-level video / display fns ────────────────────────

    fn numVideoDisplays(): i32 {
        SDL_GetNumVideoDisplays()
    }
    fn displayName(displayIndex: i32): string {
        stringFromCstr(SDL_GetDisplayName(displayIndex))
    }
    fn displayBounds(displayIndex: i32): Rect {
        let raw = new WindowRect()
        SDL_GetDisplayBounds(displayIndex, raw)
        let r = new Rect()
        r.x = raw.x
        r.y = raw.y
        r.w = raw.w
        r.h = raw.h
        r
    }
    fn displayUsableBounds(displayIndex: i32): Rect {
        let raw = new WindowRect()
        SDL_GetDisplayUsableBounds(displayIndex, raw)
        let r = new Rect()
        r.x = raw.x
        r.y = raw.y
        r.w = raw.w
        r.h = raw.h
        r
    }
    fn displayDPI(displayIndex: i32): DisplayDPI {
        let dd = new FloatOut()
        let hd = new FloatOut()
        let vd = new FloatOut()
        SDL_GetDisplayDPI(displayIndex, dd, hd, vd)
        let d = new DisplayDPI()
        d.diagonal = dd.v
        d.horizontal = hd.v
        d.vertical = vd.v
        d
    }
    fn displayOrientation(displayIndex: i32): i32 {
        SDL_GetDisplayOrientation(displayIndex)
    }
    fn numDisplayModes(displayIndex: i32): i32 {
        SDL_GetNumDisplayModes(displayIndex)
    }
    fn displayMode(displayIndex: i32, modeIndex: i32): DisplayMode {
        let raw = new SDL_DisplayMode()
        SDL_GetDisplayMode(displayIndex, modeIndex, raw)
        let m = new DisplayMode()
        m.format = raw.format
        m.w = raw.w
        m.h = raw.h
        m.refresh_rate = raw.refresh_rate
        m
    }
    fn desktopDisplayMode(displayIndex: i32): DisplayMode {
        let raw = new SDL_DisplayMode()
        SDL_GetDesktopDisplayMode(displayIndex, raw)
        let m = new DisplayMode()
        m.format = raw.format
        m.w = raw.w
        m.h = raw.h
        m.refresh_rate = raw.refresh_rate
        m
    }
    fn currentDisplayMode(displayIndex: i32): DisplayMode {
        let raw = new SDL_DisplayMode()
        SDL_GetCurrentDisplayMode(displayIndex, raw)
        let m = new DisplayMode()
        m.format = raw.format
        m.w = raw.w
        m.h = raw.h
        m.refresh_rate = raw.refresh_rate
        m
    }

    fn videoInit(driver: string): i32 {
        SDL_VideoInit(cstrFromString(driver))
    }
    fn videoQuit() {
        SDL_VideoQuit()
    }
    fn currentVideoDriver(): string {
        stringFromCstr(SDL_GetCurrentVideoDriver())
    }
    fn videoDriver(index: i32): string {
        stringFromCstr(SDL_GetVideoDriver(index))
    }
    fn numVideoDrivers(): i32 {
        SDL_GetNumVideoDrivers()
    }

    fn enableScreenSaver() {
        SDL_EnableScreenSaver()
    }
    fn disableScreenSaver() {
        SDL_DisableScreenSaver()
    }
    fn isScreenSaverEnabled(): bool {
        SDL_IsScreenSaverEnabled() != 0
    }

    // ─── OpenGL helpers ──────────────────────────────────────────
    fn glLoadLibrary(path: string): i32 {
        SDL_GL_LoadLibrary(cstrFromString(path))
    }
    fn glUnloadLibrary() {
        SDL_GL_UnloadLibrary()
    }
    fn glExtensionSupported(name: string): bool {
        SDL_GL_ExtensionSupported(cstrFromString(name)) != 0
    }
    fn glResetAttributes() {
        SDL_GL_ResetAttributes()
    }
    fn glSetAttribute(attr: i32, value: i32): i32 {
        SDL_GL_SetAttribute(attr, value)
    }
    fn glGetAttribute(attr: i32): i32 {
        let buf = new IntOut()
        SDL_GL_GetAttribute(attr, buf)
        buf.v
    }
    fn glSetSwapInterval(interval: i32): i32 {
        SDL_GL_SetSwapInterval(interval)
    }
    fn glGetSwapInterval(): i32 {
        SDL_GL_GetSwapInterval()
    }
}

// ─── SDL_WindowFlags ──────────────────────────────────────────────
const WINDOW_FULLSCREEN: u32         = 0x00000001
const WINDOW_OPENGL: u32             = 0x00000002
const WINDOW_SHOWN: u32              = 0x00000004
const WINDOW_HIDDEN: u32             = 0x00000008
const WINDOW_BORDERLESS: u32         = 0x00000010
const WINDOW_RESIZABLE: u32          = 0x00000020
const WINDOW_MINIMIZED: u32          = 0x00000040
const WINDOW_MAXIMIZED: u32          = 0x00000080
const WINDOW_MOUSE_GRABBED: u32      = 0x00000100
const WINDOW_INPUT_FOCUS: u32        = 0x00000200
const WINDOW_MOUSE_FOCUS: u32        = 0x00000400
const WINDOW_FULLSCREEN_DESKTOP: u32 = 0x00001001
const WINDOW_FOREIGN: u32            = 0x00000800
const WINDOW_ALLOW_HIGHDPI: u32      = 0x00002000
const WINDOW_MOUSE_CAPTURE: u32      = 0x00004000
const WINDOW_ALWAYS_ON_TOP: u32      = 0x00008000
const WINDOW_SKIP_TASKBAR: u32       = 0x00010000
const WINDOW_UTILITY: u32            = 0x00020000
const WINDOW_TOOLTIP: u32            = 0x00040000
const WINDOW_POPUP_MENU: u32         = 0x00080000
const WINDOW_KEYBOARD_GRABBED: u32   = 0x00100000
const WINDOW_VULKAN: u32             = 0x10000000
const WINDOW_METAL: u32              = 0x20000000

// ─── SDL_WindowEventID (sub-events of SDL_WINDOWEVENT) ────────────
const WINDOWEVENT_NONE: i32             = 0
const WINDOWEVENT_SHOWN: i32            = 1
const WINDOWEVENT_HIDDEN: i32           = 2
const WINDOWEVENT_EXPOSED: i32          = 3
const WINDOWEVENT_MOVED: i32            = 4
const WINDOWEVENT_RESIZED: i32          = 5
const WINDOWEVENT_SIZE_CHANGED: i32     = 6
const WINDOWEVENT_MINIMIZED: i32        = 7
const WINDOWEVENT_MAXIMIZED: i32        = 8
const WINDOWEVENT_RESTORED: i32         = 9
const WINDOWEVENT_ENTER: i32            = 10
const WINDOWEVENT_LEAVE: i32            = 11
const WINDOWEVENT_FOCUS_GAINED: i32     = 12
const WINDOWEVENT_FOCUS_LOST: i32       = 13
const WINDOWEVENT_CLOSE: i32            = 14
const WINDOWEVENT_TAKE_FOCUS: i32       = 15
const WINDOWEVENT_HIT_TEST: i32         = 16
const WINDOWEVENT_ICCPROF_CHANGED: i32  = 17
const WINDOWEVENT_DISPLAY_CHANGED: i32  = 18

// ─── SDL_WINDOWPOS_* helpers ──────────────────────────────────────
const WINDOWPOS_UNDEFINED: i32 = 0x1FFF0000
const WINDOWPOS_CENTERED: i32  = 0x2FFF0000

// ─── SDL_DisplayOrientation ───────────────────────────────────────
const ORIENTATION_UNKNOWN: i32           = 0
const ORIENTATION_LANDSCAPE: i32         = 1
const ORIENTATION_LANDSCAPE_FLIPPED: i32 = 2
const ORIENTATION_PORTRAIT: i32          = 3
const ORIENTATION_PORTRAIT_FLIPPED: i32  = 4

// ─── SDL_FlashOperation ───────────────────────────────────────────
const FLASH_CANCEL: i32        = 0
const FLASH_BRIEFLY: i32       = 1
const FLASH_UNTIL_FOCUSED: i32 = 2

// ─── SDL_GLattr — values for `glSetAttribute` / `glGetAttribute` ─
const GL_RED_SIZE: i32                    = 0
const GL_GREEN_SIZE: i32                  = 1
const GL_BLUE_SIZE: i32                   = 2
const GL_ALPHA_SIZE: i32                  = 3
const GL_BUFFER_SIZE: i32                 = 4
const GL_DOUBLEBUFFER: i32                = 5
const GL_DEPTH_SIZE: i32                  = 6
const GL_STENCIL_SIZE: i32                = 7
const GL_ACCUM_RED_SIZE: i32              = 8
const GL_ACCUM_GREEN_SIZE: i32            = 9
const GL_ACCUM_BLUE_SIZE: i32             = 10
const GL_ACCUM_ALPHA_SIZE: i32            = 11
const GL_STEREO: i32                      = 12
const GL_MULTISAMPLEBUFFERS: i32          = 13
const GL_MULTISAMPLESAMPLES: i32          = 14
const GL_ACCELERATED_VISUAL: i32          = 15
const GL_RETAINED_BACKING: i32            = 16
const GL_CONTEXT_MAJOR_VERSION: i32       = 17
const GL_CONTEXT_MINOR_VERSION: i32       = 18
const GL_CONTEXT_EGL: i32                 = 19
const GL_CONTEXT_FLAGS: i32               = 20
const GL_CONTEXT_PROFILE_MASK: i32        = 21
const GL_SHARE_WITH_CURRENT_CONTEXT: i32  = 22
const GL_FRAMEBUFFER_SRGB_CAPABLE: i32    = 23
const GL_CONTEXT_RELEASE_BEHAVIOR: i32    = 24
const GL_CONTEXT_RESET_NOTIFICATION: i32  = 25
const GL_CONTEXT_NO_ERROR: i32            = 26
const GL_FLOATBUFFERS: i32                = 27

// SDL_GLcontextFlag (bit-flags for GL_CONTEXT_FLAGS)
const GL_CONTEXT_DEBUG_FLAG: i32              = 0x0001
const GL_CONTEXT_FORWARD_COMPATIBLE_FLAG: i32 = 0x0002
const GL_CONTEXT_ROBUST_ACCESS_FLAG: i32      = 0x0004
const GL_CONTEXT_RESET_ISOLATION_FLAG: i32    = 0x0008

// SDL_GLprofile (values for GL_CONTEXT_PROFILE_MASK)
const GL_CONTEXT_PROFILE_CORE: i32          = 0x0001
const GL_CONTEXT_PROFILE_COMPATIBILITY: i32 = 0x0002
const GL_CONTEXT_PROFILE_ES: i32            = 0x0004
