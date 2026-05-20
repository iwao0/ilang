/// Workspace + pasteboard — NSPasteboard / NSWorkspace plus the
/// PasteboardTypeString UTI constant.

use foundation { NSObject, NSString, NSURL, nsstr, ilstr }

/// Standard UTI for plain UTF-8 text on the pasteboard. Pass to
/// `pasteboardCopy` / `setString(forType:)`.
pub const PasteboardTypeString: string = "public.utf8-plain-text"

@extern(ObjC, "/System/Library/Frameworks/AppKit.framework/AppKit") {
    /// `NSPasteboard` — system clipboard. `generalPasteboard`
    /// is the standard "copy / paste" surface; per-type APIs
    /// (`stringForType:` / `setString:forType:`) read or write
    /// individual representations. `clearContents` resets the
    /// pasteboard's change count and removes all data.
    @objc pub class NSPasteboard : NSObject {
        @objc("generalPasteboard")
        pub static generalPasteboard(): NSPasteboard

        @objc("clearContents")
        pub clearContents(): i64
        @objc("setString:forType:")
        pub setString(s: NSString, forType: NSString): bool
        @objc("stringForType:")
        pub stringForType(t: NSString): NSString
    }

    /// `NSWorkspace` — Finder / Launch-Services bridge. Use
    /// `openURL:` to hand a URL off to the default handler app,
    /// or `openFile:` to open a filesystem path in its default
    /// application.
    @objc pub class NSWorkspace : NSObject {
        @objc("sharedWorkspace")
        pub static sharedWorkspace(): NSWorkspace

        @objc("openURL:")
        pub openURL(u: NSURL): bool
        @objc("openFile:")
        pub openFile(path: NSString): bool
    }

    /// Copy `s` to the general pasteboard as UTF-8 plain text.
    pub fn pasteboardCopy(s: string) {
        let pb = NSPasteboard.generalPasteboard()
        pb.clearContents()
        pb.setString(nsstr(s), nsstr(PasteboardTypeString))
    }

    /// Read the current UTF-8 plain-text contents of the general
    /// pasteboard, if any.
    pub fn pasteboardPaste(): string? {
        let pb = NSPasteboard.generalPasteboard()
        let v = pb.stringForType(nsstr(PasteboardTypeString))
        if v.handle == 0 { return none }
        some(ilstr(v))
    }

    /// Open `url` in its registered default application via
    /// LaunchServices (`http(s)://`, `mailto:`, `file://`, …).
    pub fn openURL(url: string) {
        let ws = NSWorkspace.sharedWorkspace()
        ws.openURL(NSURL.fromString(nsstr(url)))
    }
}
