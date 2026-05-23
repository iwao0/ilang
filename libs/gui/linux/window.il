/// Window lifecycle backed by GtkApplicationWindow.
///
/// The single process-wide GtkApplication is created lazily on the
/// first `createNativeWindow` call and stored in `APP_HOLDER`.
/// `runEventLoop` enters the application's main loop via
/// `g_application_run` and returns when the last window closes.

use gtk {
    GtkWidget, GtkWindow, GtkApplication, GtkApplicationWindow,
    GApplication, GObject, GtkFixed,
    G_APPLICATION_DEFAULT_FLAGS,
    gtk_application_new, gtk_application_window_new,
    gtk_window_set_title, gtk_window_set_default_size,
    gtk_window_present, gtk_window_set_child, gtk_window_destroy,
    gtk_widget_set_visible, gtk_widget_set_size_request,
    gtk_fixed_new,
    g_application_run, g_signal_connect_data,
}
use core { WINDOWS, WindowSlot, APP_HOLDER }

/// `void on_window_destroy(GtkWindow*, gpointer)` — GTK's
/// `destroy` signal fires after the user closes the window.
/// We look the slot up by the GtkWindow pointer (cast to i64),
/// dispatch the user's onClose, and drop the slot.
@extern(C) {
    pub fn linux_window_destroy(win: i64, data: i64) {
        let h = win as i64
        if let some(slot) = WINDOWS.get(h) {
            let cb = slot.onClose
            cb()
            let _ = WINDOWS.delete(h)
        }
    }
}

@extern(C) {

pub fn createNativeWindow(title: string, width: f64, height: f64): i64 {
    // Lazily create the singleton GtkApplication.
    if APP_HOLDER.length == 0 {
        let app = gtk_application_new(
            cstrFromString("org.ilang.gui"),
            G_APPLICATION_DEFAULT_FLAGS
        )
        APP_HOLDER.push(app)
    }
    let app = APP_HOLDER[0]
    let win = gtk_application_window_new(app)
    gtk_window_set_title(win, cstrFromString(title))
    gtk_window_set_default_size(win, width as i32, height as i32)

    // Absolute-positioning container — every widget goes through
    // gtk_fixed_put rather than the default child slot.
    let fixed = gtk_fixed_new()
    gtk_window_set_child(win, fixed as GtkWidget)

    let handle = win as i64
    WINDOWS[handle] = new WindowSlot(win, fixed)

    // Wire `destroy` so close drives our onClose channel.
    let _ = g_signal_connect_data(
        win as GObject,
        cstrFromString("destroy"),
        linux_window_destroy,
        0 as *void,
        0 as i64,
        0 as u32
    )
    handle
}

pub fn setWindowTitle(handle: i64, title: string) {
    if let some(slot) = WINDOWS.get(handle) {
        gtk_window_set_title(slot.win, cstrFromString(title))
    }
}

pub fn setWindowSize(handle: i64, width: f64, height: f64) {
    if let some(slot) = WINDOWS.get(handle) {
        gtk_window_set_default_size(slot.win, width as i32, height as i32)
    }
}

pub fn showNativeWindow(handle: i64) {
    if let some(slot) = WINDOWS.get(handle) {
        gtk_window_present(slot.win)
    }
}

pub fn hideNativeWindow(handle: i64) {
    if let some(slot) = WINDOWS.get(handle) {
        gtk_widget_set_visible(slot.win as GtkWidget, 0 as i32)
    }
}

pub fn runEventLoop() {
    if APP_HOLDER.length == 0 { return }
    let app = APP_HOLDER[0]
    let _ = g_application_run(app as GApplication, 0 as i32, 0 as *const *const char)
}

} // @extern(C)
