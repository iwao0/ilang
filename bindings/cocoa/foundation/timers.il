// Timer / run-loop / progress bindings. Cross-cutting "things
// that fire over time" — separate from `core` because the block-
// taking factories (`scheduledTimerWithTimeInterval:repeats:block:`,
// etc.) lean on the block helpers and pull in NSDate / NSString
// from siblings.

use core { NSObject, NSString }
use datetime { NSDate }
use concurrency { QualityOfService }

@extern(ObjC, "/System/Library/Frameworks/Foundation.framework/Foundation") {
    /// `NSTimer` — fires a callback after a fixed delay (one-shot)
    /// or every `interval` seconds (`repeats: true`). The
    /// block-based factory is the simplest entry point; the older
    /// target/action shape is also bound for selector-style call
    /// sites. Timers attach themselves to the current run loop's
    /// default mode automatically — use `NSRunLoop.addTimer` to
    /// pin one to a specific mode (e.g. `NSEventTrackingRunLoopMode`
    /// so it keeps firing while a menu is open).
    @objc pub class NSTimer : NSObject {
        @objc("scheduledTimerWithTimeInterval:repeats:block:")
        static _scheduledWithBlock(
            interval: f64, repeats: bool, block: ObjCBlock<fn(i64)>
        ): NSTimer

        @objc("timerWithTimeInterval:repeats:block:")
        static _timerWithBlock(
            interval: f64, repeats: bool, block: ObjCBlock<fn(i64)>
        ): NSTimer

        @objc("scheduledTimerWithTimeInterval:target:selector:userInfo:repeats:")
        pub static scheduledTimerWithSelector(
            interval: f64, target: NSObject, selector: i64,
            userInfo: NSObject, repeats: bool
        ): NSTimer

        @objc("fire")
        pub fire()
        @objc("invalidate")
        pub invalidate()
        @objc("isValid")
        pub get isValid(): bool

        @objc("fireDate")
        pub get fireDate(): NSDate
        @objc("setFireDate:")
        pub set fireDate(d: NSDate)

        @objc("timeInterval")
        pub get timeInterval(): f64
        @objc("tolerance")
        pub get tolerance(): f64
        @objc("setTolerance:")
        pub set tolerance(t: f64)

        @objc("userInfo")
        pub get userInfo(): NSObject

        /// Block-based factory — `body` runs each time the timer
        /// fires, receiving the timer's own `id` so it can call
        /// `.invalidate()` to stop a `repeats: true` schedule.
        /// Returns the scheduled timer; the runtime owns it
        /// (autoreleased), so don't release it manually.
        pub static scheduledTimer(
            interval: f64, repeats: bool, body: fn(i64)
        ): NSTimer {
            NSTimer._scheduledWithBlock(
                interval, repeats, new ObjCBlock(body)
            )
        }

        /// Unscheduled variant — pair with `NSRunLoop.addTimer` to
        /// attach to a specific mode.
        pub static timer(
            interval: f64, repeats: bool, body: fn(i64)
        ): NSTimer {
            NSTimer._timerWithBlock(
                interval, repeats, new ObjCBlock(body)
            )
        }
    }

    /// `NSRunLoop` — the per-thread event loop driver. `main` is
    /// the AppKit-managed main thread's loop (call `app.run()` to
    /// hand control over to it); `current` is whatever thread the
    /// code runs on. Worker threads have to drive their own loop
    /// explicitly via `run` / `runUntilDate:` if they want timers
    /// / source callbacks to fire.
    @objc pub class NSRunLoop : NSObject {
        @objc("currentRunLoop")
        pub static current(): NSRunLoop
        @objc("mainRunLoop")
        pub static main(): NSRunLoop

        @objc("currentMode")
        pub get currentMode(): NSString

        /// Run forever (or until the loop has no sources / timers
        /// to wait on, which never happens for AppKit's main loop).
        /// Returns `void` — control returns only when the loop
        /// decides it has nothing left to do.
        @objc("run")
        pub run()

        /// Run until `limit`. Returns when either `limit` passes
        /// or the loop exhausts its sources; the latter happens
        /// almost immediately on an empty worker-thread loop.
        @objc("runUntilDate:")
        pub runUntilDate(limit: NSDate)

        /// Run one iteration of `mode` until `limit`. Returns
        /// `true` if the loop processed an input source / timer,
        /// `false` on timeout or empty mode.
        @objc("runMode:beforeDate:")
        pub runMode(mode: NSString, beforeDate: NSDate): bool

        @objc("addTimer:forMode:")
        pub addTimer(timer: NSTimer, forMode: NSString)
    }

    /// `NSProgress` — tracks work that completes over time. The
    /// child / parent composition (`becomeCurrent` /
    /// `addChild:withPendingUnitCount:`) lets nested operations
    /// roll up to a single progress reading; `fractionCompleted`
    /// is the aggregated 0..1 value drivers like NSProgressIndicator
    /// observe.
    @objc pub class NSProgress : NSObject {
        @objc("discreteProgressWithTotalUnitCount:")
        pub static discreteProgress(totalUnitCount: i64): NSProgress
        @objc("progressWithTotalUnitCount:")
        pub static progress(totalUnitCount: i64): NSProgress
        @objc("currentProgress")
        pub static currentProgress(): NSProgress

        @objc("totalUnitCount")
        pub get totalUnitCount(): i64
        @objc("setTotalUnitCount:")
        pub set totalUnitCount(v: i64)
        @objc("completedUnitCount")
        pub get completedUnitCount(): i64
        @objc("setCompletedUnitCount:")
        pub set completedUnitCount(v: i64)

        @objc("fractionCompleted")
        pub get fractionCompleted(): f64
        @objc("isFinished")
        pub get isFinished(): bool
        @objc("isCancelled")
        pub get isCancelled(): bool
        @objc("isIndeterminate")
        pub get isIndeterminate(): bool
        @objc("isPaused")
        pub get isPaused(): bool
        @objc("isCancellable")
        pub get isCancellable(): bool
        @objc("isPausable")
        pub get isPausable(): bool

        @objc("cancel")
        pub cancel()
        @objc("pause")
        pub pause()
        @objc("resume")
        pub resume()

        @objc("localizedDescription")
        pub get localizedDescription(): NSString
        @objc("setLocalizedDescription:")
        pub set localizedDescription(s: NSString)
        @objc("localizedAdditionalDescription")
        pub get localizedAdditionalDescription(): NSString
        @objc("setLocalizedAdditionalDescription:")
        pub set localizedAdditionalDescription(s: NSString)

        @objc("becomeCurrentWithPendingUnitCount:")
        pub becomeCurrentWithPendingUnitCount(units: i64)
        @objc("resignCurrent")
        pub resignCurrent()
        @objc("addChild:withPendingUnitCount:")
        pub addChild(child: NSProgress, withPendingUnitCount: i64)
    }

    /// `NSBackgroundActivityScheduler` (macOS) — energy-aware
    /// scheduler for periodic background work (deep crawls,
    /// maintenance jobs). The system fires `scheduleWithBlock:`
    /// when the user is idle / on AC / not running on battery,
    /// instead of strictly on a timer. Identifier should be a
    /// reverse-DNS string unique to this scheduler.
    @objc pub class NSBackgroundActivityScheduler : NSObject {
        @objc("alloc")
        pub static alloc(): NSBackgroundActivityScheduler
        @objc("initWithIdentifier:")
        pub initWithIdentifier(id: NSString): NSBackgroundActivityScheduler

        @objc("identifier")
        pub get identifier(): NSString
        @objc("qualityOfService")
        pub get qualityOfService(): QualityOfService
        @objc("setQualityOfService:")
        pub set qualityOfService(q: QualityOfService)
        @objc("repeats")
        pub get repeats(): bool
        @objc("setRepeats:")
        pub set repeats(v: bool)
        @objc("interval")
        pub get interval(): f64
        @objc("setInterval:")
        pub set interval(t: f64)
        @objc("tolerance")
        pub get tolerance(): f64
        @objc("setTolerance:")
        pub set tolerance(t: f64)
        @objc("shouldDefer")
        pub get shouldDefer(): bool

        @objc("scheduleWithBlock:")
        _scheduleWithBlock(block: ObjCBlock<fn(i64)>)
        /// `scheduleWithBlock:` — register the periodic work
        /// `body`. The argument the system hands you is itself a
        /// `void(^)(NSBackgroundActivityResult)` completion
        /// handler block (raw `id`); invoke it when the work is
        /// done so the system can score the activity. Without
        /// invoking, the scheduler considers the activity still
        /// running and skips the next slot.
        pub scheduleWithBlock(body: fn(i64)) {
            this._scheduleWithBlock(new ObjCBlock(body))
        }
        @objc("invalidate")
        pub invalidate()
    }
}
