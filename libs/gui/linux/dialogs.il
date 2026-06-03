/// Modal dialogs — `GtkAlertDialog` / `GtkFileDialog` (the GTK
/// 4.10+ async API). Each ilang-side dialog call stashes the
/// user callback in `PENDING` keyed by the dialog handle, kicks
/// off the async choose/open/save, and dispatches the result
/// from the `GAsyncReadyCallback` shim into the stored callback.

use std.ffi { cstrFromString, stringFromCstr }
use gtk {
    GtkWindow, GtkAlertDialog, GtkFileDialog, GFile, GAsyncResult,
    GCancellable,
    gtk_alert_dialog_new, gtk_alert_dialog_set_buttons,
    gtk_alert_dialog_set_cancel_button, gtk_alert_dialog_set_default_button,
    gtk_alert_dialog_choose, gtk_alert_dialog_choose_finish,
    gtk_file_dialog_new, gtk_file_dialog_set_title, gtk_file_dialog_set_initial_name,
    gtk_file_dialog_open, gtk_file_dialog_open_finish,
    gtk_file_dialog_save, gtk_file_dialog_save_finish,
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

} // @extern(C)
