/// Combo box (dropdown) — COMBOBOX window class with
/// `CBS_DROPDOWNLIST`. CBN_SELCHANGE = notify 1 (shared with
/// LBN_SELCHANGE) — `wnd_proc` calls `tryComboSelect` first.

use windows {
    HWND, HINSTANCE, HMENU,
    WindowStyle, ShowCmd,
    GetModuleHandleA,
    CreateWindowExW, EnableWindow, MoveWindow, ShowWindow,
    SendMessageA, SendMessageW,
    HeapFree, GetProcessHeap,
}
use util { toWideUtf8, noOpI32 }
use core { windowHwnd }

class ComboSlot {
    pub hwnd:     HWND
    pub onSelect: fn(i32)
    pub init(hwnd: HWND) {
        this.hwnd = hwnd
        this.onSelect = noOpI32
    }
}

let COMBOS: ComboSlot[] = []

fn findComboSlot(hwnd: i64): i64 {
    let n = COMBOS.length
    let i: i64 = 0
    let found: i64 = -1
    while i < n {
        if COMBOS[i].hwnd as i64 == hwnd {
            found = i
            i = n
        } else {
            i = i + 1
        }
    }
    found
}

pub fn tryComboSelect(hwnd: i64): bool {
    let ix = findComboSlot(hwnd)
    if ix < 0 { return false }
    let cb = COMBOS[ix].onSelect
    cb(readComboSelectedIndex(ix))
    true
}

@extern(C) {

pub fn createNativeCombo(
    windowHandle: i64,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let parent = windowHwnd(windowHandle)
    if parent as i64 == 0 as i64 { return -1 }
    let id: i64 = COMBOS.length as i64
    let classW = toWideUtf8("COMBOBOX") as *u16
    let emptyW = toWideUtf8("") as *u16
    let visibleBits: u32 = if visible { WindowStyle.visible as u32 } else { 0 as u32 }
    // CBS_DROPDOWNLIST = 0x0003, WS_VSCROLL for tall lists.
    let style: u32 = (WindowStyle.child as u32) | visibleBits
                   | (0x0003 as u32)
                   | (WindowStyle.vScroll as u32)
    let hInstance = GetModuleHandleA(0 as *const char)
    // Win32 takes (closed box + dropped list) as one rect; add
    // ~120 px so the popup is actually usable.
    let totalH = (height as i32) + 120
    let hwnd = CreateWindowExW(
        0,
        classW,
        emptyW,
        style,
        x as i32, y as i32, width as i32, totalH,
        parent,
        0 as HMENU,
        hInstance as HINSTANCE,
        0 as *void
    )
    HeapFree(GetProcessHeap(), 0 as u32, classW as *void)
    HeapFree(GetProcessHeap(), 0 as u32, emptyW as *void)
    let enabledFlag: i32 = if enabled { 1 } else { 0 }
    EnableWindow(hwnd, enabledFlag)
    COMBOS.push(new ComboSlot(hwnd))
    id
}

pub fn setOnComboSelect(handle: i64, cb: fn(i32)) {
    if handle >= 0 && handle < (COMBOS.length as i64) {
        COMBOS[handle].onSelect = cb
    }
}
/// CB_ADDSTRING = 0x143 (wide variant for non-ASCII items).
pub fn addComboItem(handle: i64, item: string) {
    if handle >= 0 && handle < (COMBOS.length as i64) {
        let w = toWideUtf8(item) as *u16
        let _ = SendMessageW(COMBOS[handle].hwnd, 0x0143 as u32,
                             0 as u64, w as i64)
        HeapFree(GetProcessHeap(), 0 as u32, w as *void)
    }
}
/// CB_RESETCONTENT = 0x14B.
pub fn clearComboItems(handle: i64) {
    if handle >= 0 && handle < (COMBOS.length as i64) {
        let _ = SendMessageA(COMBOS[handle].hwnd, 0x014B as u32,
                             0 as u64, 0 as i64)
    }
}
/// CB_SETCURSEL = 0x14E.
pub fn setComboSelectedIndex(handle: i64, index: i32) {
    if handle >= 0 && handle < (COMBOS.length as i64) {
        let _ = SendMessageA(COMBOS[handle].hwnd, 0x014E as u32,
                             index as i64 as u64, 0 as i64)
    }
}
/// CB_GETCURSEL = 0x147; returns `CB_ERR = -1` for no selection.
pub fn readComboSelectedIndex(handle: i64): i32 {
    if handle < 0 || handle >= (COMBOS.length as i64) { return -1 as i32 }
    let r = SendMessageA(COMBOS[handle].hwnd, 0x0147 as u32, 0 as u64, 0 as i64)
    r as i32
}
pub fn setComboFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if handle >= 0 && handle < (COMBOS.length as i64) {
        let totalH = (height as i32) + 120
        MoveWindow(COMBOS[handle].hwnd,
                   x as i32, y as i32, width as i32, totalH, 1)
    }
}
pub fn setComboEnabled(handle: i64, enabled: bool) {
    if handle >= 0 && handle < (COMBOS.length as i64) {
        let flag: i32 = if enabled { 1 } else { 0 }
        EnableWindow(COMBOS[handle].hwnd, flag)
    }
}
pub fn setComboVisible(handle: i64, visible: bool) {
    if handle >= 0 && handle < (COMBOS.length as i64) {
        let cmd: i32 = if visible { ShowCmd.show as i32 } else { ShowCmd.hide as i32 }
        ShowWindow(COMBOS[handle].hwnd, cmd)
    }
}

} // @extern(C)
