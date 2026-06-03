/// Push-button widget. Backed by the BUTTON window class.
///
/// `BN_CLICKED` notifications come in via `wnd_proc.guiWndProc`'s
/// WM_COMMAND arm; we expose `tryButtonClick(hwnd)` so the proc
/// doesn't have to reach into our `BUTTONS` map directly.

use windows {
    HWND, HINSTANCE, HMENU,
    WindowStyle, ShowCmd,
    GetModuleHandleA,
    CreateWindowExW, SetWindowTextW, EnableWindow, MoveWindow, ShowWindow,
    SendMessageW,
}
use util { applyExplorerTheme, applyGuiFont, noOpClick }
use core { windowHwnd }

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

/// Per-button state. Stored in `BUTTONS` keyed by the button's
/// HWND (`hwnd as i64`) — same key gui-core sees as the public
/// `handle: i64`, so both the public setters and the
/// wnd_proc-side sender lookup are O(1) Map gets.
class ButtonSlot {
    pub hwnd:    HWND
    pub onClick: fn(f64, f64, i32)
    pub winHandle: i64
    pub init(hwnd: HWND, winHandle: i64) {
        this.hwnd = hwnd
        this.onClick = noOpClick
        this.winHandle = winHandle
    }
}

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

@extern(C) {

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
    let style: u32 = (WindowStyle.child as u32) | visibleBits
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
