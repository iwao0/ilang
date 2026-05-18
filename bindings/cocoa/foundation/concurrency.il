// Threading and synchronisation bindings — NSThread,
// the lock family (NSLock / NSRecursiveLock / NSCondition /
// NSConditionLock), and the NSOperation tree.
//
// Most of these classes accept a `void(^)(void)` callback (thread
// body, completion block, block-operation body), which routes
// through the runtime's existing `makeBlock` helper. NSThread
// also has a `+detachNewThreadSelector:toTarget:withObject:` form
// for selector-driven dispatch.

use core { NSObject, NSString, NSArray, makeBlock }
use datetime { NSDate }

// NSQualityOfService — scheduler hint for both NSThread and
// NSOperation / NSOperationQueue. Higher numbers ask the system
// to prioritise the work (`userInteractive` > `userInitiated` >
// `utility` > `background`); `default` lets the system pick.
pub enum QualityOfService: i64 {
    userInteractive = 0x21
    userInitiated   = 0x19
    utility         = 0x11
    background      = 0x09
    default         = -1
}

// NSOperationQueuePriority — within a single operation queue,
// ready operations execute in priority order. `normal` is the
// default; lower / higher are best-effort hints.
pub enum OperationQueuePriority: i64 {
    veryLow  = -8
    low      = -4
    normal   = 0
    high     = 4
    veryHigh = 8
}

@extern(ObjC, "/System/Library/Frameworks/Foundation.framework/Foundation") {
    /// `NSThread` — wraps a kernel thread. The block-based
    /// `detachNewThread(body)` is the easiest entry point; for
    /// thread-aware code that needs `cancel` / `isFinished`
    /// observation, `alloc().initWithBlock(...)` returns a paused
    /// thread that `.start()` launches.
    @objc pub class NSThread : NSObject {
        @objc("currentThread")
        pub static current(): NSThread
        @objc("mainThread")
        pub static main(): NSThread
        @objc("isMainThread")
        pub static isMainThread(): bool
        @objc("isMultiThreaded")
        pub static isMultiThreaded(): bool

        @objc("sleepForTimeInterval:")
        pub static sleepForTimeInterval(secs: f64)
        @objc("sleepUntilDate:")
        pub static sleepUntilDate(d: NSDate)
        @objc("exit")
        pub static exit()

        @objc("detachNewThreadSelector:toTarget:withObject:")
        pub static detachNewThreadWithSelector(
            sel: i64, target: NSObject, arg: NSObject
        )

        @objc("detachNewThreadWithBlock:")
        static _detachNewThreadWithBlock(block: i64)
        @objc("initWithBlock:")
        _initWithBlock(block: i64): NSThread
        @objc("alloc")
        pub static alloc(): NSThread

        @objc("start")
        pub start()
        @objc("cancel")
        pub cancel()
        @objc("isExecuting")
        pub get isExecuting(): bool
        @objc("isFinished")
        pub get isFinished(): bool
        @objc("isCancelled")
        pub get isCancelled(): bool

        @objc("name")
        pub get name(): NSString
        @objc("setName:")
        pub set name(s: NSString)
        @objc("qualityOfService")
        pub get qualityOfService(): QualityOfService
        @objc("setQualityOfService:")
        pub set qualityOfService(q: QualityOfService)
        @objc("stackSize")
        pub get stackSize(): u64
        @objc("setStackSize:")
        pub set stackSize(s: u64)

        /// Fire-and-forget: spawn a new thread that runs `body` once
        /// and exits. The thread is auto-released by the system
        /// after the body returns.
        pub static detachNewThread(body: fn()) {
            NSThread._detachNewThreadWithBlock(makeBlock(body))
        }

        /// Construct a `start`-able thread carrying `body`. The
        /// returned thread is in the "not yet started" state —
        /// call `.start()` when you're ready, or set `name` /
        /// `stackSize` / `qualityOfService` first.
        pub static threadWithBody(body: fn()): NSThread {
            NSThread.alloc()._initWithBlock(makeBlock(body))
        }
    }

    /// `NSLock` — basic non-recursive mutex. Calling `lock()` on a
    /// thread that already holds it deadlocks; use NSRecursiveLock
    /// when re-entry might happen.
    @objc pub class NSLock : NSObject {
        @objc("alloc")
        pub static alloc(): NSLock
        @objc("init")
        pub init(): NSLock

        @objc("lock")
        pub lock()
        @objc("unlock")
        pub unlock()
        @objc("tryLock")
        pub tryLock(): bool
        @objc("lockBeforeDate:")
        pub lockBeforeDate(d: NSDate): bool

        @objc("name")
        pub get name(): NSString
        @objc("setName:")
        pub set name(s: NSString)
    }

    /// `NSRecursiveLock` — reentrant mutex. A thread that already
    /// holds the lock can `lock()` again without deadlocking; the
    /// lock counts depth and only releases on a matching number
    /// of `unlock()`s.
    @objc pub class NSRecursiveLock : NSObject {
        @objc("alloc")
        pub static alloc(): NSRecursiveLock
        @objc("init")
        pub init(): NSRecursiveLock

        @objc("lock")
        pub lock()
        @objc("unlock")
        pub unlock()
        @objc("tryLock")
        pub tryLock(): bool
        @objc("lockBeforeDate:")
        pub lockBeforeDate(d: NSDate): bool

        @objc("name")
        pub get name(): NSString
        @objc("setName:")
        pub set name(s: NSString)
    }

    /// `NSCondition` — POSIX-style condition variable bundled
    /// with its own mutex. `lock()` / `wait()` / `signal()` /
    /// `broadcast()` / `unlock()` mirror the pthread API. Always
    /// hold the lock around `wait` / `signal`; the wait releases
    /// the lock atomically and re-acquires it before returning.
    @objc pub class NSCondition : NSObject {
        @objc("alloc")
        pub static alloc(): NSCondition
        @objc("init")
        pub init(): NSCondition

        @objc("lock")
        pub lock()
        @objc("unlock")
        pub unlock()
        @objc("wait")
        pub waitForSignal()
        @objc("waitUntilDate:")
        pub waitUntilDate(d: NSDate): bool
        @objc("signal")
        pub signal()
        @objc("broadcast")
        pub broadcast()

        @objc("name")
        pub get name(): NSString
        @objc("setName:")
        pub set name(s: NSString)
    }

    /// `NSConditionLock` — lock that's only acquirable when the
    /// associated integer "condition" matches the requested
    /// value. `lockWhenCondition(N)` blocks until another thread
    /// `unlockWithCondition(N)`s. Useful for producer/consumer
    /// hand-offs where the condition is the queue depth or a
    /// pipeline stage tag.
    @objc pub class NSConditionLock : NSObject {
        @objc("alloc")
        pub static alloc(): NSConditionLock
        @objc("init")
        pub init(): NSConditionLock
        @objc("initWithCondition:")
        pub initWithCondition(c: i64): NSConditionLock

        @objc("condition")
        pub get condition(): i64

        @objc("lock")
        pub lock()
        @objc("unlock")
        pub unlock()
        @objc("tryLock")
        pub tryLock(): bool
        @objc("lockBeforeDate:")
        pub lockBeforeDate(d: NSDate): bool

        @objc("lockWhenCondition:")
        pub lockWhenCondition(c: i64)
        @objc("lockWhenCondition:beforeDate:")
        pub lockWhenConditionBeforeDate(c: i64, beforeDate: NSDate): bool
        @objc("tryLockWhenCondition:")
        pub tryLockWhenCondition(c: i64): bool
        @objc("unlockWithCondition:")
        pub unlockWithCondition(c: i64)

        @objc("name")
        pub get name(): NSString
        @objc("setName:")
        pub set name(s: NSString)
    }

    /// `NSOperation` — abstract base for cancellable units of
    /// work. Subclass it for custom operations (override `main`)
    /// or use `NSBlockOperation` for block-based one-offs.
    /// `dependencies` define a wait-on-other-operations DAG.
    @objc pub class NSOperation : NSObject {
        @objc("start")
        pub start()
        @objc("main")
        pub main()
        @objc("cancel")
        pub cancel()
        @objc("waitUntilFinished")
        pub waitUntilFinished()

        @objc("isCancelled")
        pub get isCancelled(): bool
        @objc("isExecuting")
        pub get isExecuting(): bool
        @objc("isFinished")
        pub get isFinished(): bool
        @objc("isReady")
        pub get isReady(): bool
        @objc("isAsynchronous")
        pub get isAsynchronous(): bool

        @objc("addDependency:")
        pub addDependency(op: NSOperation)
        @objc("removeDependency:")
        pub removeDependency(op: NSOperation)
        @objc("dependencies")
        pub get dependencies(): NSArray

        @objc("queuePriority")
        pub get queuePriority(): OperationQueuePriority
        @objc("setQueuePriority:")
        pub set queuePriority(p: OperationQueuePriority)
        @objc("qualityOfService")
        pub get qualityOfService(): QualityOfService
        @objc("setQualityOfService:")
        pub set qualityOfService(q: QualityOfService)
        @objc("name")
        pub get name(): NSString
        @objc("setName:")
        pub set name(s: NSString)

        @objc("setCompletionBlock:")
        _setCompletionBlock(block: i64)

        /// Install a completion handler that fires once the
        /// operation finishes (whether cancelled or completed).
        /// Pass `body` once; reassigning replaces the previous
        /// handler.
        pub setCompletion(body: fn()) {
            this._setCompletionBlock(makeBlock(body))
        }
    }

    /// `NSBlockOperation` — bundle one or more blocks into a
    /// runnable operation. `addExecutionBlock(...)` queues
    /// additional blocks that may run concurrently (the operation
    /// reports `isFinished` only after every block returns).
    @objc pub class NSBlockOperation : NSOperation {
        @objc("alloc")
        pub static alloc(): NSBlockOperation
        @objc("init")
        pub init(): NSBlockOperation

        @objc("blockOperationWithBlock:")
        static _blockOperationWithBlock(block: i64): NSBlockOperation
        @objc("addExecutionBlock:")
        _addExecutionBlock(block: i64)

        /// Construct a one-shot block operation. Hand it to an
        /// NSOperationQueue (or call `.start()` directly for an
        /// inline run on the current thread).
        pub static withBlock(body: fn()): NSBlockOperation {
            NSBlockOperation._blockOperationWithBlock(makeBlock(body))
        }

        pub addExecutionBlock(body: fn()) {
            this._addExecutionBlock(makeBlock(body))
        }
    }

    /// `NSOperationQueue` — runs operations on a pool of system-
    /// managed threads. `mainQueue` is the singleton bound to the
    /// main thread (UI work goes through there); user-constructed
    /// queues default to "as many threads as the system thinks is
    /// useful" (`maxConcurrentOperationCount = -1`).
    @objc pub class NSOperationQueue : NSObject {
        @objc("alloc")
        pub static alloc(): NSOperationQueue
        @objc("init")
        pub init(): NSOperationQueue
        @objc("mainQueue")
        pub static mainQueue(): NSOperationQueue
        @objc("currentQueue")
        pub static currentQueue(): NSOperationQueue

        @objc("name")
        pub get name(): NSString
        @objc("setName:")
        pub set name(s: NSString)

        @objc("maxConcurrentOperationCount")
        pub get maxConcurrentOperationCount(): i64
        @objc("setMaxConcurrentOperationCount:")
        pub set maxConcurrentOperationCount(n: i64)
        @objc("qualityOfService")
        pub get qualityOfService(): QualityOfService
        @objc("setQualityOfService:")
        pub set qualityOfService(q: QualityOfService)

        @objc("isSuspended")
        pub get isSuspended(): bool
        @objc("setSuspended:")
        pub set isSuspended(v: bool)
        @objc("operationCount")
        pub get operationCount(): i64
        @objc("operations")
        pub get operations(): NSArray

        @objc("addOperation:")
        pub addOperation(op: NSOperation)
        @objc("addOperations:waitUntilFinished:")
        pub addOperations(ops: NSArray, waitUntilFinished: bool)
        @objc("addOperationWithBlock:")
        _addOperationWithBlock(block: i64)
        @objc("cancelAllOperations")
        pub cancelAllOperations()
        @objc("waitUntilAllOperationsAreFinished")
        pub waitUntilAllOperationsAreFinished()

        /// Enqueue a block-only operation. Equivalent to
        /// `addOperation(NSBlockOperation.withBlock(body))` but
        /// skips the wrapper allocation.
        pub addBlock(body: fn()) {
            this._addOperationWithBlock(makeBlock(body))
        }
    }
}
