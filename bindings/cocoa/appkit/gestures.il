// NSGestureRecognizer family + NSTrackingArea + NSPressure
// configuration — view-level mouse / trackpad input.

use foundation { NSObject, NSString, NSPoint, NSRect }
use core { NSView }
use events { NSEvent }

// NSGestureRecognizerState — discriminant on a recognizer's
// `state` property. The runtime drives it through the
// possible → began → changed → (ended | cancelled | failed)
// sequence.
pub enum NSGestureRecognizerState: i64 {
    possible  = 0
    began     = 1
    changed   = 2
    ended     = 3
    cancelled = 4
    failed    = 5
    recognized = 3
}

// NSTrackingAreaOptions — bit flags for which NSResponder
// mouse-tracking events the tracking area subscribes to.
// Combine with `|`. `activeAlways` is the typical choice for
// custom hit-tracking.
@flags
pub enum NSTrackingAreaOptions: u64 {
    mouseEnteredAndExited = 0x01
    mouseMoved            = 0x02
    cursorUpdate          = 0x04
    activeWhenFirstResponder = 0x10
    activeInKeyWindow     = 0x20
    activeInActiveApp     = 0x40
    activeAlways          = 0x80
    assumeInside          = 0x100
    inVisibleRect         = 0x200
    enabledDuringMouseDrag = 0x400
}

@extern(ObjC, "/System/Library/Frameworks/AppKit.framework/AppKit") {
    /// `NSGestureRecognizer` — abstract base for the click /
    /// pan / pinch / rotate / press recognizers below. Construct
    /// the concrete subclass, set target+action, and attach
    /// via `view.addGestureRecognizer(g)`.
    @objc pub class NSGestureRecognizer : NSObject {
        @objc("alloc")
        pub static alloc(): NSGestureRecognizer
        @objc("initWithTarget:action:")
        pub initWithTarget(target: NSObject, action: i64): NSGestureRecognizer

        @objc("setTarget:")
        pub setTarget(t: NSObject)
        @objc("setAction:")
        pub setAction(sel: i64)
        @objc("addTarget:action:")
        pub addTarget(target: NSObject, action: i64)
        @objc("removeTarget:action:")
        pub removeTarget(target: NSObject, action: i64)

        @objc("state")
        pub get state(): NSGestureRecognizerState
        @objc("isEnabled")
        pub get isEnabled(): bool
        @objc("setEnabled:")
        pub set isEnabled(flag: bool)
        @objc("view")
        pub get view(): NSView
        @objc("delaysPrimaryMouseButtonEvents")
        pub get delaysPrimaryMouseButtonEvents(): bool
        @objc("setDelaysPrimaryMouseButtonEvents:")
        pub set delaysPrimaryMouseButtonEvents(flag: bool)

        @objc("locationInView:")
        pub locationInView(v: NSView): NSPoint
    }

    /// `NSClickGestureRecognizer` — fires on a click of
    /// `buttonMask` mouse buttons, optionally requiring N
    /// clicks (double-click etc.).
    @objc pub class NSClickGestureRecognizer : NSGestureRecognizer {
        @objc("buttonMask")
        pub get buttonMask(): u64
        @objc("setButtonMask:")
        pub set buttonMask(m: u64)
        @objc("numberOfClicksRequired")
        pub get numberOfClicksRequired(): i64
        @objc("setNumberOfClicksRequired:")
        pub set numberOfClicksRequired(n: i64)
        @objc("numberOfTouchesRequired")
        pub get numberOfTouchesRequired(): i64
        @objc("setNumberOfTouchesRequired:")
        pub set numberOfTouchesRequired(n: i64)
    }

    /// `NSPanGestureRecognizer` — drag with a configurable
    /// number of touches / mouse buttons. The action method
    /// reads `translationInView:` for the cumulative offset
    /// since the gesture began.
    @objc pub class NSPanGestureRecognizer : NSGestureRecognizer {
        @objc("buttonMask")
        pub get buttonMask(): u64
        @objc("setButtonMask:")
        pub set buttonMask(m: u64)
        @objc("numberOfTouchesRequired")
        pub get numberOfTouchesRequired(): i64
        @objc("setNumberOfTouchesRequired:")
        pub set numberOfTouchesRequired(n: i64)

        @objc("translationInView:")
        pub translationInView(v: NSView): NSPoint
        @objc("setTranslation:inView:")
        pub setTranslation(p: NSPoint, inView: NSView)
        @objc("velocityInView:")
        pub velocityInView(v: NSView): NSPoint
    }

    /// `NSPressGestureRecognizer` — long-press / press-and-hold.
    @objc pub class NSPressGestureRecognizer : NSGestureRecognizer {
        @objc("buttonMask")
        pub get buttonMask(): u64
        @objc("setButtonMask:")
        pub set buttonMask(m: u64)
        @objc("minimumPressDuration")
        pub get minimumPressDuration(): f64
        @objc("setMinimumPressDuration:")
        pub set minimumPressDuration(s: f64)
        @objc("allowableMovement")
        pub get allowableMovement(): f64
        @objc("setAllowableMovement:")
        pub set allowableMovement(d: f64)
        @objc("numberOfTouchesRequired")
        pub get numberOfTouchesRequired(): i64
        @objc("setNumberOfTouchesRequired:")
        pub set numberOfTouchesRequired(n: i64)
    }

    /// `NSMagnificationGestureRecognizer` — pinch-to-zoom on
    /// trackpads. Read `magnification` (a cumulative scale
    /// delta) for the current value.
    @objc pub class NSMagnificationGestureRecognizer : NSGestureRecognizer {
        @objc("magnification")
        pub get magnification(): f64
        @objc("setMagnification:")
        pub set magnification(m: f64)
    }

    /// `NSRotationGestureRecognizer` — two-finger rotate on
    /// trackpads. `rotation` is radians cumulative since the
    /// gesture began.
    @objc pub class NSRotationGestureRecognizer : NSGestureRecognizer {
        @objc("rotation")
        pub get rotation(): f64
        @objc("setRotation:")
        pub set rotation(r: f64)
        @objc("rotationInDegrees")
        pub get rotationInDegrees(): f64
        @objc("setRotationInDegrees:")
        pub set rotationInDegrees(d: f64)
    }

    /// `NSTrackingArea` — defines a region inside an NSView that
    /// generates `mouseEntered:` / `mouseExited:` / `mouseMoved:`
    /// callbacks regardless of mouse-down state. Attach via
    /// `view.addTrackingArea(t)`.
    @objc pub class NSTrackingArea : NSObject {
        @objc("alloc")
        pub static alloc(): NSTrackingArea
        @objc("initWithRect:options:owner:userInfo:")
        pub init(
            rect: NSRect, options: NSTrackingAreaOptions,
            owner: NSObject, userInfo: NSObject
        ): NSTrackingArea

        @objc("rect")
        pub get rect(): NSRect
        @objc("options")
        pub get options(): NSTrackingAreaOptions
        @objc("owner")
        pub get owner(): NSObject
        @objc("userInfo")
        pub get userInfo(): NSObject
    }

    /// `NSPressureConfiguration` — force-touch sensitivity
    /// profile. Attach to a view to opt that view's
    /// `pressureChange` event into the typed pressure stages.
    @objc pub class NSPressureConfiguration : NSObject {
        @objc("alloc")
        pub static alloc(): NSPressureConfiguration
        @objc("initWithPressureBehavior:")
        pub initWithPressureBehavior(behavior: i64): NSPressureConfiguration

        @objc("pressureBehavior")
        pub get pressureBehavior(): i64
        @objc("set")
        pub apply()
    }
}
