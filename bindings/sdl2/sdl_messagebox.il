/// SDL2 message-box dialogs. Mirrors SDL_messagebox.h. The simple
/// form (`showSimpleMessageBox`) covers 90 % of real-world uses
/// (one-shot info / warning / error popups). The richer
/// `SDL_ShowMessageBox` is exposed raw for callers that need
/// custom buttons / colour schemes — building the
/// `SDL_MessageBoxData` structure is fiddly so we don't wrap it.

use sdl_window { Window }
use std.ffi { cstrFromString }

/// `SDL_MessageBoxFlags` — visual style of the dialog. Use these
/// with `showSimpleMessageBox`. SDL ignores the flag on platforms
/// whose native dialog has no equivalent (e.g. KDE simple
/// message box).
pub enum MessageBoxFlag: u32 {
    error              = 0x00000010
    warning            = 0x00000020
    information        = 0x00000040
    buttonsLeftToRight = 0x00000080
    buttonsRightToLeft = 0x00000100
}

@extern(C, "SDL2") {
    /// Raw SDL2 binding. Most callers want
    /// `showSimpleMessageBox(...)` (Window-typed) instead — this
    /// stays public so power users can pass a raw window pointer
    /// (`0` for "system-modal, no parent").
    @lib pub fn SDL_ShowSimpleMessageBox(
        flags: u32, title: *const char, message: *const char,
        window: *void
    ): i32

    /// Show a single-button modal dialog and block until it
    /// closes.
    ///
    /// **Args:**
    /// - `flags` — `MessageBoxFlag.*` (info / warning / error icon).
    /// - `title` — dialog title bar text.
    /// - `message` — dialog body text.
    /// - `parent` — when `some(window)`, the dialog is modal to
    ///   that window; when `none`, the dialog is
    ///   system-modal with no parent.
    ///
    /// **Returns:**
    ///   `0` on success, `< 0` on failure (call `sdl.getError()`).
    pub fn showSimpleMessageBox(
        flags: MessageBoxFlag, title: string, message: string,
        parent: Window?
    ): i32 {
        let h: i64 = if let some(w) = parent { w.handle } else { 0 }
        SDL_ShowSimpleMessageBox(
            flags as u32,
            cstrFromString(title),
            cstrFromString(message),
            h as *void
        )
    }

    /// Convenience: error-icon dialog (no parent window).
    pub fn showError(title: string, message: string): i32 {
        SDL_ShowSimpleMessageBox(
            MessageBoxFlag.error as u32,
            cstrFromString(title),
            cstrFromString(message),
            0 as *void
        )
    }

    /// Convenience: warning-icon dialog (no parent window).
    pub fn showWarning(title: string, message: string): i32 {
        SDL_ShowSimpleMessageBox(
            MessageBoxFlag.warning as u32,
            cstrFromString(title),
            cstrFromString(message),
            0 as *void
        )
    }

    /// Convenience: info-icon dialog (no parent window).
    pub fn showInfo(title: string, message: string): i32 {
        SDL_ShowSimpleMessageBox(
            MessageBoxFlag.information as u32,
            cstrFromString(title),
            cstrFromString(message),
            0 as *void
        )
    }
}
