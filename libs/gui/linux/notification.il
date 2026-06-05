/// Notification — Linux backend via `libnotify`. The library is
/// commonly installed as `libnotify.so.4` on most desktops; the
/// JIT dlopens it at first use through the `@extern(C, "notify")`
/// declarations below. Round 1 wires summary + body + icon
/// path; the click callback is a no-op because libnotify's
/// only click surface is via "actions" (separate buttons inside
/// the notification body), not the banner itself.

use std.ffi { cstrFromString }

@extern(C, "notify") {
    @handle pub struct NotifyNotification {}

    /// Initialise libnotify with the application name. Idempotent
    /// — repeat calls just re-set the name. Returns non-zero on
    /// success.
    @lib pub fn notify_init(app_name: *const char): i32
    @lib pub fn notify_uninit()
    /// Build a notification. `summary` becomes the headline,
    /// `body` the descriptive text, `icon` an icon name from the
    /// freedesktop icon theme (or a filesystem path).
    @lib pub fn notify_notification_new(
        summary: *const char, body: *const char, icon: *const char
    ): NotifyNotification
    /// Actually display the notification. `error` may be NULL.
    /// Returns non-zero on success.
    @lib pub fn notify_notification_show(
        notification: NotifyNotification, error: *void
    ): i32
}

class NotificationSlot {
    pub onClick: fn()
    pub init() { this.onClick = notifNoOp }
}

fn notifNoOp() {}

let NOTIFICATIONS = new Map<i64, NotificationSlot>()
let NEXT_NOTIF_ID: i64 = 1 as i64
let NOTIFY_INITED: bool = false

@extern(C) {

pub fn sendNativeNotification(title: string, body: string, iconPath: string): i64 {
    if !NOTIFY_INITED {
        NOTIFY_INITED = true
        notify_init(cstrFromString("ilang"))
    }
    let icon: *const char = if iconPath == "" {
        0 as *const char
    } else {
        cstrFromString(iconPath)
    }
    let n = notify_notification_new(
        cstrFromString(title),
        cstrFromString(body),
        icon
    )
    notify_notification_show(n, 0 as *void)
    let id = NEXT_NOTIF_ID
    NEXT_NOTIF_ID = id + 1 as i64
    NOTIFICATIONS[id] = new NotificationSlot()
    id
}

pub fn setOnNotificationClick(handle: i64, cb: fn()) {
    if let some(slot) = NOTIFICATIONS.get(handle) {
        slot.onClick = cb
    }
}

} // @extern(C)
