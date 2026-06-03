/// Events + input — NSEvent + the NSEventType / NSEventModifierFlags
/// enums.

use foundation { NSObject, NSString, NSPoint, KeyCode }

/// NSEventType — discriminant for what an `NSEvent` carries. Read
/// via `event.eventType()`; common cases are `leftMouseDown` /
/// `keyDown` / `mouseMoved` / `scrollWheel`.
pub enum NSEventType: u64 {
    leftMouseDown     = 1
    leftMouseUp       = 2
    rightMouseDown    = 3
    rightMouseUp      = 4
    mouseMoved        = 5
    leftMouseDragged  = 6
    rightMouseDragged = 7
    mouseEntered      = 8
    mouseExited       = 9
    keyDown           = 10
    keyUp             = 11
    flagsChanged      = 12
    appKitDefined     = 13
    systemDefined     = 14
    applicationDefined = 15
    periodic          = 16
    cursorUpdate      = 17
    scrollWheel       = 22
    otherMouseDown    = 25
    otherMouseUp      = 26
    otherMouseDragged = 27
}

// NSEventModifierFlags — bit field of "control / option / shift
// / command / caps-lock / numpad / fn / help" modifiers held
// during the event. Combine with `|`.
@flags
pub enum NSEventModifierFlags: u64 {
    capsLock = 0x10000
    shift    = 0x20000
    control  = 0x40000
    option   = 0x80000
    command  = 0x100000
    numericPad = 0x200000
    help     = 0x400000
    function = 0x800000
}

/// `NSEventMask` — `1 << NSEventType` bitfield for
/// `addLocalMonitorForEventsMatchingMask:`. Combine with `|`.
/// Only the values libs/gui currently routes through monitor
/// pipelines are exposed; add more as the consumer set grows.
@flags
pub enum NSEventMask: u64 {
    leftMouseDown  = 0x2
    leftMouseUp    = 0x4
    rightMouseDown = 0x8
    rightMouseUp   = 0x10
    keyDown        = 0x400
    keyUp          = 0x800
}

@extern(ObjC, "/System/Library/Frameworks/AppKit.framework/AppKit") {
    /// `NSEvent` — keyboard / mouse / scroll wheel event passed
    /// to `SKScene.mouseDown:` and friends when subclassing for
    /// input. Read the discriminant via `eventType()`, then pull
    /// the type-specific fields (`keyCode`, `characters`,
    /// `locationInWindow`, `deltaX/Y`, `buttonNumber`,
    /// `clickCount`).
    @objc pub class NSEvent : NSObject {
        @objc("type")
        pub get eventType(): NSEventType
        @objc("locationInWindow")
        pub get locationInWindow(): NSPoint
        @objc("modifierFlags")
        pub get modifierFlags(): NSEventModifierFlags
        /// Class-method snapshot of the current modifier-key
        /// state. Useful when surfacing modifiers for an event
        /// we don't directly hold — e.g. an AppKit target/action
        /// callback (`-[id action:]`) carries no `NSEvent`, but
        /// the user's Ctrl / Shift / Alt / Cmd state at the
        /// moment the action fires is still queryable.
        @objc("modifierFlags")
        pub static get currentModifierFlags(): NSEventModifierFlags
        @objc("timestamp")
        pub get timestamp(): f64
        @objc("clickCount")
        pub get clickCount(): i64
        @objc("buttonNumber")
        pub get buttonNumber(): i64
        @objc("deltaX")
        pub get deltaX(): f64
        @objc("deltaY")
        pub get deltaY(): f64
        @objc("deltaZ")
        pub get deltaZ(): f64

        /// Keyboard
        @objc("keyCode")
        pub get keyCode(): KeyCode
        @objc("characters")
        pub get characters(): NSString
        @objc("charactersIgnoringModifiers")
        pub get charactersIgnoringModifiers(): NSString
        @objc("isARepeat")
        pub get isARepeat(): bool
        /// Window the event originated in. `nil` for system-wide
        /// or appKitDefined events (none of which our monitors
        /// currently filter on).
        @objc("window")
        pub get window(): NSObject

        /// Process-local event monitor. `handler` runs before the
        /// matched events are dispatched up the responder chain;
        /// return the same event to forward it, or `nil` (raw `0`)
        /// to swallow it. The returned monitor `id` must be kept
        /// alive — releasing it tears the monitor down.
        @objc("addLocalMonitorForEventsMatchingMask:handler:")
        pub static addLocalMonitorForEventsMatchingMask(
            mask: NSEventMask, handler: ObjCBlock<fn(i64): i64>
        ): NSObject
        /// Removes a previously-installed monitor. Pass the id
        /// returned by the `addLocal…` call.
        @objc("removeMonitor:")
        pub static removeMonitor(monitor: NSObject)
    }
}
