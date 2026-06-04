/// Modal dialogs — `GtkAlertDialog` / `GtkFileDialog` (the GTK
/// 4.10+ async API). Each ilang-side dialog call stashes the
/// user callback in `PENDING` keyed by the dialog handle, kicks
/// off the async choose/open/save, and dispatches the result
/// from the `GAsyncReadyCallback` shim into the stored callback.

use std.ffi { cstrFromString, stringFromCstr, readF32, writeF32 }
use gtk {
    GtkWindow, GtkAlertDialog, GtkFileDialog, GFile, GAsyncResult,
    GCancellable, GtkColorDialog, GtkFontDialog, PangoFontDescription,
    gtk_alert_dialog_new, gtk_alert_dialog_set_buttons,
    gtk_alert_dialog_set_buttons_array,
    gtk_alert_dialog_set_cancel_button, gtk_alert_dialog_set_default_button,
    gtk_alert_dialog_choose, gtk_alert_dialog_choose_finish,
    gtk_file_dialog_new, gtk_file_dialog_set_title, gtk_file_dialog_set_initial_name,
    gtk_file_dialog_open, gtk_file_dialog_open_finish,
    gtk_file_dialog_save, gtk_file_dialog_save_finish,
    gtk_color_dialog_new, gtk_color_dialog_choose_rgba,
    gtk_color_dialog_choose_rgba_finish, gdk_rgba_free,
    gtk_font_dialog_new, gtk_font_dialog_choose_font,
    gtk_font_dialog_choose_font_finish,
    pango_font_description_from_string, pango_font_description_free,
    pango_font_description_get_family, pango_font_description_get_size,
    pango_font_description_get_size_is_absolute,
    pango_font_description_get_weight, pango_font_description_get_style,
    g_file_get_path,
}
use core { WINDOWS }

/// Pending alert / confirm callback, keyed by the dialog's
/// GObject pointer. `mode = 0` = alert (no bool to caller),
/// `mode = 1` = confirm. The bool callback for confirm sees
/// `true` when the OK button (index 0) was picked.
class PendingAlert {
    pub mode:        i32
    pub onDismiss:   fn()
    pub onConfirm:   fn(bool)
    pub init(mode: i32, onDismiss: fn(), onConfirm: fn(bool)) {
        this.mode      = mode
        this.onDismiss = onDismiss
        this.onConfirm = onConfirm
    }
}
let PENDING_ALERTS = new Map<i64, PendingAlert>()

/// Pending file callback. `mode = 0` = open, `mode = 1` = save.
class PendingFile {
    pub mode:     i32
    pub callback: fn(string)
    pub init(mode: i32, callback: fn(string)) {
        this.mode     = mode
        this.callback = callback
    }
}
let PENDING_FILES = new Map<i64, PendingFile>()

/// Pending message-dialog callback. Holds the user-facing
/// `fn(i32)` keyed by the GtkAlertDialog pointer.
class PendingMessage {
    pub callback: fn(i32)
    pub init(callback: fn(i32)) { this.callback = callback }
}
let PENDING_MESSAGES = new Map<i64, PendingMessage>()

class PendingColor {
    pub initial:  u32
    pub callback: fn(u32, bool)
    pub init(initial: u32, callback: fn(u32, bool)) {
        this.initial  = initial
        this.callback = callback
    }
}
let PENDING_COLORS = new Map<i64, PendingColor>()

class PendingFont {
    pub family:   string
    pub size:     f64
    pub weight:   i32
    pub italic:   bool
    pub callback: fn(string, f64, i32, bool, bool)
    pub init(
        family: string, size: f64, weight: i32, italic: bool,
        callback: fn(string, f64, i32, bool, bool)
    ) {
        this.family   = family
        this.size     = size
        this.weight   = weight
        this.italic   = italic
        this.callback = callback
    }
}
let PENDING_FONTS = new Map<i64, PendingFont>()

fn noOpDismiss() {}
fn noOpConfirm(b: bool) {}

@extern(C) {
    pub fn linux_alert_finished(source: i64, result: i64, data: i64) {
        if let some(pending) = PENDING_ALERTS.get(source) {
            let dlg = source as GtkAlertDialog
            let r = result as GAsyncResult
            let idx = gtk_alert_dialog_choose_finish(dlg, r, 0 as *void)
            PENDING_ALERTS.delete(source)
            if pending.mode == 0 as i32 {
                let cb = pending.onDismiss
                cb()
            } else {
                let cb = pending.onConfirm
                cb(idx == 0 as i32)
            }
        }
    }
    pub fn linux_file_open_finished(source: i64, result: i64, data: i64) {
        if let some(pending) = PENDING_FILES.get(source) {
            let dlg = source as GtkFileDialog
            let r = result as GAsyncResult
            let file = gtk_file_dialog_open_finish(dlg, r, 0 as *void)
            PENDING_FILES.delete(source)
            let path = if file as i64 == 0 as i64 {
                ""
            } else {
                stringFromCstr(g_file_get_path(file))
            }
            let cb = pending.callback
            cb(path)
        }
    }
    pub fn linux_file_save_finished(source: i64, result: i64, data: i64) {
        if let some(pending) = PENDING_FILES.get(source) {
            let dlg = source as GtkFileDialog
            let r = result as GAsyncResult
            let file = gtk_file_dialog_save_finish(dlg, r, 0 as *void)
            PENDING_FILES.delete(source)
            let path = if file as i64 == 0 as i64 {
                ""
            } else {
                stringFromCstr(g_file_get_path(file))
            }
            let cb = pending.callback
            cb(path)
        }
    }

    pub fn linux_message_finished(source: i64, result: i64, data: i64) {
        if let some(pending) = PENDING_MESSAGES.get(source) {
            let dlg = source as GtkAlertDialog
            let r = result as GAsyncResult
            let idx = gtk_alert_dialog_choose_finish(dlg, r, 0 as *void)
            PENDING_MESSAGES.delete(source)
            let cb = pending.callback
            cb(idx)
        }
    }

    pub fn linux_color_finished(source: i64, result: i64, data: i64) {
        if let some(pending) = PENDING_COLORS.get(source) {
            let dlg = source as GtkColorDialog
            let r = result as GAsyncResult
            let rgba = gtk_color_dialog_choose_rgba_finish(dlg, r, 0 as *void)
            PENDING_COLORS.delete(source)
            let cb = pending.callback
            if rgba as i64 == 0 as i64 {
                cb(pending.initial, false)
                return
            }
            let r2 = readF32(rgba as *const void, 0 as i64)
            let g2 = readF32(rgba as *const void, 4 as i64)
            let b2 = readF32(rgba as *const void, 8 as i64)
            let a2 = readF32(rgba as *const void, 12 as i64)
            gdk_rgba_free(rgba)
            let ri = (r2 * 255.0) as u32
            let gi = (g2 * 255.0) as u32
            let bi = (b2 * 255.0) as u32
            let ai = (a2 * 255.0) as u32
            cb((ai << 24) | (ri << 16) | (gi << 8) | bi, true)
        }
    }

    pub fn linux_font_finished(source: i64, result: i64, data: i64) {
        if let some(pending) = PENDING_FONTS.get(source) {
            let dlg = source as GtkFontDialog
            let r = result as GAsyncResult
            let desc = gtk_font_dialog_choose_font_finish(dlg, r, 0 as *void)
            PENDING_FONTS.delete(source)
            let cb = pending.callback
            if desc as i64 == 0 as i64 {
                cb(pending.family, pending.size, pending.weight, pending.italic, false)
                return
            }
            let familyPtr = pango_font_description_get_family(desc)
            let pickedFamily = stringFromCstr(familyPtr)
            let rawSize = pango_font_description_get_size(desc)
            // pango size is in PANGO_SCALE units (1024 per point)
            // when get_size_is_absolute() == 0; treat absolute
            // case as pixels and return that value directly.
            let pickedSize: f64 = if pango_font_description_get_size_is_absolute(desc) == 0 as i32 {
                (rawSize as f64) / 1024.0
            } else {
                rawSize as f64
            }
            let pickedWeight = pango_font_description_get_weight(desc)
            let pangoStyle = pango_font_description_get_style(desc)
            // PangoStyle.normal = 0, oblique = 1, italic = 2; map
            // both slanted styles onto the ilang `italic` bool.
            let pickedItalic = pangoStyle != 0 as i32
            pango_font_description_free(desc)
            cb(pickedFamily, pickedSize, pickedWeight, pickedItalic, true)
        }
    }
}

@extern(C) {

pub fn showAlertDialog(windowHandle: i64, text: string, callback: fn()) {
    if let some(slot) = WINDOWS.get(windowHandle) {
        let d = gtk_alert_dialog_new(cstrFromString(text))
        // `set_buttons` takes a NULL-terminated *const *char in C —
        // ilang's @lib binding declares it as `*const char` so we
        // construct the pointer-array on the C side via the
        // sentinel-terminated string-list helper. For the simple
        // "OK only" case we leave the default buttons alone (GTK
        // shows a single "Close" button).
        PENDING_ALERTS[d as i64] = new PendingAlert(0 as i32, callback, noOpConfirm)
        gtk_alert_dialog_choose(
            d, slot.win, 0 as GCancellable,
            linux_alert_finished, 0 as *void
        )
    }
}

pub fn showConfirmDialog(windowHandle: i64, text: string, callback: fn(bool)) {
    if let some(slot) = WINDOWS.get(windowHandle) {
        let d = gtk_alert_dialog_new(cstrFromString(text))
        // Two-button layout. `set_buttons` would need an array
        // sentinel; skip configuring it here — GTK falls back to
        // a "OK" / "Cancel" pair when buttons aren't set, matching
        // the cocoa default order.
        gtk_alert_dialog_set_cancel_button(d, 1 as i32)
        gtk_alert_dialog_set_default_button(d, 0 as i32)
        PENDING_ALERTS[d as i64] = new PendingAlert(1 as i32, noOpDismiss, callback)
        gtk_alert_dialog_choose(
            d, slot.win, 0 as GCancellable,
            linux_alert_finished, 0 as *void
        )
    }
}

pub fn showOpenFileDialog(windowHandle: i64, callback: fn(string)) {
    if let some(slot) = WINDOWS.get(windowHandle) {
        let d = gtk_file_dialog_new()
        PENDING_FILES[d as i64] = new PendingFile(0 as i32, callback)
        gtk_file_dialog_open(
            d, slot.win, 0 as GCancellable,
            linux_file_open_finished, 0 as *void
        )
    }
}

pub fn showSaveFileDialog(
    windowHandle: i64, suggestedName: string, callback: fn(string)
) {
    if let some(slot) = WINDOWS.get(windowHandle) {
        let d = gtk_file_dialog_new()
        if suggestedName != "" {
            gtk_file_dialog_set_initial_name(d, cstrFromString(suggestedName))
        }
        PENDING_FILES[d as i64] = new PendingFile(1 as i32, callback)
        gtk_file_dialog_save(
            d, slot.win, 0 as GCancellable,
            linux_file_save_finished, 0 as *void
        )
    }
}

pub fn showMessageDialog(
    windowHandle: i64, text: string, buttons: string[], icon: i32,
    callback: fn(i32)
) {
    if let some(slot) = WINDOWS.get(windowHandle) {
        let d = gtk_alert_dialog_new(cstrFromString(text))
        // Build a NULL-terminated array of UTF-8 cstr pointers for
        // `gtk_alert_dialog_set_buttons` (the GIR binding collapses
        // it to `*const char`; the symbol-aliased variant takes
        // the correct pointer-of-pointer shape).
        if buttons.length > 0 {
            let ptrs: i64[] = []
            let i = 0
            while i < buttons.length {
                ptrs.push(cstrFromString(buttons[i]) as i64)
                i = i + 1
            }
            ptrs.push(0 as i64)
            gtk_alert_dialog_set_buttons_array(d, ptrs as *const i64)
        }
        PENDING_MESSAGES[d as i64] = new PendingMessage(callback)
        gtk_alert_dialog_choose(
            d, slot.win, 0 as GCancellable,
            linux_message_finished, 0 as *void
        )
    }
}

pub fn showColorPickerDialog(
    windowHandle: i64, initial: u32, callback: fn(u32, bool)
) {
    if let some(slot) = WINDOWS.get(windowHandle) {
        let d = gtk_color_dialog_new()
        // 4×f32 GdkRGBA — pre-filled with the seed colour so the
        // dialog opens at that value. Pointer stays valid for the
        // sync `_choose_rgba` call (the dialog copies the bytes
        // before the async work starts).
        let mask: u32 = 0xFF as u32
        let r = (((initial >> 16) & mask) as f32) / 255.0 as f32
        let g = (((initial >>  8) & mask) as f32) / 255.0 as f32
        let b = (( initial        & mask) as f32) / 255.0 as f32
        let a = (((initial >> 24) & mask) as f32) / 255.0 as f32
        let buf: f32[] = [r, g, b, a]
        PENDING_COLORS[d as i64] = new PendingColor(initial, callback)
        gtk_color_dialog_choose_rgba(
            d, slot.win, buf as *const f32,
            0 as GCancellable, linux_color_finished, 0 as *void
        )
    }
}

pub fn showFontPickerDialog(
    windowHandle: i64,
    family: string, size: f64, weight: i32, italic: bool,
    callback: fn(string, f64, i32, bool, bool)
) {
    if let some(slot) = WINDOWS.get(windowHandle) {
        let d = gtk_font_dialog_new()
        // Build the initial PangoFontDescription from a CSS-ish
        // string: "<family>[ Bold][ Italic] <size>". Pango parses
        // the trailing number as the point size.
        let styleSuffix = if weight >= 600 as i32 && italic {
            " Bold Italic"
        } elif weight >= 600 as i32 {
            " Bold"
        } elif italic {
            " Italic"
        } else {
            ""
        }
        let descStr = family + styleSuffix + " " + (size as i32).toString()
        let initialDesc = pango_font_description_from_string(cstrFromString(descStr))
        PENDING_FONTS[d as i64] = new PendingFont(
            family, size, weight, italic, callback
        )
        gtk_font_dialog_choose_font(
            d, slot.win, initialDesc,
            0 as GCancellable, linux_font_finished, 0 as *void
        )
        pango_font_description_free(initialDesc)
    }
}

} // @extern(C)
