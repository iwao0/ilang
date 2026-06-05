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
    HWND, HMENU, POINT,
    MenuFlags,
    CreatePopupMenu, CreateMenu, AppendMenuW, SetMenu, DrawMenuBar,
    EnableMenuItem, CheckMenuItem, CheckMenuRadioItem,
    TrackPopupMenuEx, ClientToScreen,
}
use core { windowHwnd }

class MenuSlot {
    pub handle: HMENU
    pub title:  string
    /// Radio-group members per group name. Used by `tryMenuClick`
    /// to drive `CheckMenuRadioItem` across the right id range.
    pub radioGroups: Map<string, i32[]>
    pub init(handle: HMENU, title: string) {
        this.handle = handle
        this.title = title
        this.radioGroups = new Map<string, i32[]>()
    }
}

let MENUS = new Map<i64, MenuSlot>()
let ITEM_ACTIONS = new Map<i32, fn()>()
let CHECK_ACTIONS = new Map<i32, fn(bool)>()
let ITEM_CHECKED = new Map<i32, bool>()
/// id → (menuHandle, groupName)
class RadioInfo {
    pub menuHandle: i64
    pub group:      string
    pub init(menuHandle: i64, group: string) {
        this.menuHandle = menuHandle
        this.group = group
    }
}
let RADIO_INFO = new Map<i32, RadioInfo>()
/// id → owning HMENU. Lets `menuItemSet*` find the right menu
/// without the public API having to pass it in.
let ITEM_MENU = new Map<i32, HMENU>()

let NEXT_ID: i32 = 1000

/// Called by wnd_proc on WM_COMMAND with lParam = 0 (no control
/// HWND, so it's either a menu item or an accelerator). Returns
/// `true` when `id` matched a known menu item.
@extern(C) {
pub fn tryMenuClick(id: i32): bool {
    // Plain command item.
    if let some(cb) = ITEM_ACTIONS.get(id) {
        // Radio item — share the same `cb` storage; check before
        // firing so the native group state stays in sync.
        if let some(info) = RADIO_INFO.get(id) {
            if let some(slot) = MENUS.get(info.menuHandle) {
                match slot.radioGroups.get(info.group) {
                    some(members) {
                        let n = members.length
                        let first = members[0]
                        let last  = members[n - 1]
                        // CheckMenuRadioItem clears every member of
                        // the [first, last] range and checks `id`.
                        // MF_BYCOMMAND = 0.
                        CheckMenuRadioItem(
                            slot.handle, first as u32, last as u32,
                            id as u32, 0 as u32
                        )
                        let i = 0
                        while i < n {
                            ITEM_CHECKED[members[i]] = members[i] == id
                            i = i + 1
                        }
                    }
                    none {}
                }
            }
        }
        cb()
        return true
    }
    // Check item.
    if let some(cb) = CHECK_ACTIONS.get(id) {
        let prev = match ITEM_CHECKED.get(id) {
            some(b) { b }
            none    { false }
        }
        let next = !prev
        ITEM_CHECKED[id] = next
        if let some(hmenu) = ITEM_MENU.get(id) {
            // MF_BYCOMMAND (0x0) | MF_CHECKED (0x8) / MF_UNCHECKED (0x0)
            let flag: u32 = if next { 0x8 as u32 } else { 0 as u32 }
            CheckMenuItem(hmenu, id as u32, flag)
        }
        cb(next)
        return true
    }
    false
}

pub fn createNativeMenu(title: string): i64 {
    let h = CreatePopupMenu()
    let key = h as i64
    MENUS[key] = new MenuSlot(h, title)
    key
}

pub fn menuAddItem(menuHandle: i64, text: string, onClick: fn()): i64 {
    menuAddItemWithShortcut(menuHandle, text, "", onClick)
}

pub fn menuAddItemWithShortcut(
    menuHandle: i64, text: string, shortcut: string, onClick: fn()
): i64 {
    match MENUS.get(menuHandle) {
        some(slot) {
            let id = NEXT_ID
            NEXT_ID = NEXT_ID + 1
            let label = if shortcut.length > 0 {
                text + "\tCtrl+" + shortcut
            } else {
                text
            }
            AppendMenuW(
                slot.handle, MenuFlags.string as u32, id as u64,
                label.encodeUtf16()
            )
            ITEM_ACTIONS[id] = onClick
            ITEM_MENU[id] = slot.handle
            id as i64
        }
        none { 0 as i64 }
    }
}

pub fn menuAddCheckItem(
    menuHandle: i64, text: string, initialChecked: bool, onToggle: fn(bool)
): i64 {
    match MENUS.get(menuHandle) {
        some(slot) {
            let id = NEXT_ID
            NEXT_ID = NEXT_ID + 1
            let flags: u32 = (MenuFlags.string as u32)
                | (if initialChecked { 0x8 as u32 } else { 0 as u32 })
            AppendMenuW(slot.handle, flags, id as u64, text.encodeUtf16())
            CHECK_ACTIONS[id] = onToggle
            ITEM_CHECKED[id] = initialChecked
            ITEM_MENU[id] = slot.handle
            id as i64
        }
        none { 0 as i64 }
    }
}

pub fn menuAddRadioItem(
    menuHandle: i64, text: string, groupName: string, onSelect: fn()
): i64 {
    match MENUS.get(menuHandle) {
        some(slot) {
            let id = NEXT_ID
            NEXT_ID = NEXT_ID + 1
            // MFT_RADIOCHECK is set by CheckMenuRadioItem on first
            // selection; appending as plain string is fine here.
            AppendMenuW(
                slot.handle, MenuFlags.string as u32, id as u64,
                text.encodeUtf16()
            )
            ITEM_ACTIONS[id] = onSelect
            RADIO_INFO[id] = new RadioInfo(menuHandle, groupName)
            ITEM_CHECKED[id] = false
            ITEM_MENU[id] = slot.handle
            let members = match slot.radioGroups.get(groupName) {
                some(xs) { xs }
                none {
                    let xs: i32[] = []
                    slot.radioGroups[groupName] = xs
                    xs
                }
            }
            members.push(id)
            id as i64
        }
        none { 0 as i64 }
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
            let flags = (MenuFlags.popup as u32) | (MenuFlags.string as u32)
            AppendMenuW(
                parent.handle, flags, child.handle as u64,
                child.title.encodeUtf16()
            )
        }
    }
}

pub fn menuItemSetEnabled(menuHandle: i64, itemHandle: i64, enabled: bool) {
    let id = itemHandle as i32
    if let some(hmenu) = ITEM_MENU.get(id) {
        // MF_BYCOMMAND (0x0) | MF_ENABLED (0x0) / MF_GRAYED (0x1)
        let flag: u32 = if enabled { 0 as u32 } else { 0x1 as u32 }
        EnableMenuItem(hmenu, id as u32, flag)
    }
}

pub fn menuItemSetChecked(menuHandle: i64, itemHandle: i64, checked: bool) {
    let id = itemHandle as i32
    ITEM_CHECKED[id] = checked
    if let some(hmenu) = ITEM_MENU.get(id) {
        let flag: u32 = if checked { 0x8 as u32 } else { 0 as u32 }
        CheckMenuItem(hmenu, id as u32, flag)
    }
}

pub fn menuPopup(menuHandle: i64, windowHandle: i64, x: f64, y: f64) {
    let hwnd = windowHwnd(windowHandle)
    if hwnd as i64 == 0 as i64 { return }
    if let some(slot) = MENUS.get(menuHandle) {
        // Caller passes client-area coordinates; TrackPopupMenuEx
        // wants screen coords. Convert via ClientToScreen.
        let pt = POINT { x: x as i32, y: y as i32 }
        let p: *POINT = &pt
        ClientToScreen(hwnd, p)
        // TPM_LEFTALIGN | TPM_TOPALIGN = 0x0000 | 0x0000
        TrackPopupMenuEx(
            slot.handle, 0 as u32, pt.x, pt.y, hwnd, 0 as *void
        )
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
            let flags = (MenuFlags.popup as u32) | (MenuFlags.string as u32)
            AppendMenuW(bar, flags, slot.handle as u64, slot.title.encodeUtf16())
        }
        i = i + 1
    }
    SetMenu(hwnd, bar)
    DrawMenuBar(hwnd)
}

} // @extern(C)
