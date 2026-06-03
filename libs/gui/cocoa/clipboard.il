/// System clipboard — `NSPasteboard.generalPasteboard`. Wraps
/// the existing `pasteboardCopy` / `pasteboardPaste` helpers
/// from the cocoa bindings.

use cocoa { pasteboardCopy, pasteboardPaste }

pub fn setClipboardText(s: string) {
    pasteboardCopy(s)
}
pub fn getClipboardText(): string {
    match pasteboardPaste() {
        some(s) { s }
        none    { "" }
    }
}
