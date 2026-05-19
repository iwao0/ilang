// Modal panels — NSAlert / NSSavePanel / NSOpenPanel plus the
// NSAlertStyle enum and the ModalResponse* / AlertFirstButtonReturn
// constant table.

use foundation { NSObject, NSString, NSURL, NSArray, nsstr, ilstr }

// NSAlertStyle — informational vs warning vs critical visual
// treatment of an NSAlert. AppKit picks an icon + colour based
// on the style; defaults to `warning`.
pub enum NSAlertStyle: i64 {
    warning      = 0
    informational = 1
    critical     = 2
}

// NSModalResponse constants — the `runModal()` return values
// used by NSSavePanel / NSOpenPanel.
pub const ModalResponseOK: i64 = 1
pub const ModalResponseCancel: i64 = 0

// NSAlert button-index constants — `runModal()` on an NSAlert
// returns `AlertFirstButtonReturn` + N where N is the 0-indexed
// position of the clicked button (first added → first return).
pub const AlertFirstButtonReturn: i64 = 1000
pub const AlertSecondButtonReturn: i64 = 1001
pub const AlertThirdButtonReturn: i64 = 1002

@extern(ObjC, "/System/Library/Frameworks/AppKit.framework/AppKit") {
    /// `NSAlert` — modal dialog with message text, an optional
    /// informative paragraph, an alert style, and one or more
    /// buttons. `runModal` returns the matching
    /// `cocoa.AlertFirstButtonReturn` + N constant for the button
    /// the user clicked. Buttons added via `addButtonWithTitle:`
    /// are ordered right-to-left in the dialog.
    @objc pub class NSAlert : NSObject {
        @objc("alloc")
        pub static alloc(): NSAlert
        @objc("init")
        pub init(): NSAlert

        @objc("setMessageText:")
        pub setMessageText(s: NSString)
        @objc("setInformativeText:")
        pub setInformativeText(s: NSString)
        @objc("setAlertStyle:")
        pub setAlertStyle(style: NSAlertStyle)
        @objc("addButtonWithTitle:")
        pub addButtonWithTitle(title: NSString): NSObject
        @objc("runModal")
        pub runModal(): i64
    }

    /// `NSSavePanel` — modal "save as" dialog. Configure with
    /// `setNameFieldStringValue:` etc., then `runModal()`. On
    /// `cocoa.ModalResponseOK`, read `URL()` for the chosen path.
    @objc pub class NSSavePanel : NSObject {
        @objc("savePanel")
        pub static savePanel(): NSSavePanel

        @objc("setTitle:")
        pub setTitle(s: NSString)
        @objc("setMessage:")
        pub setMessage(s: NSString)
        @objc("setPrompt:")
        pub setPrompt(s: NSString)
        @objc("setNameFieldStringValue:")
        pub setNameFieldStringValue(s: NSString)
        @objc("setDirectoryURL:")
        pub setDirectoryURL(u: NSURL)
        @objc("runModal")
        pub runModal(): i64
        @objc("URL")
        pub get URL(): NSURL
    }

    /// `NSOpenPanel` — modal "open" dialog. Toggle file / folder
    /// / multi-select via the setters, then `runModal()`. On
    /// `cocoa.ModalResponseOK`, read `URLs()` for the array of
    /// selections, or `URL()` for the single-select case.
    @objc pub class NSOpenPanel : NSSavePanel {
        @objc("openPanel")
        pub static openPanel(): NSOpenPanel

        @objc("setCanChooseFiles:")
        pub setCanChooseFiles(flag: bool)
        @objc("setCanChooseDirectories:")
        pub setCanChooseDirectories(flag: bool)
        @objc("setAllowsMultipleSelection:")
        pub setAllowsMultipleSelection(flag: bool)
        @objc("setResolvesAliases:")
        pub setResolvesAliases(flag: bool)
        @objc("URLs")
        pub get URLs(): NSArray
    }

    // ─── Convenience helpers ─────────────────────────────────────

    /// Simple modal alert. Two-button variant (OK + Cancel by
    /// default — override via the `buttons` array). Returns
    /// `cocoa.AlertFirstButtonReturn`, `…SecondButton…`, etc.
    pub fn runAlert(
        message: string,
        informative: string,
        style: NSAlertStyle
    ): i64 {
        let a = NSAlert.alloc().init()
        a.setMessageText(nsstr(message))
        a.setInformativeText(nsstr(informative))
        a.setAlertStyle(style)
        a.addButtonWithTitle(nsstr("OK"))
        a.runModal()
    }

    /// Open-file dialog for a single file. Returns the chosen
    /// path as an ilang string, or `none` if the user cancelled.
    pub fn runOpenFileDialog(): string? {
        let p = NSOpenPanel.openPanel()
        p.setCanChooseFiles(true)
        p.setCanChooseDirectories(false)
        p.setAllowsMultipleSelection(false)
        if p.runModal() != ModalResponseOK { return none }
        some(ilstr(p.URL.path))
    }

    /// Save-file dialog with a default filename. Returns the
    /// chosen path, or `none` if the user cancelled.
    pub fn runSaveFileDialog(suggestedName: string): string? {
        let p = NSSavePanel.savePanel()
        p.setNameFieldStringValue(nsstr(suggestedName))
        if p.runModal() != ModalResponseOK { return none }
        some(ilstr(p.URL.path))
    }
}
