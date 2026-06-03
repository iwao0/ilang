/// Label widget — STATIC window class. Display-only, no events.

use windows {
    HWND, HINSTANCE, HMENU, RECT,
    WindowStyle, ShowCmd,
    TOOLINFOW, TTS_ALWAYSTIP, TTF_IDISHWND, TTF_SUBCLASS,
    TTM_ADDTOOLW, TTM_DELTOOLW, TTM_UPDATETIPTEXTW,
    GetModuleHandleA,
    CreateWindowExW, SetWindowTextW, MoveWindow, ShowWindow,
    SendMessageW,
}
use util { applyGuiFont }
use core { windowHwnd }

class LabelSlot {
    pub hwnd: HWND
    pub winHandle: i64
    /// Tooltip control owned by this label. Lazy-created on the
    /// first `setLabelTooltip(_, non-empty)` call; reused for
    /// subsequent updates via `TTM_UPDATETIPTEXTW`. Stays `0` (the
    /// HWND nullary) while no tooltip is configured.
    pub tooltipHwnd: HWND
    pub init(hwnd: HWND, winHandle: i64) {
        this.hwnd = hwnd
        this.winHandle = winHandle
        this.tooltipHwnd = 0 as HWND
    }
}

let LABELS = new Map<i64, LabelSlot>()

@extern(C) {

pub fn createNativeLabel(
    windowHandle: i64,
    text: string, x: f64, y: f64, width: f64, height: f64,
    visible: bool, selectable: bool
): i64 {
    // `selectable` is honoured on macOS (NSTextField), no-op here:
    // Win32 STATIC controls don't carry a selection state.
    let parent = windowHwnd(windowHandle)
    if parent as i64 == 0 as i64 { return -1 }
    let classW = "STATIC".encodeUtf16()
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
        0 as HMENU,            // labels emit no commands
        hInstance as HINSTANCE,
        0 as *void
    )
    applyGuiFont(hwnd)
    let handle = hwnd as i64
    LABELS[handle] = new LabelSlot(hwnd, windowHandle)
    handle
}

pub fn setLabelText(handle: i64, text: string) {
    if let some(slot) = LABELS.get(handle) {
        SetWindowTextW(slot.hwnd, text.encodeUtf16())
    }
}
pub fn setLabelFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = LABELS.get(handle) {
        MoveWindow(slot.hwnd,
                   x as i32, y as i32, width as i32, height as i32, 1)
    }
}
pub fn setLabelVisible(handle: i64, visible: bool) {
    if let some(slot) = LABELS.get(handle) {
        let cmd: i32 = if visible { ShowCmd.show as i32 } else { ShowCmd.hide as i32 }
        ShowWindow(slot.hwnd, cmd)
    }
}
pub fn setLabelSelectable(handle: i64, selectable: bool) {
    // No-op on Win32 — STATIC controls don't carry a selection state.
}
pub fn setLabelTooltip(handle: i64, text: string) {
    if let some(slot) = LABELS.get(handle) {
        // Empty string → delete the tool from the tooltip control
        // (if one exists). The tooltip HWND is left alive so
        // flipping back to a non-empty string reuses it.
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
        // `textW` must outlive the SendMessageW call: TOOLINFOW
        // stores a borrowed pointer that the tooltip control reads
        // before copying internally.
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

} // @extern(C)
