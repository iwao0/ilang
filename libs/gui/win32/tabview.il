/// Tab strip — `SysTabControl32` common control. Each ilang Tab
/// is a single tab item; the tab control itself is just the row
/// of clickable labels (Round 1 — content management is on the
/// caller).
///
/// Tabs notify selection changes via WM_NOTIFY / TCN_SELCHANGE
/// on the parent window; `wnd_proc` calls `tryTabViewSelChange`
/// from its WM_NOTIFY arm to dispatch into the right slot.

use std.ffi { readI32, readU64 }
use windows {
    HWND, HINSTANCE, HMENU, RECT,
    WindowStyle, ShowCmd, WindowMessage,
    NMHDR, TCITEMW, TCIF_TEXT, TCN_SELCHANGE,
    TCM_INSERTITEMW, TCM_DELETEITEM, TCM_GETCURSEL, TCM_SETCURSEL,
    TCM_GETITEMCOUNT, TCM_SETITEMW,
    GetModuleHandleA,
    CreateWindowExW, EnableWindow, MoveWindow, ShowWindow,
    SendMessageW, SendMessageA,
    TOOLINFOW, TTS_ALWAYSTIP, TTF_IDISHWND, TTF_SUBCLASS,
    TTM_ADDTOOLW, TTM_DELTOOLW, TTM_UPDATETIPTEXTW,
}
use util { applyGuiFont }
use core { windowHwnd }

class TabViewSlot {
    pub hwnd:     HWND
    pub winHandle: i64
    pub tooltipHwnd: HWND
    pub onChange: fn(i32)
    pub init(hwnd: HWND, winHandle: i64) {
        this.hwnd = hwnd
        this.winHandle = winHandle
        this.tooltipHwnd = 0 as HWND
        this.onChange = tabNoOpI32
    }
}

fn tabNoOpI32(i: i32) {}

let TAB_VIEWS = new Map<i64, TabViewSlot>()

/// Called by wnd_proc on WM_NOTIFY when the notification's hwndFrom
/// matches a registered TabView. `code` is the NMHDR.code field.
@extern(C) {
pub fn tryTabViewSelChange(hwnd: i64, code: i32): bool {
    if code != TCN_SELCHANGE { return false }
    if let some(slot) = TAB_VIEWS.get(hwnd) {
        let idx = SendMessageA(slot.hwnd, TCM_GETCURSEL, 0 as u64, 0 as i64) as i32
        slot.onChange(idx)
        return true
    }
    false
}
}

@extern(C) {

pub fn createNativeTabView(
    windowHandle: i64,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let parent = windowHwnd(windowHandle)
    if parent as i64 == 0 as i64 { return -1 }
    let classW = "SysTabControl32".encodeUtf16()
    let emptyW = "".encodeUtf16()
    let visibleBits: u32 = if visible { WindowStyle.visible as u32 } else { 0 as u32 }
    let disabledBits: u32 = if !enabled { WindowStyle.disabled as u32 } else { 0 as u32 }
    let style: u32 = (WindowStyle.child as u32) | visibleBits | disabledBits
    let hInstance = GetModuleHandleA(0 as *const char)
    let hwnd = CreateWindowExW(
        0,
        classW,
        emptyW,
        style,
        x as i32, y as i32, width as i32, height as i32,
        parent,
        0 as HMENU,
        hInstance as HINSTANCE,
        0 as *void
    )
    applyGuiFont(hwnd)
    let handle = hwnd as i64
    TAB_VIEWS[handle] = new TabViewSlot(hwnd, windowHandle)
    handle
}

pub fn setTabViewFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = TAB_VIEWS.get(handle) {
        MoveWindow(slot.hwnd, x as i32, y as i32, width as i32, height as i32, 1)
    }
}
pub fn setTabViewEnabled(handle: i64, enabled: bool) {
    if let some(slot) = TAB_VIEWS.get(handle) {
        let flag: i32 = if enabled { 1 } else { 0 }
        EnableWindow(slot.hwnd, flag)
    }
}
pub fn setTabViewVisible(handle: i64, visible: bool) {
    if let some(slot) = TAB_VIEWS.get(handle) {
        let cmd: i32 = if visible { ShowCmd.show as i32 } else { ShowCmd.hide as i32 }
        ShowWindow(slot.hwnd, cmd)
    }
}
pub fn setOnTabViewChange(handle: i64, cb: fn(i32)) {
    if let some(slot) = TAB_VIEWS.get(handle) { slot.onChange = cb }
}

pub fn tabViewAddTab(handle: i64, title: string): i64 {
    match TAB_VIEWS.get(handle) {
        some(slot) {
            let count = SendMessageA(slot.hwnd, TCM_GETITEMCOUNT, 0 as u64, 0 as i64) as i32
            let textW = title.encodeUtf16()
            let item = TCITEMW {
                mask:        TCIF_TEXT,
                dwState:     0 as u32,
                dwStateMask: 0 as u32,
                _pad0:       0 as u32,
                pszText:     textW,
                cchTextMax:  0 as i32,
                iImage:      0 as i32,
                lParam:      0 as i64,
            }
            let p: *const TCITEMW = &item
            SendMessageW(slot.hwnd, TCM_INSERTITEMW, count as u64, p as i64)
            // win32 has no per-item stable pointer, so we hand back
            // the 0-based insert index — the caller stores it as
            // `TabItem.handle`. Subsequent reorderings via
            // `removeTab` will shift sibling indices.
            count as i64
        }
        none { 0 as i64 }
    }
}

pub fn tabViewRemoveTab(handle: i64, index: i32) {
    if let some(slot) = TAB_VIEWS.get(handle) {
        SendMessageA(slot.hwnd, TCM_DELETEITEM, index as u64, 0 as i64)
    }
}

pub fn tabViewGetSelectedIndex(handle: i64): i32 {
    match TAB_VIEWS.get(handle) {
        some(slot) {
            SendMessageA(slot.hwnd, TCM_GETCURSEL, 0 as u64, 0 as i64) as i32
        }
        none { -1 as i32 }
    }
}
pub fn tabViewSetSelectedIndex(handle: i64, index: i32) {
    if let some(slot) = TAB_VIEWS.get(handle) {
        SendMessageA(slot.hwnd, TCM_SETCURSEL, index as u64, 0 as i64)
    }
}
pub fn tabViewGetTabCount(handle: i64): i32 {
    match TAB_VIEWS.get(handle) {
        some(slot) {
            SendMessageA(slot.hwnd, TCM_GETITEMCOUNT, 0 as u64, 0 as i64) as i32
        }
        none { 0 as i32 }
    }
}

pub fn tabItemSetTitle(viewHandle: i64, itemHandle: i64, title: string) {
    if let some(slot) = TAB_VIEWS.get(viewHandle) {
        let textW = title.encodeUtf16()
        let item = TCITEMW {
            mask:        TCIF_TEXT,
            dwState:     0 as u32,
            dwStateMask: 0 as u32,
            _pad0:       0 as u32,
            pszText:     textW,
            cchTextMax:  0 as i32,
            iImage:      0 as i32,
            lParam:      0 as i64,
        }
        let p: *const TCITEMW = &item
        SendMessageW(slot.hwnd, TCM_SETITEMW, itemHandle as u64, p as i64)
    }
}
pub fn tabItemSetEnabled(viewHandle: i64, itemHandle: i64, enabled: bool) {
    // SysTabControl32 doesn't expose per-tab disable — surface
    // omitted on win32 for now.
}

pub fn setTabViewTooltip(handle: i64, text: string) {
    if let some(slot) = TAB_VIEWS.get(handle) {
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
