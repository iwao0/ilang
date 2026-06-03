/// Tab strip — `NSTabView` with one `NSTabViewItem` per tab.
/// The item's content view is left at AppKit's default (an
/// empty NSView). The public class surface only exposes the tab
/// selection events — content management lives on the caller's
/// side.

use cocoa {
    NSObject, NSView, NSWindow,
    NSTabView, NSTabViewItem, NSTabViewDelegate,
    NSString,
    nsstr, ilstr, makeRect, nil,
}
use core { WINDOWS, noOp1, noOpI32 }

class TabViewSlot {
    pub tabView:   NSTabView
    pub delegate:  NSObject
    pub onChange:  fn(i32)
    pub items:     i64[]   // ordered tab item ObjC pointers
    pub init(tabView: NSTabView, delegate: NSObject) {
        this.tabView  = tabView
        this.delegate = delegate
        this.onChange = noOpI32
        this.items    = []
    }
}

let TAB_VIEWS = new Map<i64, TabViewSlot>()
/// item-pointer → owning tabview handle. Used by the delegate
/// to look up the right slot from the AppKit `didSelectTabViewItem:`
/// argument.
let ITEM_TO_TABVIEW = new Map<i64, i64>()

class TabViewHandler : NSObject {
    pub tabViewDidSelectTabViewItem(tv: NSObject, item: NSObject) {
        if let some(viewH) = ITEM_TO_TABVIEW.get(item.handle) {
            if let some(slot) = TAB_VIEWS.get(viewH) {
                // Look the item's index up in our ordered list.
                let n = slot.items.length
                let i = 0
                while i < n {
                    if slot.items[i] == item.handle {
                        let cb = slot.onChange
                        cb(i as i32)
                        return
                    }
                    i = i + 1
                }
            }
        }
    }
}

pub fn createNativeTabView(
    windowHandle: i64,
    x: f64, y: f64, width: f64, height: f64,
    enabled: bool, visible: bool
): i64 {
    let winSlot = match WINDOWS.get(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let tv = NSTabView.alloc().init(makeRect(x, y, width, height))
    (tv as NSView).setHidden(!visible)

    TabViewHandler.register()
    let dg = TabViewHandler.alloc().init()
    tv.setDelegate(dg as NSObject)

    winSlot.win.contentView.addSubview(tv as NSView)

    let handle = (tv as NSView).handle
    TAB_VIEWS[handle] = new TabViewSlot(tv, dg as NSObject)
    let _ = enabled   // NSTabView has no per-view enabled toggle
    handle
}

pub fn setTabViewFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {
    if let some(slot) = TAB_VIEWS.get(handle) {
        (slot.tabView as NSView).setFrame(makeRect(x, y, width, height))
    }
}
pub fn setTabViewEnabled(handle: i64, enabled: bool) {
    // NSTabView has no global setEnabled — per-tab disable lives
    // on NSTabViewItem (handled via tabItemSetEnabled below).
}
pub fn setTabViewVisible(handle: i64, visible: bool) {
    if let some(slot) = TAB_VIEWS.get(handle) {
        (slot.tabView as NSView).setHidden(!visible)
    }
}
pub fn setTabViewTooltip(handle: i64, text: string) {
    if let some(slot) = TAB_VIEWS.get(handle) {
        if text == "" {
            (slot.tabView as NSView).setToolTip(nil)
        } else {
            (slot.tabView as NSView).setToolTip(nsstr(text) as NSObject)
        }
    }
}

pub fn setOnTabViewChange(handle: i64, cb: fn(i32)) {
    if let some(slot) = TAB_VIEWS.get(handle) { slot.onChange = cb }
}

pub fn tabViewAddTab(handle: i64, title: string): i64 {
    match TAB_VIEWS.get(handle) {
        some(slot) {
            let item = NSTabViewItem.alloc().initWithIdentifier(
                nsstr("") as NSObject
            )
            item.label = nsstr(title)
            slot.tabView.addTabViewItem(item)
            let itemH = (item as NSObject).handle
            slot.items.push(itemH)
            ITEM_TO_TABVIEW[itemH] = handle
            itemH
        }
        none { 0 as i64 }
    }
}

pub fn tabViewRemoveTab(handle: i64, index: i32) {
    if let some(slot) = TAB_VIEWS.get(handle) {
        let n = slot.items.length
        let i = index as i64
        if i < 0 as i64 || i >= n { return }
        let itemH = slot.items[i]
        let item = NSTabViewItem.__wrap_handle_unowned(itemH)
        slot.tabView.removeTabViewItem(item)
        ITEM_TO_TABVIEW.delete(itemH)
        // Compact `items` — keep insertion-order semantics intact.
        let rest: i64[] = []
        let k = 0
        while k < n {
            if k != i { rest.push(slot.items[k]) }
            k = k + 1
        }
        slot.items = rest
    }
}

pub fn tabViewGetSelectedIndex(handle: i64): i32 {
    let result: i32 = -1 as i32
    if let some(slot) = TAB_VIEWS.get(handle) {
        let sel = slot.tabView.selectedTabViewItem
        if sel.handle == 0 as i64 { return result }
        let n = slot.items.length
        let i = 0
        while i < n {
            if slot.items[i] == sel.handle {
                result = i as i32
                return result
            }
            i = i + 1
        }
    }
    result
}
pub fn tabViewSetSelectedIndex(handle: i64, index: i32) {
    if let some(slot) = TAB_VIEWS.get(handle) {
        let n = slot.items.length
        let i = index as i64
        if i < 0 as i64 || i >= n { return }
        slot.tabView.selectTabViewItemAtIndex(i)
    }
}
pub fn tabViewGetTabCount(handle: i64): i32 {
    match TAB_VIEWS.get(handle) {
        some(slot) { slot.items.length as i32 }
        none       { 0 as i32 }
    }
}

pub fn tabItemSetTitle(viewHandle: i64, itemHandle: i64, title: string) {
    let item = NSTabViewItem.__wrap_handle_unowned(itemHandle)
    item.label = nsstr(title)
}
pub fn tabItemSetEnabled(viewHandle: i64, itemHandle: i64, enabled: bool) {
    // NSTabViewItem has no setEnabled in our binding — surface
    // omitted on macOS for now.
}
