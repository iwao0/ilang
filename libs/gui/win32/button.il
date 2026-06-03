/// Push-button widget. Backed by the BUTTON window class.
///
/// `BN_CLICKED` notifications come in via `wnd_proc.guiWndProc`'s
/// WM_COMMAND arm; we expose `tryButtonClick(hwnd)` so the proc
/// doesn't have to reach into our `BUTTONS` map directly.

use windows {
    HWND, HINSTANCE, HMENU, RECT,
    WindowStyle, ShowCmd, WindowMessage, TrackMouseFlags,
    TRACKMOUSEEVENT,
    TOOLINFOW, TTS_ALWAYSTIP, TTF_IDISHWND, TTF_SUBCLASS,
    TTM_ADDTOOLW, TTM_DELTOOLW, TTM_UPDATETIPTEXTW,
    GetModuleHandleA,
    CreateWindowExW, SetWindowTextW, EnableWindow, MoveWindow, ShowWindow,
    SendMessageW, SetFocus, TrackMouseEvent,
    SetWindowSubclass, DefSubclassProc,
}
use util { applyExplorerTheme, applyGuiFont, noOpClick }
use core { windowHwnd, noOp1 }

/// `BM_SETSTYLE` — flip the button style at runtime (used to set /
/// clear `BS_DEFPUSHBUTTON`). `wParam` is the new button-style bits,
/// `lParam` is the repaint flag.
const BM_SETSTYLE: u32 = 0x00F4 as u32
/// `BS_DEFPUSHBUTTON` — heavy frame + "default" semantics. Paired
/// with our own Enter-key routing in `wnd_proc`, since this
/// process's message pump doesn't run `IsDialogMessageW`.
const BS_DEFPUSHBUTTON: u64 = 0x00000001 as u64
const BS_PUSHBUTTON:    u64 = 0x00000000 as u64
/// `BM_CLICK` — simulates a click. The control synthesises
/// `WM_LBUTTONDOWN` / `WM_LBUTTONUP` on itself, animates briefly,
/// and posts the usual `WM_COMMAND` / `BN_CLICKED` to the parent
/// — which `wnd_proc` routes back into `tryButtonClick`.
const BM_CLICK: u32 = 0x00F5 as u32
/// `BS_NOTIFY` — make the button send `BN_SETFOCUS` /
/// `BN_KILLFOCUS` to its parent via `WM_COMMAND`. Without this
/// flag the BUTTON wndproc swallows focus transitions silently.
const BS_NOTIFY: u32 = 0x00004000 as u32
/// Button notify codes for `WM_COMMAND` HIWORD(wParam).
pub const BN_SETFOCUS:  i32 = 6 as i32
pub const BN_KILLFOCUS: i32 = 7 as i32

/// Per-button state. Stored in `BUTTONS` keyed by the button's
/// HWND (`hwnd as i64`) — same key gui-core sees as the public
/// `handle: i64`, so both the public setters and the
/// wnd_proc-side sender lookup are O(1) Map gets.
class ButtonSlot {
    pub hwnd:    HWND
    pub onClick: fn(f64, f64, i32)
    pub onFocus: fn()
    pub onBlur:  fn()
    pub onMouseEnter: fn(f64, f64)
    pub onMouseLeave: fn(f64, f64)
    pub onMouseDown:  fn(f64, f64, i32)
    pub onMouseUp:    fn(f64, f64, i32)
    pub onContextMenu: fn(f64, f64, i32)
    /// Win32 has no native `WM_MOUSEENTER`; we synthesise it the
    /// first time `WM_MOUSEMOVE` arrives after a previous leave.
    /// `mouseTracking` flips to true when we've registered a
    /// `TrackMouseEvent` (so the leave message will fire) and back
    /// to false on `WM_MOUSELEAVE`.
    pub mouseTracking: bool
    pub winHandle: i64
    /// Tooltip control owned by this button. Lazy-created on the
    /// first `setButtonTooltip(_, non-empty)` call; reused for
    /// subsequent updates via `TTM_UPDATETIPTEXTW`. Stays `0` (the
    /// HWND nullary) while no tooltip is configured.
    pub tooltipHwnd: HWND
    pub init(hwnd: HWND, winHandle: i64) {
        this.hwnd = hwnd
        this.onClick = noOpClick
        this.onFocus = noOp1
        this.onBlur  = noOp1
        this.onMouseEnter = noOpEnterLeave
        this.onMouseLeave = noOpEnterLeave
        this.onMouseDown  = noOpClick
        this.onMouseUp    = noOpClick
        this.onContextMenu = noOpClick
        this.mouseTracking = false
        this.winHandle = winHandle
        this.tooltipHwnd = 0 as HWND
    }
}

fn noOpEnterLeave(x: f64, y: f64) {}

let BUTTONS = new Map<i64, ButtonSlot>()

/// Window handle → current default-button handle. Win32 has no
/// runtime API for "which button is the dialog default" outside
/// `IsDialogMessageW`, so we track it ourselves; `tryDefaultEnter`
/// reads this from `wnd_proc` when Return is pressed.
let DEFAULT_BUTTONS = new Map<i64, i64>()

/// Called by `wnd_proc` for WM_COMMAND / BN_CLICKED. Returns
/// `true` when the HWND matched a button (so the dispatcher
/// stops walking).
pub fn tryButtonClick(hwnd: i64): bool {
    if let some(slot) = BUTTONS.get(hwnd) {
        slot.onClick(0.0, 0.0, 0)
        true
    } else {
        false
    }
}

/// Called by `wnd_proc` on `WM_KEYDOWN` with `VK_RETURN`. Fires
/// the default button's click handler if one is registered for
/// the given window. Returns `true` when a default button matched.
pub fn tryDefaultEnter(parentHandle: i64): bool {
    if let some(btnHandle) = DEFAULT_BUTTONS.get(parentHandle) {
        if let some(slot) = BUTTONS.get(btnHandle) {
            slot.onClick(0.0, 0.0, 0)
            return true
        }
    }
    false
}

/// Called by `wnd_proc` for WM_COMMAND / BN_SETFOCUS or
/// BN_KILLFOCUS. Looks up the button by control HWND and fires
/// the matching callback. Returns `true` when the HWND matched a
/// button so the dispatcher stops walking.
pub fn tryButtonFocus(hwnd: i64): bool {
    if let some(slot) = BUTTONS.get(hwnd) {
        let cb = slot.onFocus
        cb()
        true
    } else {
        false
    }
}
pub fn tryButtonBlur(hwnd: i64): bool {
    if let some(slot) = BUTTONS.get(hwnd) {
        let cb = slot.onBlur
        cb()
        true
    } else {
        false
    }
}

@extern(C) {

/// Window-procedure subclass installed on every BUTTON instance.
/// Surfaces the per-button mouse events that the default BUTTON
/// wndproc swallows. Forwards everything else to `DefSubclassProc`
/// so click + visual state stay intact.
pub fn buttonSubclassProc(
    hwnd: i64, msg: u32, wParam: u64, lParam: i64,
    idSubclass: u64, refData: u64
): i64 {
    let lo = (lParam & 0xFFFF) as f64
    let hi = ((lParam >> 16) & 0xFFFF) as f64
    match msg as WindowMessage {
        WindowMessage.mouseMove {
            if let some(slot) = BUTTONS.get(hwnd) {
                if !slot.mouseTracking {
                    // First move after creation / a previous leave:
                    // register for WM_MOUSELEAVE and synthesise the
                    // enter event ourselves.
                    let tme = TRACKMOUSEEVENT {
                        cbSize:      24 as u32,
                        dwFlags:     TrackMouseFlags.leave as u32,
                        hwndTrack:   hwnd as HWND,
                        dwHoverTime: 0 as u32,
                    }
                    TrackMouseEvent(&tme)
                    slot.mouseTracking = true
                    let cb = slot.onMouseEnter
                    cb(lo, hi)
                }
            }
        }
        WindowMessage.mouseLeave {
            if let some(slot) = BUTTONS.get(hwnd) {
                slot.mouseTracking = false
                let cb = slot.onMouseLeave
                cb(lo, hi)
            }
        }
        WindowMessage.lbuttondown {
            if let some(slot) = BUTTONS.get(hwnd) {
                let cb = slot.onMouseDown
                cb(lo, hi, 0)
            }
        }
        WindowMessage.lbuttonup {
            if let some(slot) = BUTTONS.get(hwnd) {
                let cb = slot.onMouseUp
                cb(lo, hi, 0)
            }
        }
        WindowMessage.mButtonDown {
            if let some(slot) = BUTTONS.get(hwnd) {
                let cb = slot.onMouseDown
                cb(lo, hi, 1)
            }
        }
        WindowMessage.mButtonUp {
            if let some(slot) = BUTTONS.get(hwnd) {
                let cb = slot.onMouseUp
                cb(lo, hi, 1)
            }
        }
        WindowMessage.rButtonDown {
            if let some(slot) = BUTTONS.get(hwnd) {
                let cb = slot.onMouseDown
                cb(lo, hi, 2)
                // Fire contextmenu at the same instant — matches
                // the cross-OS "right-press" timing the public API
                // promises. Keyboard-triggered context menus
                // (Shift+F10, the Menu key) would surface via
                // WM_CONTEXTMENU and aren't wired yet.
                let ctx = slot.onContextMenu
                ctx(lo, hi, 2)
            }
        }
        WindowMessage.rButtonUp {
            if let some(slot) = BUTTONS.get(hwnd) {
                let cb = slot.onMouseUp
                cb(lo, hi, 2)
            }
        }
        _ {}
    }
    DefSubclassProc(hwnd as HWND, msg, wParam, lParam)
}

pub fn createNativeButton(
    windowHandle: i64,
    text: string, x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool, isDefault: bool
): i64 {
    let parent = windowHwnd(windowHandle)
    if parent as i64 == 0 as i64 { return -1 }
    let classW = "BUTTON".encodeUtf16()
    let textW  = text.encodeUtf16()
    let visibleBits: u32 = if visible {
        WindowStyle.visible as u32
    } else {
        0 as u32
    }
    // `BS_NOTIFY` opts the BUTTON into BN_SETFOCUS / BN_KILLFOCUS
    // (and BN_DBLCLK / BN_PUSHED / ...) WM_COMMAND notifications.
    // Without it the BUTTON wndproc never tells the parent about
    // focus transitions, so onFocus / onBlur would never fire.
    let style: u32 = (WindowStyle.child as u32) | visibleBits | BS_NOTIFY
    let hInstance = GetModuleHandleA(0 as *const char)
    let hwnd = CreateWindowExW(
        0,
        classW,
        textW,
        style,
        x as i32, y as i32, width as i32, height as i32,
        parent,
        0 as HMENU,
        hInstance as HINSTANCE,
        0 as *void
    )
    let enabledFlag: i32 = if enabled { 1 } else { 0 }
    EnableWindow(hwnd, enabledFlag)
    applyExplorerTheme(hwnd)
    applyGuiFont(hwnd)
    // Subclass the BUTTON so we see WM_LBUTTONDOWN / UP /
    // WM_MOUSEMOVE / WM_MOUSELEAVE before they reach the
    // default BUTTON wndproc. `DefSubclassProc` from inside the
    // hook keeps the standard click + visual press feedback.
    SetWindowSubclass(hwnd, buttonSubclassProc, 1 as u64, 0 as u64)
    let handle = hwnd as i64
    BUTTONS[handle] = new ButtonSlot(hwnd, windowHandle)
    if isDefault { setButtonDefault(handle, true) }
    handle
}

pub fn setOnButtonClick(handle: i64, cb: fn(f64, f64, i32)) {
    if let some(slot) = BUTTONS.get(handle) { slot.onClick = cb }
}
pub fn setButtonText(handle: i64, text: string) {
    if let some(slot) = BUTTONS.get(handle) {
        SetWindowTextW(slot.hwnd, text.encodeUtf16())
    }
}
pub fn setButtonFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = BUTTONS.get(handle) {
        MoveWindow(slot.hwnd,
                   x as i32, y as i32, width as i32, height as i32, 1)
    }
}
pub fn setButtonEnabled(handle: i64, enabled: bool) {
    if let some(slot) = BUTTONS.get(handle) {
        let flag: i32 = if enabled { 1 } else { 0 }
        EnableWindow(slot.hwnd, flag)
    }
}
pub fn setButtonVisible(handle: i64, visible: bool) {
    if let some(slot) = BUTTONS.get(handle) {
        let cmd: i32 = if visible { ShowCmd.show as i32 } else { ShowCmd.hide as i32 }
        ShowWindow(slot.hwnd, cmd)
    }
}
pub fn clickButton(handle: i64) {
    if let some(slot) = BUTTONS.get(handle) {
        SendMessageW(slot.hwnd, BM_CLICK, 0 as u64, 0 as i64)
    }
}
pub fn setOnButtonFocus(handle: i64, cb: fn()) {
    if let some(slot) = BUTTONS.get(handle) { slot.onFocus = cb }
}
pub fn setOnButtonBlur(handle: i64, cb: fn()) {
    if let some(slot) = BUTTONS.get(handle) { slot.onBlur = cb }
}
pub fn setOnButtonMouseEnter(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = BUTTONS.get(handle) { slot.onMouseEnter = cb }
}
pub fn setOnButtonMouseLeave(handle: i64, cb: fn(f64, f64)) {
    if let some(slot) = BUTTONS.get(handle) { slot.onMouseLeave = cb }
}
pub fn setOnButtonMouseDown(handle: i64, cb: fn(f64, f64, i32)) {
    if let some(slot) = BUTTONS.get(handle) { slot.onMouseDown = cb }
}
pub fn setOnButtonMouseUp(handle: i64, cb: fn(f64, f64, i32)) {
    if let some(slot) = BUTTONS.get(handle) { slot.onMouseUp = cb }
}
pub fn setOnButtonContextMenu(handle: i64, cb: fn(f64, f64, i32)) {
    if let some(slot) = BUTTONS.get(handle) { slot.onContextMenu = cb }
}
pub fn setButtonTooltip(handle: i64, text: string) {
    if let some(slot) = BUTTONS.get(handle) {
        // Empty string → delete the tool from the tooltip control
        // (if one exists). We deliberately leave the tooltip HWND
        // alive even after a clear so flipping back to a non-empty
        // string reuses it.
        if text == "" {
            if slot.tooltipHwnd as i64 != 0 as i64 {
                let info = TOOLINFOW {
                    cbSize:     72 as u32,
                    uFlags:     TTF_IDISHWND,
                    hwnd:       slot.winHandle as HWND,
                    uId:        slot.hwnd as u64,
                    rect:       RECT { left: 0 as i32, top: 0 as i32, right: 0 as i32, bottom: 0 as i32 },
                    hinst:      0 as HINSTANCE,
                    lpszText:   0 as *const u16,
                    lParam:     0 as i64,
                    lpReserved: 0 as *void,
                }
                let p: *const TOOLINFOW = &info
                SendMessageW(slot.tooltipHwnd, TTM_DELTOOLW, 0 as u64, p as i64)
            }
            return
        }
        // Wide-text buffer kept in scope so ARC doesn't free it
        // mid-call. TOOLINFOW.lpszText must outlive the
        // SendMessageW that delivers it; tooltip controls copy the
        // string internally so re-using `text` after this is fine.
        let textW = text.encodeUtf16()
        let info = TOOLINFOW {
            cbSize:     72 as u32,
            uFlags:     TTF_IDISHWND | TTF_SUBCLASS,
            hwnd:       slot.winHandle as HWND,
            uId:        slot.hwnd as u64,
            rect:       RECT { left: 0 as i32, top: 0 as i32, right: 0 as i32, bottom: 0 as i32 },
            hinst:      0 as HINSTANCE,
            lpszText:   textW,
            lParam:     0 as i64,
            lpReserved: 0 as *void,
        }
        let p: *const TOOLINFOW = &info
        if slot.tooltipHwnd as i64 == 0 as i64 {
            // First-time setup: spawn a tooltip control as a popup
            // child of the parent window. `TTS_ALWAYSTIP` makes the
            // tooltip appear even when the owning window is
            // inactive (matches AppKit's behaviour). `TTF_SUBCLASS`
            // tells the tooltip control to install its own
            // SetWindowSubclass on the button so the cursor-track
            // messages are picked up without us relaying them
            // ourselves — Win32 keys multi-subclass by `idSubclass`
            // so it co-exists with our `buttonSubclassProc`.
            let classW = "tooltips_class32".encodeUtf16()
            let hInstance = GetModuleHandleA(0 as *const char)
            let tt = CreateWindowExW(
                0,
                classW,
                0 as *const u16,
                (WindowStyle.popup as u32) | TTS_ALWAYSTIP,
                0, 0, 0, 0,
                slot.winHandle as HWND,
                0 as HMENU,
                hInstance as HINSTANCE,
                0 as *void
            )
            slot.tooltipHwnd = tt
            SendMessageW(tt, TTM_ADDTOOLW, 0 as u64, p as i64)
        } else {
            SendMessageW(slot.tooltipHwnd, TTM_UPDATETIPTEXTW, 0 as u64, p as i64)
        }
    }
}
pub fn focusButton(handle: i64) {
    if let some(slot) = BUTTONS.get(handle) {
        // `SetFocus` posts WM_KILLFOCUS to the previous holder and
        // WM_SETFOCUS to the button; the BUTTON wndproc then sends
        // BN_KILLFOCUS / BN_SETFOCUS WM_COMMAND notifications to
        // the parent because we built the control with BS_NOTIFY.
        SetFocus(slot.hwnd)
    }
}
pub fn setButtonDefault(handle: i64, isDefault: bool) {
    if let some(slot) = BUTTONS.get(handle) {
        if isDefault {
            // Drop the heavy frame off the previous holder so the
            // window only has one visually-default button. Skip when
            // the previous holder *is* this button (idempotent set).
            if let some(prev) = DEFAULT_BUTTONS.get(slot.winHandle) {
                if prev != handle {
                    if let some(prevSlot) = BUTTONS.get(prev) {
                        SendMessageW(prevSlot.hwnd, BM_SETSTYLE,
                                     BS_PUSHBUTTON, 1 as i64)
                    }
                }
            }
            SendMessageW(slot.hwnd, BM_SETSTYLE, BS_DEFPUSHBUTTON, 1 as i64)
            DEFAULT_BUTTONS[slot.winHandle] = handle
        } else {
            SendMessageW(slot.hwnd, BM_SETSTYLE, BS_PUSHBUTTON, 1 as i64)
            if let some(cur) = DEFAULT_BUTTONS.get(slot.winHandle) {
                if cur == handle { DEFAULT_BUTTONS.delete(slot.winHandle) }
            }
        }
    }
}

} // @extern(C)
