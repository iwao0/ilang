/// SDL2 mouse: cursor position / button state, cursor visibility,
/// custom cursor creation, relative-mode capture. Mirrors
/// SDL_mouse.h. Initialise SDL with `InitFlag.video` (events ride
/// along automatically).

use sdl_core { Point }
use sdl_window { Window, SDL_Window }

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

@extern(C, "SDL2") {
    /// Opaque SDL_Cursor — created by one of the `createCursor*`
    /// fns, freed by `freeCursor`.
    pub struct SDL_Cursor {}

    @lib pub fn SDL_GetMouseState(x: *i32, y: *i32): u32
    @lib pub fn SDL_GetGlobalMouseState(x: *i32, y: *i32): u32
    @lib pub fn SDL_GetRelativeMouseState(x: *i32, y: *i32): u32
    @lib pub fn SDL_WarpMouseInWindow(window: *SDL_Window, x: i32, y: i32)
    @lib pub fn SDL_WarpMouseGlobal(x: i32, y: i32): i32
    @lib pub fn SDL_SetRelativeMouseMode(enabled: i32): i32
    @lib pub fn SDL_GetRelativeMouseMode(): i32
    @lib pub fn SDL_ShowCursor(toggle: i32): i32
    @lib pub fn SDL_GetMouseFocus(): *void

    // ─── cursor lifecycle ────────────────────────────────────────
    @lib pub fn SDL_CreateSystemCursor(id: i32): *SDL_Cursor
    @lib pub fn SDL_CreateCursor(
        data: *const u8, mask: *const u8,
        w: i32, h: i32, hot_x: i32, hot_y: i32
    ): *SDL_Cursor
    @lib pub fn SDL_CreateColorCursor(
        surface: *void, hot_x: i32, hot_y: i32
    ): *SDL_Cursor
    @lib pub fn SDL_SetCursor(cursor: *SDL_Cursor)
    @lib pub fn SDL_GetCursor(): *SDL_Cursor
    @lib pub fn SDL_GetDefaultCursor(): *SDL_Cursor
    @lib pub fn SDL_FreeCursor(cursor: *SDL_Cursor)

    /// Namespace class for the global mouse state. All methods are
    /// `static` — there's no per-instance mouse to construct, just
    /// a process-wide pointer maintained by SDL. Mirrors the
    /// shape of `Joystick` / `GameController`'s static helpers.
    pub class Mouse {
        /// Snapshot of the pointer's window-local position. Pair
        /// with `Mouse.buttons()` to read pressed buttons in the
        /// same poll.
        pub static position(): Point {
            let p = new Point()
            SDL_GetMouseState(&p.x, &p.y)
            p
        }

        /// The held-button bitmask (no position).
        ///
        /// **Returns:**
        ///   a `MouseButton` flag set. Use bit-and against
        ///   `MouseButton.left` / `.right` / ... to test specific
        ///   buttons, or compare against `0 as MouseButton` to
        ///   check whether any button is held.
        pub static buttons(): MouseButton {
            let xs: i32 = 0
            let ys: i32 = 0
            SDL_GetMouseState(&xs, &ys) as MouseButton
        }

        /// Like `position()`, but coordinates are relative to the
        /// desktop, not any specific window.
        pub static globalPosition(): Point {
            let p = new Point()
            SDL_GetGlobalMouseState(&p.x, &p.y)
            p
        }

        /// `(dx, dy)` since the previous call, accumulating motion
        /// that happened between SDL event-pump cycles. The first
        /// call after `sdl.init` returns garbage for `dx` / `dy` —
        /// discard it.
        pub static relativePosition(): Point {
            let p = new Point()
            SDL_GetRelativeMouseState(&p.x, &p.y)
            p
        }

        /// Move the mouse cursor to `(x, y)` inside `window`'s
        /// client area.
        pub static warpInWindow(window: Window, x: i32, y: i32) {
            if window.handle == 0 { return }
            SDL_WarpMouseInWindow(window.handle as *SDL_Window, x, y)
        }

        /// Move the cursor in screen coordinates.
        ///
        /// **Returns:**
        ///   `0` on success, `< 0` on failure (some platforms
        ///   forbid global warps).
        pub static warpGlobal(x: i32, y: i32): i32 {
            SDL_WarpMouseGlobal(x, y)
        }

        /// Toggle relative mouse mode — hides the cursor, locks
        /// it to the window, and `Mouse.relativePosition()`
        /// reports raw deltas. Used for first-person cameras /
        /// drag interactions.
        ///
        /// **Returns:**
        ///   `0` on success, `< 0` if not supported by the
        ///   platform.
        pub static setRelativeMode(enabled: bool): i32 {
            let v: i32 = if enabled { 1 } else { 0 }
            SDL_SetRelativeMouseMode(v)
        }

        /// **Returns:**
        ///   true when relative mode is active.
        pub static isRelativeMode(): bool {
            SDL_GetRelativeMouseMode() != 0
        }

        /// Show or hide the system cursor.
        ///
        /// **Args:**
        /// - `visible` — true shows, false hides.
        ///
        /// **Returns:**
        ///   the resulting visibility (`1` shown, `0` hidden), or
        ///   `< 0` on error.
        pub static showCursor(visible: bool): i32 {
            let v: i32 = if visible { 1 } else { 0 }
            SDL_ShowCursor(v)
        }

        /// **Returns:**
        ///   the current visibility (true = shown, false = hidden)
        ///   without changing it.
        pub static isCursorVisible(): bool {
            SDL_ShowCursor(-1) == 1
        }

        /// **Returns:**
        ///   the raw `*SDL_Window` (cast to i64) of the window
        ///   currently under the mouse, or `0` when the mouse is
        ///   outside every SDL window. NOT wrapped in `Window`
        ///   because SDL retains ownership — wrapping would risk
        ///   a double-free at deinit.
        pub static focus(): i64 {
            SDL_GetMouseFocus() as i64
        }
    }

    /// ARC-managed `SDL_Cursor`. Construct via `new Cursor(id)` for
    /// a system cursor; `deinit` frees the underlying resource.
    /// `Cursor.setDefault()` restores SDL's stock arrow without
    /// needing an instance.
    pub class Cursor {
        /// Raw `SDL_Cursor*` cast to i64.
        pub handle: i64
        /// True when the cursor is one of the system / bitmap /
        /// color variants we own and must `SDL_FreeCursor` on
        /// deinit. False for cursors that borrow an SDL-owned
        /// static handle.
        pub owned: bool

        /// Wrap one of SDL's stock cursors (arrow, IBeam, hand,
        /// …). SDL caches these so multiple instances of the same
        /// `SystemCursor` are still safe to construct.
        pub init(id: SystemCursor) {
            this.handle = SDL_CreateSystemCursor(id as i32) as i64
            this.owned = true
        }

        deinit() {
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

        /// Restore the OS default cursor on every SDL window.
        /// Static — no Cursor instance needed; SDL owns the
        /// default handle internally.
        pub static setDefault() {
            SDL_SetCursor(SDL_GetDefaultCursor())
        }
    }
}
