/// System clipboard — text only.
///
///   gui.Clipboard.setText("hello")
///   let s = gui.Clipboard.getText()
///
/// macOS uses `NSPasteboard.generalPasteboard`; Windows uses
/// `OpenClipboard` + `CF_UNICODETEXT`; Linux uses the default
/// `GdkClipboard` of the default display.
///
/// `getText` returns `""` if the clipboard is empty or holds
/// non-text content. On Linux the cross-process readback is
/// async-only, so `getText` falls back to the value last written
/// from this process (Round 2 may wire a sync read through a
/// nested main loop).

use gui_impl { setClipboardText, getClipboardText }

pub class Clipboard {
    pub static setText(s: string) {
        setClipboardText(s)
    }
    pub static getText(): string {
        getClipboardText()
    }
}
