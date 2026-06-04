/// System-tray icon — Linux stub. GTK 4 dropped its native
/// `GtkStatusIcon` API and the recommended successor
/// (`libayatana-appindicator3`, or the `StatusNotifierItem`
/// DBus protocol it speaks) isn't bound here yet, so the
/// linux backend is a no-op: `createNativeTrayIcon` returns
/// `-1` (every public setter then no-ops because the
/// `TrayIcon.handle` guard fails). Round 2 will graduate this
/// to a real appindicator integration.

@extern(C) {

pub fn createNativeTrayIcon(
    iconPath: string, tooltip: string, menuHandle: i64
): i64 {
    -1 as i64
}
pub fn removeNativeTrayIcon(handle: i64) {}
pub fn setTrayIconIcon(handle: i64, path: string) {}
pub fn setTrayIconTooltip(handle: i64, text: string) {}
pub fn setTrayIconTitle(handle: i64, text: string) {}
pub fn setTrayIconMenu(handle: i64, menuHandle: i64) {}
pub fn showTrayIcon(handle: i64, visible: bool) {}
pub fn setOnTrayIconClick(handle: i64, cb: fn()) {}

} // @extern(C)
