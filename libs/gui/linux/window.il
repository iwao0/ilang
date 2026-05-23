/// Window lifecycle backed by a plain GtkWindow + GMainLoop.
///
/// We deliberately avoid GtkApplication's activate-driven flow:
/// gui-core's user code creates windows eagerly (before
/// runEventLoop), which conflicts with GtkApplication's contract
/// that windows be built inside an `activate` signal handler. The
/// manual path lets the same eager creation pattern work.

use gtk {
    GtkWidget, GtkWindow, GObject, GtkFixed, GMainLoop,
    gtk_init, gtk_window_new,
    gtk_window_set_title, gtk_window_set_default_size,
    gtk_window_present, gtk_window_set_child, gtk_window_destroy,
    gtk_widget_set_visible, gtk_widget_set_size_request,
    gtk_fixed_new,
    g_main_loop_new, g_main_loop_run,
}
use core { WINDOWS, WindowSlot, GTK_INITED, LOOP_HOLDER }

@extern(C) {

pub fn createNativeWindow(title: string, width: f64, height: f64): i64 {
    if !GTK_INITED[0] {
        gtk_init()
        GTK_INITED[0] = true
    }
    let win = gtk_window_new()
    gtk_window_set_title(win, cstrFromString(title))
    gtk_window_set_default_size(win, width as i32, height as i32)

    // Absolute-positioning container — every widget goes through
    // gtk_fixed_put rather than the default child slot.
    let fixed = gtk_fixed_new()
    gtk_window_set_child(win, fixed as GtkWidget)

    let handle = win as i64
    WINDOWS[handle] = new WindowSlot(win, fixed)
    // TODO: wire `destroy` so onClose dispatches. Blocked on ilang
    //       passing `fn` values as raw C function pointers (the
    //       fn-typed-arg path crashed when GTK invoked the
    //       handler; fnAddr is also unimplemented in MIR).
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
    if !GTK_INITED[0] { return }
    if LOOP_HOLDER.length == 0 {
        LOOP_HOLDER.push(g_main_loop_new(0 as *void, 0 as i32))
    }
    g_main_loop_run(LOOP_HOLDER[0])
}

} // @extern(C)
