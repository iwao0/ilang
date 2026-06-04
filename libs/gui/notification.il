/// OS notification — fire a banner / toast through the
/// platform's notification system. Not tied to a `Window`, so
/// the same `Notification` can be `show()`n after every UI
/// window has closed (useful for background apps that live in
/// the tray).
///
///   let n = new gui.Notification("Build done", "All tests passed.")
///   n.setIcon("/path/to/icon.png")
///   n.onClick.add(fn(e: gui.MouseEvent) { /* user clicked the banner */ })
///   n.show()
///
/// Backends:
///   - macOS : NSUserNotificationCenter (deprecated but works
///             without a permission prompt).
///   - Win32 : Shell_NotifyIconW with NIF_INFO — pops a balloon
///             from a transient tray icon that auto-removes
///             once the user clicks or the system times it out.
///   - Linux : libnotify (`notify_notification_new` + `show`),
///             linked at JIT load time.

use std.events { Signal }
use events { EventTarget, MouseEvent, Event }
use gui_impl {
    sendNativeNotification, setOnNotificationClick,
}

/// "Closed" event payload — fires after the notification is
/// dismissed (auto-timeout or user-driven). Empty for now;
/// included so subscribers can use the same `Signal<T>` shape
/// as the rest of the library.
pub class NotificationCloseEvent : Event {
    pub init(sender: EventTarget) { super(sender) }
}

pub class Notification : EventTarget {
    pub handle:    i64
    pub _title:    string
    pub _body:     string
    pub _iconPath: string
    pub onClick:   Signal<MouseEvent>
    pub onClose:   Signal<NotificationCloseEvent>

    pub init(title: string, body: string) {
        super()
        this.handle    = -1
        this._title    = title
        this._body     = body
        this._iconPath = ""
        this.onClick   = new Signal<MouseEvent>()
        this.onClose   = new Signal<NotificationCloseEvent>()
    }

    pub setIcon(path: string) {
        this._iconPath = path
    }

    /// Fire the notification. The backend draws the banner; the
    /// click callback fires if / when the user activates it.
    /// Subsequent `show()` calls on the same instance redeliver
    /// the same payload — they are NOT replaced edits.
    pub show() {
        let nHandle = sendNativeNotification(
            this._title, this._body, this._iconPath
        )
        if nHandle >= 0 as i64 {
            this.handle = nHandle
            let signal = this.onClick
            let sender = this
            setOnNotificationClick(nHandle, fn() {
                signal.emit(new MouseEvent(
                    sender, 0.0, 0.0, 0, false, false, false, false
                ))
            })
        }
    }
}
