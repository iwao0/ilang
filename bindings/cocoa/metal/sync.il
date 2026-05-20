// Synchronisation + capture:
//   * `MTLEvent` / `MTLSharedEvent` — CPU↔GPU and cross-queue
//     rendezvous primitives.
//   * `MTLCapture{Descriptor,Manager,Scope}` — Xcode GPU frame
//     capture / .gputrace export.
//
// Both groups are diagnostic / glue rather than data-flow, hence
// the shared file.

use foundation { NSObject, NSString, NSError, NSRange, withObjcError, objcRetain }
use types.*

@extern(ObjC, "/System/Library/Frameworks/Metal.framework/Metal") {
    /// `id<MTLEvent>` — opaque, GPU-only synchronisation token
    /// whose value is a 64-bit counter. Use
    /// `commandBuffer.encodeSignalEvent(event, value)` to publish
    /// a new value and `encodeWaitForEvent(event, value)` to
    /// gate later GPU work on the counter reaching that value.
    /// No CPU side — for that, use `MTLSharedEvent`.
    @objc pub class MTLEvent : NSObject {
        @objc("setLabel:")
        pub setLabel(s: NSString)
        @objc("label")
        pub label(): NSString
    }

    /// `id<MTLSharedEvent>` — same counter semantics as
    /// `MTLEvent`, but the CPU can also read (`signaledValue`),
    /// write (`setSignaledValue:`), and wait
    /// (`waitUntilSignaledValue:timeoutMS:`) on it, and can
    /// register a block-based listener for value transitions
    /// (`notifyListener:atValue:block:`).
    ///
    /// Use this to (a) tell the GPU to wait for the CPU
    /// (`encodeWaitForEvent(ev, n)` on the GPU side, then bump
    /// the value from the CPU), or (b) the reverse (GPU signals
    /// via `encodeSignalEvent`, CPU waits via the
    /// `wait…timeoutMS:` selector below). Inherits the GPU-side
    /// signal/wait API by responding to the `MTLEvent`
    /// selectors transparently.
    @objc pub class MTLSharedEvent : MTLEvent {
        @objc("signaledValue")
        pub signaledValue(): u64
        @objc("setSignaledValue:")
        pub setSignaledValue(v: u64)
        @objc("waitUntilSignaledValue:timeoutMS:")
        pub waitUntilSignaledValue(value: u64, timeoutMS: u64): bool
        // The notify listener selector takes an
        // `MTLSharedEventListener` + value + `void(^)(id<MTLSharedEvent>, u64)`
        // block; bind that when there's a real CPU-side handler
        // need — for the casual single-poll case
        // `waitUntilSignaledValue:` is enough.

        // setLabel / label are inherited from MTLEvent.
    }

    /// `MTLCaptureDescriptor` — configures a `MTLCaptureManager`
    /// capture: the object whose work to capture (a device,
    /// command queue, or scope), the destination (Xcode vs a
    /// `.gputrace` file), and an output URL when writing to a
    /// file. Build via `MTLCaptureDescriptor.alloc().init()`.
    @objc pub class MTLCaptureDescriptor : NSObject {
        @objc("alloc")
        pub static alloc(): MTLCaptureDescriptor
        @objc("init")
        pub init(): MTLCaptureDescriptor

        /// The capture target — an `id` that's either an MTLDevice,
        /// an MTLCommandQueue, or an MTLCaptureScope. Bound as
        /// NSObject so any of the three can be passed through.
        @objc("setCaptureObject:")
        pub setCaptureObject(o: NSObject)
        @objc("setDestination:")
        pub setDestination(d: MTLCaptureDestination)
        @objc("setOutputURL:")
        pub setOutputURL(url: NSObject)
    }

    /// `MTLCaptureManager` — process-singleton that drives GPU
    /// frame capture. The casual flow is:
    ///
    /// 1. `mgr = MTLCaptureManager.sharedCaptureManager()`
    /// 2. `desc = MTLCaptureDescriptor.alloc().init()` + setters
    /// 3. `mgr.startCaptureWithDescriptor(desc, errorSlot)`
    /// 4. ...issue GPU work into the captured target...
    /// 5. `mgr.stopCapture()`
    ///
    /// For finer control, mint an `MTLCaptureScope` via
    /// `newCaptureScopeWithDevice:` / `newCaptureScopeWithCommandQueue:`,
    /// bracket the work with `scope.begin/endScope()`, and set the
    /// scope as the descriptor's capture object.
    @objc pub class MTLCaptureManager : NSObject {
        @objc("sharedCaptureManager")
        pub static sharedCaptureManager(): MTLCaptureManager

        @objc("supportsDestination:")
        pub supportsDestination(d: MTLCaptureDestination): bool

        /// Capture lifecycle. `startCaptureWithDescriptor:error:`
        /// takes an `NSError **` slot (raw `i64`) — pair-return
        /// wrapper to follow.
        @objc("startCaptureWithDescriptor:error:")
        pub startCaptureWithDescriptor(
            descriptor: MTLCaptureDescriptor, error: i64
        ): bool
        @objc("stopCapture")
        pub stopCapture()
        @objc("isCapturing")
        pub isCapturing(): bool

        /// Capture-scope factories. Bracket draw work with
        /// `scope.begin/endScope()`; the manager then captures only
        /// that bracket when configured to target the scope.
        @objc("newCaptureScopeWithDevice:")
        pub newCaptureScopeWithDevice(d: MTLDevice): MTLCaptureScope
        @objc("newCaptureScopeWithCommandQueue:")
        pub newCaptureScopeWithCommandQueue(q: MTLCommandQueue): MTLCaptureScope

        @objc("defaultCaptureScope")
        pub defaultCaptureScope(): MTLCaptureScope
        @objc("setDefaultCaptureScope:")
        pub setDefaultCaptureScope(s: MTLCaptureScope)
    }

    /// `id<MTLCaptureScope>` — a named bracket within a stream of
    /// command-buffer commits. When the capture manager targets
    /// the scope, only the work between `beginScope()` and
    /// `endScope()` is recorded. Use this to pin the capture to
    /// "one frame" or "one logical pass".
    @objc pub class MTLCaptureScope : NSObject {
        @objc("beginScope")
        pub beginScope()
        @objc("endScope")
        pub endScope()
        @objc("setLabel:")
        pub setLabel(s: NSString)
        @objc("label")
        pub label(): NSString
    }
}

/// Start a GPU frame capture with `descriptor` and pair the
/// success bool with an `NSError?` populated from the
/// `startCaptureWithDescriptor:error:` out-slot on failure.
/// Follows the same `OrError` pattern as the shader / pipeline
/// constructors above.
pub fn startCaptureOrError(
    mgr: MTLCaptureManager, descriptor: MTLCaptureDescriptor
): (bool, NSError?) {
    let (rc, err) = withObjcError(fn(slot: i64): i64 {
        if mgr.startCaptureWithDescriptor(descriptor, slot) { 1 } else { 0 }
    })
    (rc != 0, err)
}
