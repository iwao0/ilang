/// Notification — Win32 backend. Real Win10+ toasts need a
/// registered AppUserModelID + an XML payload built through the
/// WinRT `Windows.UI.Notifications` namespace, which is more
/// COM/WinRT plumbing than we want to take on right now. The
/// legacy `Shell_NotifyIconW` balloon is simpler but requires a
/// persistent tray icon; that conflicts with the `Notification`
/// shape (fire-and-forget, no tray dependency). Round 1 leaves
/// this as a no-op stub so the public API still compiles.

@extern(C) {

pub fn sendNativeNotification(title: string, body: string, iconPath: string): i64 {
    -1 as i64
}
pub fn setOnNotificationClick(handle: i64, cb: fn()) {}

} // @extern(C)
