/// Modal dialogs (alert / confirm / openFile / saveFile).
///
/// Each dialog runs synchronously inside `runModal` — the calling
/// thread blocks until the user dismisses it — and invokes the
/// supplied callback right before this function returns. From the
/// user's side the API still reads as "fire-and-forget with
/// callback"; the synchronous modal is just an implementation
/// detail that keeps the binding surface small (no ObjCBlock
/// completion-handler plumbing).

use cocoa {
    NSObject, NSView, NSWindow, NSAlert, NSOpenPanel, NSSavePanel,
    NSURL, NSArray, NSNotification,
    NSAlertStyle, ModalResponseOK, AlertFirstButtonReturn,
    NSColor, NSColorWell, NSFont, NSFontManager, NSFontPanel,
    NSFontDescriptor, NSFontSymbolicTraits, NSApplication,
    NSRect, nil, nsstr, ilstr, makeRect,
}
use core { WINDOWS }

pub fn showAlertDialog(windowHandle: i64, text: string, callback: fn()) {
    if let some(_slot) = WINDOWS.get(windowHandle) {
        let a = NSAlert.alloc().init()
        a.setMessageText(nsstr(text))
        a.setAlertStyle(NSAlertStyle.informational)
        a.addButtonWithTitle(nsstr("OK"))
        let _ = a.runModal()
        callback()
    }
}

pub fn showConfirmDialog(windowHandle: i64, text: string, callback: fn(bool)) {
    if let some(_slot) = WINDOWS.get(windowHandle) {
        let a = NSAlert.alloc().init()
        a.setMessageText(nsstr(text))
        a.setAlertStyle(NSAlertStyle.informational)
        // Order matters — NSAlert paints buttons right-to-left,
        // so the first-added one (OK) lands on the right.
        a.addButtonWithTitle(nsstr("OK"))
        a.addButtonWithTitle(nsstr("Cancel"))
        let r = a.runModal()
        callback(r == AlertFirstButtonReturn)
    }
}

pub fn showOpenFileDialog(windowHandle: i64, callback: fn(string)) {
    if let some(_slot) = WINDOWS.get(windowHandle) {
        let p = NSOpenPanel.openPanel()
        p.setCanChooseFiles(true)
        p.setCanChooseDirectories(false)
        p.setAllowsMultipleSelection(false)
        if p.runModal() != ModalResponseOK {
            callback("")
        } else {
            callback(ilstr(p.URL.path))
        }
    }
}

pub fn showSaveFileDialog(
    windowHandle: i64, suggestedName: string, callback: fn(string)
) {
    if let some(_slot) = WINDOWS.get(windowHandle) {
        let p = NSSavePanel.savePanel()
        if suggestedName != "" {
            p.setNameFieldStringValue(nsstr(suggestedName))
        }
        if p.runModal() != ModalResponseOK {
            callback("")
        } else {
            callback(ilstr(p.URL.path))
        }
    }
}

/// Map a `gui.MessageIcon` int (info=0 / warning=1 / error=2 /
/// question=3) onto the closest `NSAlertStyle`. AppKit only has
/// three styles, so warning + question collapse to `warning`.
fn iconToAlertStyle(icon: i32): NSAlertStyle {
    if icon == 1 as i32 { return NSAlertStyle.warning }
    if icon == 2 as i32 { return NSAlertStyle.critical }
    if icon == 3 as i32 { return NSAlertStyle.warning }
    NSAlertStyle.informational
}

pub fn showMessageDialog(
    windowHandle: i64, text: string, buttons: string[], icon: i32,
    callback: fn(i32)
) {
    if let some(_slot) = WINDOWS.get(windowHandle) {
        let a = NSAlert.alloc().init()
        a.setMessageText(nsstr(text))
        a.setAlertStyle(iconToAlertStyle(icon))
        if buttons.length == 0 {
            // Defensive: an empty list is meaningless; fall back
            // to a single OK so the dialog can still close.
            a.addButtonWithTitle(nsstr("OK"))
            let _ = a.runModal()
            callback(0 as i32)
            return
        }
        let i = 0
        while i < buttons.length {
            a.addButtonWithTitle(nsstr(buttons[i]))
            i = i + 1
        }
        let r = a.runModal()
        // NSAlert returns AlertFirstButtonReturn + N for button N.
        callback((r - AlertFirstButtonReturn) as i32)
    }
}

fn argbToNSColor(argb: u32): NSColor {
    let mask: u32 = 0xFF as u32
    let a = (((argb >> 24) & mask) as f64) / 255.0
    let r = (((argb >> 16) & mask) as f64) / 255.0
    let g = (((argb >>  8) & mask) as f64) / 255.0
    let b = (( argb        & mask) as f64) / 255.0
    NSColor.rgba(r, g, b, a)
}

fn nsColorToArgb(c: NSColor): u32 {
    // `NSColor` from rgba(…) lives in the sRGB colour space and
    // exposes 0..1 components directly; clamp + cast to byte.
    let r = (c.redComponent   * 255.0) as u32
    let g = (c.greenComponent * 255.0) as u32
    let b = (c.blueComponent  * 255.0) as u32
    let a = (c.alphaComponent * 255.0) as u32
    (a << 24) | (r << 16) | (g << 8) | b
}

pub fn showColorPickerDialog(
    windowHandle: i64, initial: u32, callback: fn(u32, bool)
) {
    if let some(_slot) = WINDOWS.get(windowHandle) {
        let a = NSAlert.alloc().init()
        a.setMessageText(nsstr("Choose a color"))
        a.setAlertStyle(NSAlertStyle.informational)
        a.addButtonWithTitle(nsstr("OK"))
        a.addButtonWithTitle(nsstr("Cancel"))
        // 60×30 colour swatch — clicking it opens the system
        // colour picker. Final colour reads back from `well.color`.
        let well = NSColorWell.alloc().init(makeRect(0.0, 0.0, 60.0, 30.0))
        well.setColor(argbToNSColor(initial))
        a.setAccessoryView(well as NSView)
        let r = a.runModal()
        let accepted = r == AlertFirstButtonReturn
        if accepted {
            callback(nsColorToArgb(well.color), true)
        } else {
            callback(initial, false)
        }
    }
}

/// Rebuild an `NSFont` for the requested family / size / weight /
/// italic — same logic the canvas backend uses, kept inline here
/// so dialogs don't reach into `canvas.il`.
fn fontFor(family: string, size: f64, weight: i32, italic: bool): NSFont {
    let base = NSFont.fontWithName(nsstr(family), size)
    let resolved = if base.handle == 0 as i64 {
        NSFont.systemFontOfSize(size)
    } else {
        base
    }
    let traits: u32 =
        (if weight >= 600 as i32 { NSFontSymbolicTraits.bold as u32 } else { 0 as u32 }) |
        (if italic { NSFontSymbolicTraits.italic as u32 } else { 0 as u32 })
    if traits == 0 as u32 { return resolved }
    let desc = resolved.fontDescriptor.fontDescriptorWithSymbolicTraits(traits)
    let withTraits = NSFont.fontWithDescriptor(desc, size)
    if withTraits.handle == 0 as i64 { resolved } else { withTraits }
}

/// `windowWillClose:` from the font panel's window delegate
/// breaks out of `runModalForWindow:` so the picker call returns
/// the instant the user closes the panel.
pub class FontPickerClose : NSObject {
    pub windowWillClose(n: NSNotification) {
        NSApplication.sharedApplication().stopModal()
    }
}

/// macOS font picker. Surfaces the system `NSFontPanel` as a
/// modal window; closing the panel returns the font the user
/// picked. There is no Cancel surface on this backend — closing
/// the panel always counts as confirmation — so `accepted` is
/// always `true`.
pub fn showFontPickerDialog(
    windowHandle: i64,
    family: string, size: f64, weight: i32, italic: bool,
    callback: fn(string, f64, i32, bool, bool)
) {
    if let some(_slot) = WINDOWS.get(windowHandle) {
        let initial = fontFor(family, size, weight, italic)
        FontPickerClose.register()
        let closer = FontPickerClose.alloc().init()

        let panel = NSFontPanel.sharedFontPanel()
        panel.setPanelFont(initial, false)
        let panelWin = panel as NSWindow
        panelWin.setDelegate(closer as NSObject)
        panelWin.makeKeyAndOrderFront(nil as NSObject)

        let app = NSApplication.sharedApplication()
        let _ = app.runModalForWindow(panelWin)

        panelWin.setDelegate(nil as NSObject)

        // Read the final font straight off the panel: panelConvertFont:
        // applies its current UI selection to the seed font.
        let picked = panel.panelConvertFont(initial)
        let traits = picked.fontDescriptor.symbolicTraits
        let boldBit:   u32 = NSFontSymbolicTraits.bold   as u32
        let italicBit: u32 = NSFontSymbolicTraits.italic as u32
        let newWeight: i32 = if (traits & boldBit) != 0 as u32 {
            700 as i32
        } else {
            400 as i32
        }
        let newItalic: bool = (traits & italicBit) != 0 as u32
        callback(
            ilstr(picked.familyName), picked.pointSize,
            newWeight, newItalic, true
        )
    }
}
