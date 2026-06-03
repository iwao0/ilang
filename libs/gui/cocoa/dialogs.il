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
    NSObject, NSAlert, NSOpenPanel, NSSavePanel, NSURL, NSArray,
    NSAlertStyle, ModalResponseOK, AlertFirstButtonReturn,
    nsstr, ilstr,
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
