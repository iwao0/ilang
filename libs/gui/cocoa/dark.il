/// Light / dark appearance detection + change notification.
///
/// macOS exposes the current setting via `NSUserDefaults`'s
/// `AppleInterfaceStyle` key ("Dark" when dark, missing
/// otherwise — NSUserDefaults returns the empty string for an
/// unset key). The system posts the
/// `AppleInterfaceThemeChangedNotification` distributed
/// notification whenever the user flips System Settings →
/// Appearance, which we forward to gui-core's global signal via
/// `installAppearanceWatcher`.

use cocoa {
    NSObject, NSString, NSUserDefaults, NSDistributedNotificationCenter,
    nsstr, ilstr, nil, selectorPtr,
}

pub fn isOSInDarkMode(): bool {
    let ud = NSUserDefaults.standard()
    let v: NSString = ud.stringForKey(nsstr("AppleInterfaceStyle"))
    if v.handle == 0 as i64 { return false }
    ilstr(v) == "Dark"
}

class DarkModeObserver : NSObject {
    pub appearanceChanged(notification: NSObject) {
        let cb = APPEARANCE_CB[0]
        cb(isOSInDarkMode())
    }
}

let APPEARANCE_INSTALLED: bool[] = [false]
let APPEARANCE_CB: fn(bool)[] = [appearanceNoOp]
/// Retains the `DarkModeObserver` instance so AppKit's
/// NSDistributedNotificationCenter doesn't drop it through ARC —
/// the centre holds a weak ref to its observers, so without this
/// the appearance callback never fires after the registering
/// function returns.
let OBSERVER_HOLD: NSObject[] = []

fn appearanceNoOp(isDark: bool) {}

pub fn installAppearanceWatcher(cb: fn(bool)) {
    APPEARANCE_CB[0] = cb
    if APPEARANCE_INSTALLED[0] { return }
    APPEARANCE_INSTALLED[0] = true
    DarkModeObserver.register()
    let observer = DarkModeObserver.alloc().init()
    OBSERVER_HOLD.push(observer as NSObject)
    let nc = NSDistributedNotificationCenter.defaultCenter()
    // `object: nil` so AppKit's broadcast of
    // `AppleInterfaceThemeChangedNotification` (from any sender)
    // matches — passing an empty NSString would only catch
    // notifications whose sender is exactly that empty string,
    // which never happens.
    nc.addObserverWithSelector(
        observer as NSObject,
        selectorPtr("appearanceChanged:"),
        nsstr("AppleInterfaceThemeChangedNotification"),
        nil
    )
}
