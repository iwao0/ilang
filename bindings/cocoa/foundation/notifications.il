/// User-facing notifications via `NSUserNotificationCenter` —
/// the legacy Banner / Notification Center API. Deprecated in
/// favour of `UNUserNotificationCenter` (UserNotifications.
/// framework) in macOS 11, but still works in non-sandboxed
/// processes without the permission popup that the modern API
/// requires. Use the legacy form here for the common case of a
/// CLI / tooling app that wants to surface a quick banner.

use core { NSObject, NSString, NSDictionary }

/// `NSUserNotificationDefaultSoundName` constant value is the
/// literal string `@"DefaultSoundName"`; pass it to
/// `setSoundName:` for the system "ding". Exposed as an ilang
/// `pub const` rather than dlsym'd at runtime.
pub const NSUserNotificationDefaultSoundName: string = "DefaultSoundName"

@extern(ObjC, "/System/Library/Frameworks/Foundation.framework/Foundation") {
    /// One notification payload — fill via `setTitle:` /
    /// `setInformativeText:` / `setSubtitle:` then hand to the
    /// center for delivery.
    @objc pub class NSUserNotification : NSObject {
        @objc("alloc")
        pub static alloc(): NSUserNotification
        @objc("init")
        pub init(): NSUserNotification

        @objc("setTitle:")
        pub setTitle(s: NSString)
        @objc("setSubtitle:")
        pub setSubtitle(s: NSString)
        @objc("setInformativeText:")
        pub setInformativeText(s: NSString)
        @objc("setSoundName:")
        pub setSoundName(s: NSString)
        @objc("setUserInfo:")
        pub setUserInfo(d: NSDictionary)
        @objc("title")
        pub get title(): NSString
        @objc("informativeText")
        pub get informativeText(): NSString
        @objc("userInfo")
        pub get userInfo(): NSDictionary
    }

    /// Singleton center the running process delivers through.
    /// `setDelegate:` lets the caller listen for activation /
    /// delivery callbacks; the user notification objects
    /// dispatched into `deliverNotification:` end up in the
    /// system Notification Center UI.
    @objc pub class NSUserNotificationCenter : NSObject {
        @objc("defaultUserNotificationCenter")
        pub static defaultUserNotificationCenter(): NSUserNotificationCenter

        @objc("deliverNotification:")
        pub deliverNotification(notification: NSUserNotification)
        @objc("removeAllDeliveredNotifications")
        pub removeAllDeliveredNotifications()
        @objc("setDelegate:")
        pub setDelegate(d: NSObject)
    }
}
