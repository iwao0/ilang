// SDL2 mouse: cursor position / button state, cursor visibility,
// custom cursor creation, relative-mode capture. Mirrors
// SDL_mouse.h. Initialise SDL with `InitFlag.video` (events ride
// along automatically).

use sdl_core { Point }

/// Bitmask values returned in the high bits of `getMouseState` /
/// `getGlobalMouseState`. AND with `MouseButton.left` etc. to test.
@flags
pub enum MouseButton: u32 {
    left   = 0x01   // SDL_BUTTON_LMASK
    middle = 0x02   // SDL_BUTTON_MMASK
    right  = 0x04   // SDL_BUTTON_RMASK
    x1     = 0x08   // SDL_BUTTON_X1MASK
    x2     = 0x10   // SDL_BUTTON_X2MASK
}

/// Built-in system cursors. Pass to `createSystemCursor`.
pub enum SystemCursor: i32 {
    arrow      = 0
    iBeam      = 1
    wait       = 2
    crosshair  = 3
    waitArrow  = 4
    sizeNWSE   = 5
    sizeNESW   = 6
    sizeWE     = 7
    sizeNS     = 8
    sizeAll    = 9
    no         = 10
    hand       = 11
}

@extern(C) {
    /// Opaque SDL_Cursor — created by one of the `createCursor*`
    /// fns, freed by `freeCursor`.
    pub struct SDL_Cursor {}

    @lib("SDL2") pub fn SDL_GetMouseState(x: *i32, y: *i32): u32
    @lib("SDL2") pub fn SDL_GetGlobalMouseState(x: *i32, y: *i32): u32
    @lib("SDL2") pub fn SDL_GetRelativeMouseState(x: *i32, y: *i32): u32
    @lib("SDL2") pub fn SDL_WarpMouseInWindow(window: *void, x: i32, y: i32)
    @lib("SDL2") pub fn SDL_WarpMouseGlobal(x: i32, y: i32): i32
    @lib("SDL2") pub fn SDL_SetRelativeMouseMode(enabled: i32): i32
    @lib("SDL2") pub fn SDL_GetRelativeMouseMode(): i32
    @lib("SDL2") pub fn SDL_ShowCursor(toggle: i32): i32
    @lib("SDL2") pub fn SDL_GetMouseFocus(): *void

    // ─── cursor lifecycle ────────────────────────────────────────
    @lib("SDL2") pub fn SDL_CreateSystemCursor(id: i32): *SDL_Cursor
    @lib("SDL2") pub fn SDL_CreateCursor(
        data: *const u8, mask: *const u8,
        w: i32, h: i32, hot_x: i32, hot_y: i32
    ): *SDL_Cursor
    @lib("SDL2") pub fn SDL_CreateColorCursor(
        surface: *void, hot_x: i32, hot_y: i32
    ): *SDL_Cursor
    @lib("SDL2") pub fn SDL_SetCursor(cursor: *SDL_Cursor)
    @lib("SDL2") pub fn SDL_GetCursor(): *SDL_Cursor
    @lib("SDL2") pub fn SDL_GetDefaultCursor(): *SDL_Cursor
    @lib("SDL2") pub fn SDL_FreeCursor(cursor: *SDL_Cursor)

    // ─── friendlier ilang wrappers ───────────────────────────────

    /// Snapshot of the mouse pointer's window-local position and
    /// pressed-button bitmask.
    ///
    /// Returns:
    ///   `Point` filled with `(x, y)` in window coordinates. Combine
    ///   with `mouseButtons()` to read the buttons in the same call.
    pub fn getMousePosition(): Point {
        let xs: i32[] = [0]
        let ys: i32[] = [0]
        let px: *i32 = xs
        let py: *i32 = ys
        SDL_GetMouseState(px, py)
        let p = new Point()
        p.x = xs[0]
        p.y = ys[0]
        p
    }

    /// Read the held-button bitmask (no position). AND the result
    /// against `MouseButton.left` etc. to test.
    pub fn mouseButtons(): u32 {
        let xs: i32[] = [0]
        let ys: i32[] = [0]
        let px: *i32 = xs
        let py: *i32 = ys
        SDL_GetMouseState(px, py)
    }

    /// Same as `getMousePosition`, but coordinates are relative to
    /// the desktop, not any specific window.
    pub fn getGlobalMousePosition(): Point {
        let xs: i32[] = [0]
        let ys: i32[] = [0]
        let px: *i32 = xs
        let py: *i32 = ys
        SDL_GetGlobalMouseState(px, py)
        let p = new Point()
        p.x = xs[0]
        p.y = ys[0]
        p
    }

    /// `(dx, dy)` since the previous call, accumulating motion that
    /// happened between SDL event-pump cycles. The first call after
    /// `init` returns garbage for `dx` / `dy` — discard it.
    pub fn getRelativeMousePosition(): Point {
        let xs: i32[] = [0]
        let ys: i32[] = [0]
        let px: *i32 = xs
        let py: *i32 = ys
        SDL_GetRelativeMouseState(px, py)
        let p = new Point()
        p.x = xs[0]
        p.y = ys[0]
        p
    }

    /// Move the mouse cursor to `(x, y)` inside the window's client
    /// area.
    pub fn warpMouseInWindow(window: i64, x: i32, y: i32) {
        SDL_WarpMouseInWindow(window as *void, x, y)
    }

    /// Move the cursor in screen coordinates.
    ///
    /// Returns:
    ///   `0` on success, `< 0` on failure (some platforms forbid
    ///   global warps).
    pub fn warpMouseGlobal(x: i32, y: i32): i32 {
        SDL_WarpMouseGlobal(x, y)
    }

    /// Toggle relative mouse mode — hides the cursor, locks it to
    /// the window, and `getRelativeMouseState` reports raw deltas.
    /// Used for first-person cameras / drag interactions.
    ///
    /// Returns:
    ///   `0` on success, `< 0` if not supported by the platform.
    pub fn setRelativeMouseMode(enabled: bool): i32 {
        let v: i32 = if enabled { 1 } else { 0 }
        SDL_SetRelativeMouseMode(v)
    }

    /// Returns:
    ///   true when relative mode is active.
    pub fn getRelativeMouseMode(): bool {
        SDL_GetRelativeMouseMode() != 0
    }

    /// Show or hide the system cursor.
    ///
    /// Args:
    ///   visible — true shows, false hides.
    ///
    /// Returns:
    ///   the resulting visibility (`1` shown, `0` hidden), or
    ///   `< 0` on error.
    pub fn showCursor(visible: bool): i32 {
        let v: i32 = if visible { 1 } else { 0 }
        SDL_ShowCursor(v)
    }

    /// Returns:
    ///   the current visibility (`1` shown, `0` hidden) without
    ///   changing it.
    pub fn cursorVisible(): bool {
        SDL_ShowCursor(-1) == 1
    }

    /// Returns:
    ///   pointer (cast to i64) to the window currently under the
    ///   mouse, or `0` when the mouse is outside every SDL window.
    pub fn getMouseFocus(): i64 {
        SDL_GetMouseFocus() as i64
    }

    /// ARC-managed `SDL_Cursor`. Construct via `Cursor.system(...)`
    /// or `Cursor.bitmap(...)`. `deinit` frees the underlying
    /// resource (skips the default cursor — SDL owns that one).
    pub class Cursor {
        /// Raw `SDL_Cursor*` cast to i64.
        pub handle: i64
        /// True when the cursor is one of the system / bitmap /
        /// color variants we own and must `SDL_FreeCursor` on
        /// deinit. False for `Cursor.default()`, which borrows
        /// SDL's static handle.
        pub owned: bool

        /// Wrap one of SDL's stock cursors (arrow, IBeam, hand, …).
        /// SDL caches these so multiple instances of the same
        /// `SystemCursor` are still safe to construct.
        pub init(id: SystemCursor) {
            this.handle = SDL_CreateSystemCursor(id as i32) as i64
            this.owned = true
        }

        pub deinit() {
            if this.owned && this.handle != 0 {
                SDL_FreeCursor(this.handle as *SDL_Cursor)
            }
        }

        /// Make this cursor active on every SDL window.
        pub activate() {
            if this.handle != 0 {
                SDL_SetCursor(this.handle as *SDL_Cursor)
            }
        }
    }

    /// Restore the OS default cursor on every SDL window.
    pub fn setDefaultCursor() {
        SDL_SetCursor(SDL_GetDefaultCursor())
    }
}
