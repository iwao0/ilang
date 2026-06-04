/// Toolbar — macOS doesn't ship a flat docked toolbar primitive
/// that lays out plain NSButtons in a row, so we lay out one
/// ourselves: a flipped `ToolbarContainer` strip docked to the
/// top edge of the window's contentView, populated with the
/// `createNativeButton` pipeline (so the per-button click
/// plumbing already lives in `button.il`) and a thin separator
/// view. `ToolbarContainer.resizeSubviewsWithOldSize:` re-runs
/// the layout so flexible-space items stretch to absorb any new
/// width.

use cocoa {
    NSObject, NSView, NSButton, NSRect, NSSize, NSAutoresizingMask,
    makeRect, makeSize,
}
use core { WINDOWS }
use button {
    createNativeButton, setOnButtonClick, setButtonText, setButtonFrame,
}

const TOOLBAR_HEIGHT:   f64 = 36.0
const BUTTON_HEIGHT:    f64 = 28.0
const BUTTON_PADDING:   f64 = 16.0
const SEPARATOR_WIDTH:  f64 = 1.0
const SEPARATOR_HEIGHT: f64 = 20.0
const ITEM_GAP:         f64 = 6.0

const KIND_BUTTON:    i32 = 0
const KIND_SEPARATOR: i32 = 1
const KIND_FLEXIBLE:  i32 = 2

pub class ToolbarEntry {
    pub kind:     i32
    /// For button entries: the handle returned by
    /// `createNativeButton` (an NSButton pointer). For
    /// separators: the separator NSView pointer. `0` for
    /// flexible spaces.
    pub handle:   i64
    pub naturalW: f64
    pub init(kind: i32, handle: i64, naturalW: f64) {
        this.kind     = kind
        this.handle   = handle
        this.naturalW = naturalW
    }
}

pub class ToolbarSlot {
    pub container:   NSView
    pub entries:     ToolbarEntry[]
    pub winHandle:   i64
    pub onItemClick: fn(i32)
    pub init(container: NSView, entries: ToolbarEntry[], winHandle: i64) {
        this.container   = container
        this.entries     = entries
        this.winHandle   = winHandle
        this.onItemClick = toolbarNoOp
    }
}

fn toolbarNoOp(idx: i32) {}

pub let TOOLBARS = new Map<i64, ToolbarSlot>()
/// Button-handle → (toolbarHandle, item index) so the per-button
/// callback can route to the right Signal.
pub class ToolbarBtnInfo {
    pub toolbarHandle: i64
    pub index:         i32
    pub init(t: i64, i: i32) {
        this.toolbarHandle = t
        this.index         = i
    }
}
pub let TOOLBAR_BUTTONS = new Map<i64, ToolbarBtnInfo>()

/// Width AppKit picks for the title + bezel; cheaper to ask the
/// view itself than to estimate per-character.
fn buttonNaturalWidth(handle: i64): f64 {
    let btn = NSButton.__wrap_handle_unowned(handle)
    btn.sizeToFit()
    (btn as NSView).frame.size.width
}

fn layoutEntries(entries: ToolbarEntry[], totalWidth: f64) {
    let n = entries.length
    if n == 0 as i64 { return }
    let fixedSum = 0.0
    let flexCount = 0 as i64
    let i = 0 as i64
    while i < n {
        let e = entries[i]
        if e.kind == KIND_FLEXIBLE {
            flexCount = flexCount + 1 as i64
        } else {
            fixedSum = fixedSum + e.naturalW
        }
        i = i + 1
    }
    let gapTotal = ITEM_GAP * ((n - 1 as i64) as f64)
    let leftover = totalWidth - fixedSum - gapTotal
    if leftover < 0.0 { leftover = 0.0 }
    let flexEach = if flexCount > 0 as i64 { leftover / (flexCount as f64) } else { 0.0 }

    let cursorX = 0.0
    let j = 0 as i64
    while j < n {
        let e = entries[j]
        let w = if e.kind == KIND_FLEXIBLE { flexEach } else { e.naturalW }
        let h = if e.kind == KIND_BUTTON {
            BUTTON_HEIGHT
        } elif e.kind == KIND_SEPARATOR {
            SEPARATOR_HEIGHT
        } else {
            0.0
        }
        let y = (TOOLBAR_HEIGHT - h) / 2.0
        if e.kind == KIND_BUTTON {
            setButtonFrame(e.handle, cursorX, y, w, h)
        } elif e.kind == KIND_SEPARATOR {
            let v = NSView.__wrap_handle_unowned(e.handle)
            v.setFrame(makeRect(cursorX, y, w, h))
        }
        cursorX = cursorX + w + ITEM_GAP
        j = j + 1
    }
}

/// `NSView` subclass that re-runs the equal-flex layout every
/// time its own frame changes.
pub class ToolbarContainer : NSView {
    pub override resizeSubviewsWithOldSize(oldSize: NSSize) {
        if let some(slot) = TOOLBARS.get(this.handle) {
            layoutEntries(slot.entries, this.frame.size.width)
        }
    }
}

/// Filler `NSView` subclass used for separator entries. Empty
/// body (no overrides); the class only exists so we have an
/// alloc-able type — the cocoa NSView binding itself doesn't
/// expose `alloc`.
pub class ToolbarSeparator : NSView {
    pub isFlipped(): bool { false }
}

@extern(C) {

pub fn createNativeToolbar(
    windowHandle: i64, kinds: i32[], texts: string[]
): i64 {
    let winSlot = match WINDOWS.get(windowHandle) {
        some(s) { s }
        none    { return -1 }
    }
    let cv = winSlot.win.contentView
    let totalW = cv.frame.size.width

    ToolbarContainer.register()
    ToolbarSeparator.register()

    let container = ToolbarContainer.alloc().init()
    (container as NSView).setFrame(makeRect(0.0, 0.0, totalW, TOOLBAR_HEIGHT))
    (container as NSView).setAutoresizingMask(
        (NSAutoresizingMask.widthSizable as u64) |
        (NSAutoresizingMask.maxYMargin   as u64)
    )

    cv.addSubview(container as NSView)
    let containerView = container as NSView
    let containerHandle = containerView.handle

    let entries: ToolbarEntry[] = []
    let n = kinds.length
    let i = 0 as i64
    while i < n {
        let kind = kinds[i]
        if kind == KIND_BUTTON {
            // `createNativeButton` parents the NSButton under the
            // resolved parent view; `resolveParentView` falls back
            // to wrapping the handle as a raw NSView, which is
            // exactly the splitter / container case.
            let btnHandle = createNativeButton(
                containerHandle, texts[i],
                0.0, 0.0, 100.0, BUTTON_HEIGHT,
                true, true, false
            )
            let natW = buttonNaturalWidth(btnHandle)
            entries.push(new ToolbarEntry(kind, btnHandle, natW))
        } elif kind == KIND_SEPARATOR {
            let v = ToolbarSeparator.alloc().init()
            (v as NSView).setFrame(makeRect(0.0, 0.0, SEPARATOR_WIDTH, SEPARATOR_HEIGHT))
            containerView.addSubview(v as NSView)
            entries.push(new ToolbarEntry(kind, (v as NSView).handle, SEPARATOR_WIDTH))
        } else {
            // Flexible space — no native peer, layout-only entry.
            entries.push(new ToolbarEntry(kind, 0 as i64, 0.0))
        }
        i = i + 1
    }

    let handle = containerHandle
    TOOLBARS[handle] = new ToolbarSlot(containerView, entries, windowHandle)

    // Now that the toolbar slot exists, wire each button's
    // onButtonClick → onItemClick (looking the item index up
    // through TOOLBAR_BUTTONS).
    let j = 0 as i64
    while j < n {
        let entry = entries[j]
        if entry.kind == KIND_BUTTON {
            TOOLBAR_BUTTONS[entry.handle] = new ToolbarBtnInfo(handle, j as i32)
            setOnButtonClick(entry.handle,
                fn(x: f64, y: f64, b: i32, ck: bool, sk: bool, ak: bool, mk: bool) {
                    if let some(info) = TOOLBAR_BUTTONS.get(entry.handle) {
                        if let some(slot) = TOOLBARS.get(info.toolbarHandle) {
                            let cb = slot.onItemClick
                            cb(info.index)
                        }
                    }
                })
        }
        j = j + 1
    }

    layoutEntries(entries, totalW)
    handle
}

pub fn setToolbarItemTitle(handle: i64, index: i32, text: string) {
    if let some(slot) = TOOLBARS.get(handle) {
        let i = index as i64
        if i < 0 as i64 || i >= slot.entries.length { return }
        let e = slot.entries[i]
        if e.kind != KIND_BUTTON { return }
        setButtonText(e.handle, text)
        e.naturalW = buttonNaturalWidth(e.handle)
        layoutEntries(slot.entries, slot.container.frame.size.width)
    }
}

pub fn setOnToolbarItemClick(handle: i64, cb: fn(i32)) {
    if let some(slot) = TOOLBARS.get(handle) {
        slot.onItemClick = cb
    }
}

} // @extern(C)
