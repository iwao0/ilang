/// Cocoa-side leak smoke test — reads libmalloc's process-wide
/// bytes-in-use before / after a workload to catch leaks the
/// ilang ARC tracker (`liveAllocBytes`) misses.
///
/// Apple's `objc_autorelease`d allocations (ObjC blocks,
/// `[NSString stringWithUTF8String:]` temporaries, etc.) go
/// through libmalloc but get freed by Apple's pool drain, not
/// ilang's `__mir_free`. So `liveAllocBytes` reports them as
/// leaked even when they aren't; the C-side libmalloc counter
/// is what actually shows whether memory grew.
///
/// All workloads are wrapped in `autoreleasepool` so each
/// iteration's autoreleased temporaries drain before the next
/// reads bytes-in-use.

use std.test
use foundation { NSString, NSURL, NSDate, NSNumber, NSMutableArray,
                 NSMutableDictionary, autoreleasepool,
                 nsstr, ilstr }

let iterations = 1000

// ─── Clean workloads — must show ~zero growth ──────────────
//
// Convenience factories return autoreleased objects so the
// pool drains them cleanly. NSString / NSURL / NSDate / NSNumber
// here all use that shape.

let cleanBody = fn() {
    let s = nsstr("hello world")
    let _ = ilstr(s)
    let _ = s.length
    let _ = NSURL.fromString(nsstr("https://example.com"))
    let _ = NSDate.dateWithTimeIntervalSince1970(1.0)
    let _ = NSNumber.fromI64(42)
}
let warm = 0
while warm < 16 { autoreleasepool(cleanBody); warm = warm + 1 }
let base = test.mallocBytesInUse()
let i = 0
while i < iterations { autoreleasepool(cleanBody); i = i + 1 }
let delta = test.mallocBytesInUse() - base
console.log("clean (string/url/date/number) delta:", delta)
// Real leaks would scale ~linearly with iterations; allow a
// generous tolerance for libmalloc bookkeeping noise.
if delta > 16 * 1024 {
    test.fail("clean workload leaked too much heap")
}

// ─── ObjCBlock — create + invoke via the runtime trampoline ─
//
// `new ObjCBlock(closure)` heap-allocates a Block_layout struct
// via __mir_alloc; the dispose_helper drops the closure refcount
// inside the pool drain. The Block_layout buffer itself is freed
// by Apple's pool — libmalloc accounting catches both sides.

// The trampoline closure is built ONCE outside the loop; if
// we constructed it inside the body the ilang ARC tracker
// would show a separate ~16 B/iter closure-allocation leak
// (a language-level pattern, not block-specific).
let doubler = fn(x: i64): i64 { x * 2 }
let blockBody = fn() {
    let b = new ObjCBlock(doubler)
    let _ = b.invoke(7)
}
let w2 = 0
while w2 < 16 { autoreleasepool(blockBody); w2 = w2 + 1 }
let baseB = test.mallocBytesInUse()
let j = 0
while j < iterations { autoreleasepool(blockBody); j = j + 1 }
let deltaB = test.mallocBytesInUse() - baseB
console.log("objc block delta:", deltaB)
if deltaB > 16 * 1024 {
    test.fail("objc block workload leaked too much heap")
}

// ─── NSMutableArray / NSMutableDictionary alloc + init ─────
//
// The `alloc / init` flavour returns a +1 retain that the
// ilang wrapper consumes on scope exit. The wrapper sequence
// (`__wrap_handle` + ilang ARC + `objc_release` in the
// auto-lifted deinit) keeps net retain at zero so the pool
// drain reclaims the underlying ObjC objects without leaking.

let mutableBody = fn() {
    let arr = NSMutableArray.alloc().init()
    arr.addObject(nsstr("a"))
    arr.addObject(nsstr("b"))
    let _ = arr.count
    let d = NSMutableDictionary.alloc().init()
    d.setObject(nsstr("v"), nsstr("k"))
}
let w3 = 0
while w3 < 16 { autoreleasepool(mutableBody); w3 = w3 + 1 }
let baseC = test.mallocBytesInUse()
let k = 0
while k < iterations { autoreleasepool(mutableBody); k = k + 1 }
let deltaC = test.mallocBytesInUse() - baseC
console.log("NSMutable*.alloc().init() delta:", deltaC)
if deltaC > 16 * 1024 {
    test.fail("NSMutable*.alloc().init() workload leaked too much heap")
}

0
