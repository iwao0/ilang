/// System clipboard — default `GdkClipboard` of the default
/// display. `setText` writes synchronously through
/// `gdk_clipboard_set_text`. `getText` returns the value last
/// written from this process via `setText`; a true cross-process
/// readback needs `gdk_clipboard_read_text_async`, which is
/// async-only and would require nested-main-loop wrangling we
/// haven't taken on yet.

use std.ffi { cstrFromString }
use gtk {
    GdkClipboard, GdkDisplay,
    gdk_display_get_default, gdk_display_get_clipboard,
    gdk_clipboard_set_text,
}

let LAST_SET: string[] = [""]

@extern(C) {

pub fn setClipboardText(s: string) {
    let display = gdk_display_get_default()
    if display as i64 == 0 as i64 { return }
    let cb = gdk_display_get_clipboard(display)
    gdk_clipboard_set_text(cb, cstrFromString(s))
    LAST_SET[0] = s
}

pub fn getClipboardText(): string {
    LAST_SET[0]
}

} // @extern(C)
