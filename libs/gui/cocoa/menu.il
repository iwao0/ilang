/// macOS menu backend. NSMenu / NSMenuItem wrappers behind the
/// gui-core `Menu` class. Each ilang Menu is a single NSMenu;
/// `gui.Window.setMenuBar([...])` wraps them in a top-level NSMenu
/// installed as the app's main menu via `NSApplication.setMainMenu`.

use cocoa {
    NSObject, NSView, NSApplication, NSMenu, NSMenuItem,
    NSPoint,
    nsstr, makePoint, selectorPtr, sharedApplication, nil,
}
use core { WINDOWS }

/// Per-ilang-Menu state. Stored in `MENUS` keyed by the NSMenu's
/// raw ObjC pointer.
class MenuSlot {
    pub menu:  NSMenu
    pub title: string
    /// Radio-group members, keyed by group name. Selecting one
    /// clears the state of every other entry in the group.
    pub radioGroups: Map<string, i64[]>
    pub init(menu: NSMenu, title: string) {
        this.menu = menu
        this.title = title
        this.radioGroups = new Map<string, i64[]>()
    }
}

let MENUS = new Map<i64, MenuSlot>()

/// Per-NSMenuItem callback table. The shared `MenuActionHandler`
/// looks up the entry by `sender.handle` and invokes it.
let ITEM_ACTIONS = new Map<i64, fn()>()
/// `addCheckItem` callbacks — receive the new checked state on
/// each toggle.
let CHECK_ACTIONS = new Map<i64, fn(bool)>()
/// Tracks the current check / radio state per item so the
/// MenuActionHandler can flip it before invoking the user
/// callback (NSMenuItem.state stays the source of truth on the
/// AppKit side, but we mirror it here to feed `onToggle(new)`).
let ITEM_CHECKED = new Map<i64, bool>()
/// Maps a radio item's handle back to its (menuHandle, groupName)
/// so the action handler can clear sibling radios on selection.
class RadioInfo {
    pub menuHandle: i64
    pub group:      string
    pub init(menuHandle: i64, group: string) {
        this.menuHandle = menuHandle
        this.group      = group
    }
}
let RADIO_INFO = new Map<i64, RadioInfo>()

/// Shared target. The same instance is reused as the `target` of
/// every menu item so AppKit's `setAction:` dispatch resolves
/// through one ObjC class. Allocated lazily on the first menu
/// item add.
class MenuActionHandler : NSObject {
    pub menuFired(sender: NSObject) {
        let h = sender.handle
        // Plain command item — just fire the callback.
        if let some(cb) = ITEM_ACTIONS.get(h) {
            cb()
            return
        }
        // Check item — toggle the mirror + native state, then
        // invoke the user callback with the new value.
        if let some(cb) = CHECK_ACTIONS.get(h) {
            let prev = match ITEM_CHECKED.get(h) {
                some(b) { b }
                none    { false }
            }
            let next = !prev
            ITEM_CHECKED[h] = next
            let item = NSMenuItem.__wrap_handle_unowned(h)
            item.setState(if next { 1 as i64 } else { 0 as i64 })
            cb(next)
            return
        }
        // Radio item — clear siblings in the same group, mark
        // this one selected, then fire the user callback.
        if let some(info) = RADIO_INFO.get(h) {
            if let some(menuSlot) = MENUS.get(info.menuHandle) {
                match menuSlot.radioGroups.get(info.group) {
                    some(members) {
                        let n = members.length
                        let i = 0
                        while i < n {
                            let m = members[i]
                            ITEM_CHECKED[m] = false
                            let it = NSMenuItem.__wrap_handle_unowned(m)
                            it.setState(0 as i64)
                            i = i + 1
                        }
                    }
                    none {}
                }
            }
            ITEM_CHECKED[h] = true
            let item = NSMenuItem.__wrap_handle_unowned(h)
            item.setState(1 as i64)
            if let some(cb) = ITEM_ACTIONS.get(h) { cb() }
        }
    }
}

let HANDLER_HOLDER: NSObject? = none

fn handler(): NSObject {
    match HANDLER_HOLDER {
        some(h) { h }
        none {
            MenuActionHandler.register()
            let h = MenuActionHandler.alloc().init() as NSObject
            HANDLER_HOLDER = some(h)
            h
        }
    }
}

pub fn createNativeMenu(title: string): i64 {
    let m = NSMenu.alloc().initWithTitle(nsstr(title))
    let h = m.handle
    MENUS[h] = new MenuSlot(m, title)
    h
}

pub fn menuAddItem(menuHandle: i64, text: string, onClick: fn()): i64 {
    menuAddItemWithShortcut(menuHandle, text, "", onClick)
}

pub fn menuAddItemWithShortcut(
    menuHandle: i64, text: string, shortcut: string, onClick: fn()
): i64 {
    match MENUS.get(menuHandle) {
        some(slot) {
            let item = slot.menu.addItemWithTitle(
                nsstr(text), selectorPtr("menuFired:"), nsstr(shortcut.toLower())
            )
            item.setTarget(handler())
            ITEM_ACTIONS[item.handle] = onClick
            item.handle
        }
        none { 0 as i64 }
    }
}

pub fn menuAddCheckItem(
    menuHandle: i64, text: string, initialChecked: bool, onToggle: fn(bool)
): i64 {
    match MENUS.get(menuHandle) {
        some(slot) {
            let item = slot.menu.addItemWithTitle(
                nsstr(text), selectorPtr("menuFired:"), nsstr("")
            )
            item.setTarget(handler())
            item.setState(if initialChecked { 1 as i64 } else { 0 as i64 })
            CHECK_ACTIONS[item.handle] = onToggle
            ITEM_CHECKED[item.handle] = initialChecked
            item.handle
        }
        none { 0 as i64 }
    }
}

pub fn menuAddRadioItem(
    menuHandle: i64, text: string, groupName: string, onSelect: fn()
): i64 {
    match MENUS.get(menuHandle) {
        some(slot) {
            let item = slot.menu.addItemWithTitle(
                nsstr(text), selectorPtr("menuFired:"), nsstr("")
            )
            item.setTarget(handler())
            ITEM_ACTIONS[item.handle] = onSelect
            RADIO_INFO[item.handle] = new RadioInfo(menuHandle, groupName)
            ITEM_CHECKED[item.handle] = false
            // Track membership so MenuActionHandler can clear
            // siblings when this row is selected.
            let members = match slot.radioGroups.get(groupName) {
                some(xs) { xs }
                none {
                    let xs: i64[] = []
                    slot.radioGroups[groupName] = xs
                    xs
                }
            }
            members.push(item.handle)
            item.handle
        }
        none { 0 as i64 }
    }
}

pub fn menuAddSeparator(menuHandle: i64) {
    if let some(slot) = MENUS.get(menuHandle) {
        slot.menu.addItem(NSMenuItem.separatorItem())
    }
}

pub fn menuAddSubmenu(parentHandle: i64, childHandle: i64) {
    if let some(parent) = MENUS.get(parentHandle) {
        if let some(child) = MENUS.get(childHandle) {
            let item = NSMenuItem.alloc().init(
                nsstr(child.title), 0 as i64, nsstr("")
            )
            item.setSubmenu(child.menu)
            parent.menu.addItem(item)
        }
    }
}

pub fn menuItemSetEnabled(menuHandle: i64, itemHandle: i64, enabled: bool) {
    let item = NSMenuItem.__wrap_handle_unowned(itemHandle)
    item.setEnabled(enabled)
}

pub fn menuItemSetChecked(menuHandle: i64, itemHandle: i64, checked: bool) {
    ITEM_CHECKED[itemHandle] = checked
    let item = NSMenuItem.__wrap_handle_unowned(itemHandle)
    item.setState(if checked { 1 as i64 } else { 0 as i64 })
}

pub fn menuPopup(menuHandle: i64, windowHandle: i64, x: f64, y: f64) {
    if let some(slot) = MENUS.get(menuHandle) {
        if let some(winSlot) = WINDOWS.get(windowHandle) {
            let view = winSlot.win.contentView
            // `nil` for the positioning item — anchor at `location`.
            let _ = slot.menu.popUpMenuPositioningItem(
                NSMenuItem.__wrap_handle_unowned(0 as i64),
                makePoint(x, y), view
            )
        }
    }
}

/// Install `menuHandles` as the app-wide menu bar. macOS shows
/// the main menu in the screen menu bar, not per-window — the
/// `windowHandle` is accepted for API symmetry with win32 but
/// ignored here.
pub fn setWindowMenuBar(windowHandle: i64, menuHandles: i64[]) {
    let bar = NSMenu.alloc().init()
    let n = menuHandles.length
    let i = 0
    while i < n {
        if let some(slot) = MENUS.get(menuHandles[i]) {
            let item = NSMenuItem.alloc().init(
                nsstr(slot.title), 0 as i64, nsstr("")
            )
            item.setSubmenu(slot.menu)
            bar.addItem(item)
        }
        i = i + 1
    }
    sharedApplication().setMainMenu(bar as NSObject)
}
