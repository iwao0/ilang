/// ActivityIndicator — stub. GTK has `GtkSpinner`
/// (`gtk_spinner_new` / `gtk_spinner_start` / `gtk_spinner_stop`)
/// which would map cleanly, but Round 1 ships the macOS path
/// only; the GTK widget is left for a follow-up. Functions are
/// present so cross-platform consumer code links and runs — the
/// returned handle is `-1` and every setter is a no-op.

@extern(C) {

pub fn createNativeActivityIndicator(
    windowHandle: i64,
    x: f64, y: f64, width: f64, height: f64,
    visible: bool
): i64 {
    -1 as i64
}

pub fn startActivityIndicator(handle: i64) {}
pub fn stopActivityIndicator(handle: i64) {}
pub fn setActivityIndicatorFrame(handle: i64, x: f64, y: f64, width: f64, height: f64) {}
pub fn setActivityIndicatorVisible(handle: i64, visible: bool) {}
pub fn setActivityIndicatorTooltip(handle: i64, text: string) {}

} // @extern(C)
