/// Workspace + pasteboard — NSPasteboard / NSWorkspace plus the
/// PasteboardTypeString UTI constant.

use foundation { NSObject, NSString, NSURL, NSPoint, nsstr, ilstr }

/// Standard UTI for plain UTF-8 text on the pasteboard. Pass to
/// `pasteboardCopy` / `setString(forType:)`.
pub const PasteboardTypeString: string = "public.utf8-plain-text"

/// Legacy "list of NSString paths" pasteboard type. Still
/// honoured by Finder drag sources on current macOS; reading via
/// `propertyListForType:` returns an `NSArray` of `NSString`
/// absolute paths.
pub const FilenamesPboardType: string = "NSFilenamesPboardType"

@extern(ObjC, "/System/Library/Frameworks/AppKit.framework/AppKit") {
    /// `NSPasteboard` — system clipboard. `generalPasteboard`
    /// is the standard "copy / paste" surface; per-type APIs
    /// (`stringForType:` / `setString:forType:`) read or write
    /// individual representations. `clearContents` resets the
    /// pasteboard's change count and removes all data.
    @objc pub class NSPasteboard : NSObject {
        @objc("generalPasteboard")
        pub static generalPasteboard(): NSPasteboard
        /// Create a private pasteboard with a unique name. Doesn't
        /// touch the user-facing system clipboard — use this for
        /// tests and any in-process copy/paste plumbing.
        @objc("pasteboardWithUniqueName")
        pub static pasteboardWithUniqueName(): NSPasteboard

        @objc("clearContents")
        pub clearContents(): i64
        @objc("setString:forType:")
        pub setString(s: NSString, forType: NSString): bool
        @objc("stringForType:")
        pub stringForType(t: NSString): NSString
        /// Decode the data for `type` as a Cocoa property list and
        /// return the root. For `FilenamesPboardType` this is an
        /// `NSArray` of `NSString` absolute paths (cast to
        /// `NSArray` via `NSArray.__wrap_handle_unowned`).
        @objc("propertyListForType:")
        pub propertyListForType(t: NSString): NSObject
    }

    /// `NSDraggingInfo` is an `@protocol`, not a class — every
    /// argument typed `id<NSDraggingInfo>` is actually some
    /// concrete AppKit object that implements the protocol's
    /// selectors. ilang binds the surface as a regular `@objc`
    /// class so callers can dispatch by name; rebind any
    /// `NSObject` sender that arrives in a dragging selector via
    /// `NSDraggingInfo.__wrap_handle_unowned(sender.handle)`.
    @objc pub class NSDraggingInfo : NSObject {
        @objc("draggingPasteboard")
        pub get draggingPasteboard(): NSPasteboard
        /// Window-coordinate location of the cursor during the
        /// drag. Convert into view coordinates with
        /// `view.convertPointFromView(p, nil)`.
        @objc("draggingLocation")
        pub get draggingLocation(): NSPoint
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
