/// System-tray icon — macOS NSStatusItem in the menu bar's
/// right-hand status area. Click + menu plumbing rides through
/// the standard NSStatusBarButton target/action pair; the click
/// callback only fires when no `NSMenu` is attached (with a
/// menu installed AppKit's own behaviour pops the menu and
/// suppresses the action message).

use cocoa {
    NSObject, NSView, NSImage, NSButton, NSMenu, NSString,
    NSStatusBar, NSStatusItem,
    StatusItemVariableLength,
    nsstr, ilstr, nil, selectorPtr,
}

pub class TrayIconSlot {
    pub item:     NSStatusItem
    pub onClick:  fn()
    pub init(item: NSStatusItem) {
        this.item    = item
        this.onClick = trayNoOp
    }
}

fn trayNoOp() {}

pub let TRAY_ICONS = new Map<i64, TrayIconSlot>()

/// `changeFont:` analogue — a single-method NSObject subclass
/// that funnels every NSStatusItem button click back into the
/// matching slot's `onClick`.
pub class TrayClickHandler : NSObject {
    pub trayClicked(sender: NSObject) {
        if let some(slot) = TRAY_ICONS.get(sender.handle) {
            slot.onClick()
        }
    }
}

@extern(C) {

fn loadImage(path: string): NSImage {
    if path == "" {
        return NSImage.__wrap_handle_unowned(0 as i64)
    }
    NSImage.alloc().initWithContentsOfFile(nsstr(path))
}

pub fn createNativeTrayIcon(
    iconPath: string, tooltip: string, menuHandle: i64
): i64 {
    let bar = NSStatusBar.systemStatusBar()
    let item = bar.statusItemWithLength(StatusItemVariableLength)
    let btn = NSButton.__wrap_handle_unowned(item.button.handle)

    if iconPath != "" {
        btn.setImage(loadImage(iconPath))
    }
    if tooltip != "" {
        (btn as NSView).setToolTip(nsstr(tooltip) as NSObject)
    }
    if menuHandle >= 0 as i64 {
        let menu = NSMenu.__wrap_handle_unowned(menuHandle)
        item.menu = menu as NSObject
    }

    let handle = (btn as NSObject).handle
    TRAY_ICONS[handle] = new TrayIconSlot(item)

    TrayClickHandler.register()
    let handler = TrayClickHandler.alloc().init()
    btn.setTarget(handler as NSObject)
    btn.setAction(selectorPtr("trayClicked:"))

    handle
}

pub fn setTrayIconIcon(handle: i64, path: string) {
    if let some(slot) = TRAY_ICONS.get(handle) {
        let btn = NSButton.__wrap_handle_unowned(handle)
        btn.setImage(loadImage(path))
    }
}

pub fn setTrayIconTitle(handle: i64, text: string) {
    if let some(slot) = TRAY_ICONS.get(handle) {
        let btn = NSButton.__wrap_handle_unowned(handle)
        btn.setTitle(nsstr(text))
    }
}

pub fn setTrayIconTooltip(handle: i64, text: string) {
    if let some(slot) = TRAY_ICONS.get(handle) {
        let btn = NSButton.__wrap_handle_unowned(handle)
        let tip: NSObject = if text == "" {
            nil as NSObject
        } else {
            nsstr(text) as NSObject
        }
        (btn as NSView).setToolTip(tip)
    }
}

pub fn setTrayIconMenu(handle: i64, menuHandle: i64) {
    if let some(slot) = TRAY_ICONS.get(handle) {
        if menuHandle < 0 as i64 {
            slot.item.menu = nil as NSObject
        } else {
            let menu = NSMenu.__wrap_handle_unowned(menuHandle)
            slot.item.menu = menu as NSObject
        }
    }
}

pub fn showTrayIcon(handle: i64, visible: bool) {
    if let some(slot) = TRAY_ICONS.get(handle) {
        slot.item.isVisible = visible
    }
}

pub fn removeNativeTrayIcon(handle: i64) {
    if let some(slot) = TRAY_ICONS.get(handle) {
        let bar = NSStatusBar.systemStatusBar()
        bar.removeStatusItem(slot.item)
        TRAY_ICONS.delete(handle)
    }
}

pub fn setOnTrayIconClick(handle: i64, cb: fn()) {
    if let some(slot) = TRAY_ICONS.get(handle) {
        slot.onClick = cb
    }
}

} // @extern(C)
