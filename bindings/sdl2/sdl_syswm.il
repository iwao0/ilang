// SDL2 platform-specific window info — `SDL_GetWindowWMInfo`. Use
// when you need the underlying native window / display handle for
// interop with another graphics or windowing toolkit (Vulkan
// surface creation, dear-imgui platform backend, native menus,
// etc.).
//
// SDL's `SDL_SysWMinfo` is a tagged union whose layout differs
// per build platform. We model it as a fixed-size struct: the
// 8-byte header (version + subsystem) followed by 4 i64 handle
// slots that absorb the natural pointer-shaped variants
// (cocoa / win32 / x11 / wayland), plus a 224-byte tail for
// forward-compat with newer SDL versions.
//
// Callers don't read `handle1..4` directly — use the per-platform
// accessors (`getCocoaWindow`, `getWin32Hwnd`, …) which guard
// the read on the recorded `subsystem`.

use sdl_window { SDL_Window }

/// `SDL_SYSWM_TYPE` — which platform / windowing system the
/// `info` payload was filled in for. Read back from
/// `SDL_SysWMinfo.subsystem` after `getWindowWMInfo`.
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
    pub struct SDL_SysWMinfo {
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
        window: *SDL_Window, info: *SDL_SysWMinfo
    ): u8

    // ─── friendlier ilang wrappers ───────────────────────────────

    /// Pull the native window-manager info for `windowHandle`.
    /// Initialises the `version` field with `2.0.0` so SDL agrees
    /// to populate the rest.
    ///
    /// Args:
    ///   windowHandle — `*SDL_Window` cast to i64.
    ///
    /// Returns:
    ///   a freshly populated `SDL_SysWMinfo`, or one with
    ///   `subsystem == SubSystem.unknown` if the call failed
    ///   (typically because `windowHandle` is `0` or SDL's
    ///   binary-compat check rejected our version).
    pub fn getWindowWMInfo(windowHandle: i64): SDL_SysWMinfo {
        let info = new SDL_SysWMinfo()
        if windowHandle == 0 {
            return info
        }
        // SDL's compat gate accepts any major == 2 build; the
        // minor / patch are informational. Use 2.0.0 as a stable
        // floor that every SDL2 release honours.
        info.versionMajor = 2 as u8
        info.versionMinor = 0 as u8
        info.versionPatch = 0 as u8
        let ok = SDL_GetWindowWMInfo(windowHandle as *SDL_Window, info)
        if ok == 0 as u8 {
            // Reset subsystem so callers see `unknown` rather than
            // whatever stale bytes happened to be in `info`.
            info.subsystem = SubSystem.unknown as u32
        }
        info
    }

    /// macOS Cocoa: return the underlying `NSWindow *` cast to
    /// i64. `0` when `info` isn't a Cocoa window.
    pub fn getCocoaWindow(info: SDL_SysWMinfo): i64 {
        if info.subsystem != SubSystem.cocoa as u32 { return 0 }
        info.handle1
    }

    /// Windows: native `HWND` cast to i64.
    pub fn getWin32Hwnd(info: SDL_SysWMinfo): i64 {
        if info.subsystem != SubSystem.windows as u32 { return 0 }
        info.handle1
    }

    /// Windows: `HDC` cast to i64.
    pub fn getWin32Hdc(info: SDL_SysWMinfo): i64 {
        if info.subsystem != SubSystem.windows as u32 { return 0 }
        info.handle2
    }

    /// Windows: `HINSTANCE` cast to i64.
    pub fn getWin32Hinstance(info: SDL_SysWMinfo): i64 {
        if info.subsystem != SubSystem.windows as u32 { return 0 }
        info.handle3
    }

    /// X11: `Display *` cast to i64.
    pub fn getX11Display(info: SDL_SysWMinfo): i64 {
        if info.subsystem != SubSystem.x11 as u32 { return 0 }
        info.handle1
    }

    /// X11: `Window` (an unsigned long ID) cast to i64.
    pub fn getX11Window(info: SDL_SysWMinfo): i64 {
        if info.subsystem != SubSystem.x11 as u32 { return 0 }
        info.handle2
    }

    /// Wayland: `wl_display *` cast to i64.
    pub fn getWaylandDisplay(info: SDL_SysWMinfo): i64 {
        if info.subsystem != SubSystem.wayland as u32 { return 0 }
        info.handle1
    }

    /// Wayland: `wl_surface *` cast to i64.
    pub fn getWaylandSurface(info: SDL_SysWMinfo): i64 {
        if info.subsystem != SubSystem.wayland as u32 { return 0 }
        info.handle2
    }

    /// Wayland: legacy `wl_shell_surface *` cast to i64. Newer
    /// compositors use xdg_surface / xdg_toplevel which live in
    /// the tail padding — extract them with `readU64` against the
    /// raw struct address if needed.
    pub fn getWaylandShellSurface(info: SDL_SysWMinfo): i64 {
        if info.subsystem != SubSystem.wayland as u32 { return 0 }
        info.handle3
    }

    /// UIKit (iOS / tvOS): `UIWindow *` cast to i64.
    pub fn getUIKitWindow(info: SDL_SysWMinfo): i64 {
        if info.subsystem != SubSystem.uikit as u32 { return 0 }
        info.handle1
    }

    /// Android: native `ANativeWindow *` cast to i64.
    pub fn getAndroidWindow(info: SDL_SysWMinfo): i64 {
        if info.subsystem != SubSystem.android as u32 { return 0 }
        info.handle1
    }
}
