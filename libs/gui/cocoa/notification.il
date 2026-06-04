/// Notification — macOS stub. Real OS-level notifications need
/// a registered bundle identifier (the modern
/// `UNUserNotificationCenter` requires it explicitly; the
/// legacy `NSUserNotificationCenter` silently no-ops without
/// one; `osascript -e 'display notification …'` only delivers
/// when the user has granted notification permission for
/// Script Editor, which a plain CLI process can't request).
///
/// ilang runs as a plain CLI without a bundle ID, so none of
/// those paths work out of the box. Round 1 leaves the macOS
/// backend as a no-op stub; Round 2 will graduate this once we
/// have either an .app-bundle build path or a vendored helper
/// daemon with its own bundle.

@extern(C) {

pub fn sendNativeNotification(title: string, body: string, iconPath: string): i64 {
    -1 as i64
}
pub fn setOnNotificationClick(handle: i64, cb: fn()) {}

} // @extern(C)
