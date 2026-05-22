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

let LISTS: ListSlot[] = []

fn findListSlot(hwnd: i64): i64 {
    let n = LISTS.length
    let i: i64 = 0
    let found: i64 = -1
    while i < n {
        if LISTS[i].hwnd as i64 == hwnd {
            found = i
            i = n
        } else {
            i = i + 1
        }
    }
    found
}

pub fn tryListSelect(hwnd: i64): bool {
    let ix = findListSlot(hwnd)
    if ix < 0 { return false }
    let cb = LISTS[ix].onSelect
    cb(readListSelectedIndex(ix))
    true
}

@extern(C) {

pub fn createNativeList(
    windowHandle: i64,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let parent = windowHwnd(windowHandle)
    if parent as i64 == 0 as i64 { return -1 }
    let id: i64 = LISTS.length as i64
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
    LISTS.push(new ListSlot(hwnd))
    id
}

pub fn setOnListSelect(handle: i64, cb: fn(i32)) {
    if handle >= 0 && handle < (LISTS.length as i64) {
        LISTS[handle].onSelect = cb
    }
}
/// LB_ADDSTRING = 0x0180.
pub fn addListItem(handle: i64, item: string) {
    if handle >= 0 && handle < (LISTS.length as i64) {
        let w = toWideUtf8(item) as *u16
        let _ = SendMessageW(LISTS[handle].hwnd, 0x0180 as u32,
                             0 as u64, w as i64)
        HeapFree(GetProcessHeap(), 0 as u32, w as *void)
    }
}
/// LB_RESETCONTENT = 0x0184.
pub fn clearListItems(handle: i64) {
    if handle >= 0 && handle < (LISTS.length as i64) {
        let _ = SendMessageA(LISTS[handle].hwnd, 0x0184 as u32,
                             0 as u64, 0 as i64)
    }
}
/// LB_SETCURSEL = 0x0186.
pub fn setListSelectedIndex(handle: i64, index: i32) {
    if handle >= 0 && handle < (LISTS.length as i64) {
        let _ = SendMessageA(LISTS[handle].hwnd, 0x0186 as u32,
                             index as i64 as u64, 0 as i64)
    }
}
/// LB_GETCURSEL = 0x0188; returns `LB_ERR = -1` for no selection.
pub fn readListSelectedIndex(handle: i64): i32 {
    if handle < 0 || handle >= (LISTS.length as i64) { return -1 as i32 }
    let r = SendMessageA(LISTS[handle].hwnd, 0x0188 as u32, 0 as u64, 0 as i64)
    r as i32
}
pub fn setListFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if handle >= 0 && handle < (LISTS.length as i64) {
        MoveWindow(LISTS[handle].hwnd,
                   x as i32, y as i32, width as i32, height as i32, 1)
    }
}
pub fn setListEnabled(handle: i64, enabled: bool) {
    if handle >= 0 && handle < (LISTS.length as i64) {
        let flag: i32 = if enabled { 1 } else { 0 }
        EnableWindow(LISTS[handle].hwnd, flag)
    }
}
pub fn setListVisible(handle: i64, visible: bool) {
    if handle >= 0 && handle < (LISTS.length as i64) {
        let cmd: i32 = if visible { ShowCmd.show as i32 } else { ShowCmd.hide as i32 }
        ShowWindow(LISTS[handle].hwnd, cmd)
    }
}

} // @extern(C)
