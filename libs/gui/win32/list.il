/// Scrollable list — LISTBOX window class. LBN_SELCHANGE shares
/// notify code 1 with CBN_SELCHANGE — `wnd_proc` walks combos
/// first, then us.

use windows {
    HWND, HINSTANCE, HMENU,
    WindowStyle, ShowCmd,
    GetModuleHandleA,
    CreateWindowExW, EnableWindow, MoveWindow, ShowWindow,
    SendMessageA, SendMessageW,
    HeapFree, GetProcessHeap,
}
use util { toWideUtf8, applyGuiFont, noOpI32 }
use core { windowHwnd }

class ListSlot {
    pub hwnd:     HWND
    pub onSelect: fn(i32)
    pub init(hwnd: HWND) {
        this.hwnd = hwnd
        this.onSelect = noOpI32
    }
}

let LISTS = new Map<i64, ListSlot>()

pub fn tryListSelect(hwnd: i64): bool {
    if let some(slot) = LISTS.get(hwnd) {
        slot.onSelect(readListSelectedIndex(hwnd))
        true
    } else {
        false
    }
}

@extern(C) {

pub fn createNativeList(
    windowHandle: i64,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let parent = windowHwnd(windowHandle)
    if parent as i64 == 0 as i64 { return -1 }
    let classW = toWideUtf8("LISTBOX") as *u16
    let emptyW = toWideUtf8("") as *u16
    let visibleBits: u32 = if visible { WindowStyle.visible as u32 } else { 0 as u32 }
    // LBS_NOTIFY = 0x0001 — required for LBN_SELCHANGE callbacks.
    let style: u32 = (WindowStyle.child as u32) | visibleBits
                   | (WindowStyle.border as u32)
                   | (WindowStyle.vScroll as u32)
                   | (0x0001 as u32)
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
    HeapFree(GetProcessHeap(), 0 as u32, classW as *void)
    HeapFree(GetProcessHeap(), 0 as u32, emptyW as *void)
    let enabledFlag: i32 = if enabled { 1 } else { 0 }
    EnableWindow(hwnd, enabledFlag)
    applyGuiFont(hwnd as i64)
    let handle = hwnd as i64
    LISTS[handle] = new ListSlot(hwnd)
    handle
}

pub fn setOnListSelect(handle: i64, cb: fn(i32)) {
    if let some(slot) = LISTS.get(handle) { slot.onSelect = cb }
}
/// LB_ADDSTRING = 0x0180.
pub fn addListItem(handle: i64, item: string) {
    if let some(slot) = LISTS.get(handle) {
        let w = toWideUtf8(item) as *u16
        let _ = SendMessageW(slot.hwnd, 0x0180 as u32,
                             0 as u64, w as i64)
        let _ = HeapFree(GetProcessHeap(), 0 as u32, w as *void)
    }
}
/// LB_RESETCONTENT = 0x0184.
pub fn clearListItems(handle: i64) {
    if let some(slot) = LISTS.get(handle) {
        let _ = SendMessageA(slot.hwnd, 0x0184 as u32,
                             0 as u64, 0 as i64)
    }
}
/// LB_SETCURSEL = 0x0186.
pub fn setListSelectedIndex(handle: i64, index: i32) {
    if let some(slot) = LISTS.get(handle) {
        let _ = SendMessageA(slot.hwnd, 0x0186 as u32,
                             index as i64 as u64, 0 as i64)
    }
}
/// LB_GETCURSEL = 0x0188; returns `LB_ERR = -1` for no selection.
pub fn readListSelectedIndex(handle: i64): i32 {
    if let some(slot) = LISTS.get(handle) {
        let r = SendMessageA(slot.hwnd, 0x0188 as u32, 0 as u64, 0 as i64)
        r as i32
    } else {
        -1 as i32
    }
}
pub fn setListFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = LISTS.get(handle) {
        let _ = MoveWindow(slot.hwnd,
                   x as i32, y as i32, width as i32, height as i32, 1)
    }
}
pub fn setListEnabled(handle: i64, enabled: bool) {
    if let some(slot) = LISTS.get(handle) {
        let flag: i32 = if enabled { 1 } else { 0 }
        let _ = EnableWindow(slot.hwnd, flag)
    }
}
pub fn setListVisible(handle: i64, visible: bool) {
    if let some(slot) = LISTS.get(handle) {
        let cmd: i32 = if visible { ShowCmd.show as i32 } else { ShowCmd.hide as i32 }
        let _ = ShowWindow(slot.hwnd, cmd)
    }
}

} // @extern(C)
