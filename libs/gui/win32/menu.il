/// Win32 menu backend. CreatePopupMenu + AppendMenuW backs each
/// ilang Menu; SetMenu installs the assembled bar on a window.
/// Item clicks come back as WM_COMMAND with lParam = 0 and the
/// item id in LOWORD(wParam) — `tryMenuClick(id)` looks the id up
/// in ITEM_ACTIONS and fires the callback.
///
/// Keyboard accelerators are rendered as a trailing `\tCtrl+X`
/// hint in the menu label; live accelerator dispatch (via
/// TranslateAccelerator) isn't wired yet — pressing the shortcut
/// won't fire the item until that's added in a follow-up.

use windows {
    HWND, HMENU,
    MenuFlags,
    CreatePopupMenu, CreateMenu, AppendMenuW, SetMenu, DrawMenuBar,
    HeapFree, GetProcessHeap,
}
use util { toWideUtf8 }
use core { windowHwnd }

class MenuSlot {
    pub handle: HMENU
    pub title:  string
    pub init(handle: HMENU, title: string) {
        this.handle = handle
        this.title = title
    }
}

let MENUS = new Map<i64, MenuSlot>()
let ITEM_ACTIONS = new Map<i32, fn()>()
let NEXT_ID: i32 = 1000

/// Called by wnd_proc on WM_COMMAND with lParam = 0 (no control
/// HWND, so it's either a menu item or an accelerator). Returns
/// `true` when `id` matched a known menu item.
pub fn tryMenuClick(id: i32): bool {
    if let some(cb) = ITEM_ACTIONS.get(id) {
        cb()
        return true
    }
    false
}

@extern(C) {

pub fn createNativeMenu(title: string): i64 {
    let h = CreatePopupMenu()
    let key = h as i64
    MENUS[key] = new MenuSlot(h, title)
    key
}

pub fn menuAddItem(menuHandle: i64, text: string, onClick: fn()) {
    menuAddItemWithShortcut(menuHandle, text, "", onClick)
}

pub fn menuAddItemWithShortcut(
    menuHandle: i64, text: string, shortcut: string, onClick: fn()
) {
    if let some(slot) = MENUS.get(menuHandle) {
        let id = NEXT_ID
        NEXT_ID = NEXT_ID + 1
        let label = if shortcut.length > 0 {
            text + "\tCtrl+" + shortcut
        } else {
            text
        }
        let w = toWideUtf8(label) as *u16
        AppendMenuW(slot.handle, MenuFlags.string as u32, id as u64, w)
        HeapFree(GetProcessHeap(), 0 as u32, w as *void)
        ITEM_ACTIONS[id] = onClick
    }
}

pub fn menuAddSeparator(menuHandle: i64) {
    if let some(slot) = MENUS.get(menuHandle) {
        AppendMenuW(slot.handle, MenuFlags.separator as u32, 0 as u64, 0 as *u16)
    }
}

pub fn menuAddSubmenu(parentHandle: i64, childHandle: i64) {
    if let some(parent) = MENUS.get(parentHandle) {
        if let some(child) = MENUS.get(childHandle) {
            let w = toWideUtf8(child.title) as *u16
            let flags = (MenuFlags.popup as u32) | (MenuFlags.string as u32)
            AppendMenuW(parent.handle, flags, child.handle as u64, w)
            HeapFree(GetProcessHeap(), 0 as u32, w as *void)
        }
    }
}

pub fn setWindowMenuBar(windowHandle: i64, menuHandles: i64[]) {
    let hwnd = windowHwnd(windowHandle)
    if hwnd as i64 == 0 as i64 { return }
    let bar = CreateMenu()
    let n = menuHandles.length
    let i = 0
    while i < n {
        if let some(slot) = MENUS.get(menuHandles[i]) {
            let w = toWideUtf8(slot.title) as *u16
            let flags = (MenuFlags.popup as u32) | (MenuFlags.string as u32)
            AppendMenuW(bar, flags, slot.handle as u64, w)
            HeapFree(GetProcessHeap(), 0 as u32, w as *void)
        }
        i = i + 1
    }
    SetMenu(hwnd, bar)
    DrawMenuBar(hwnd)
}

} // @extern(C)
