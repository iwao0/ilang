// Two closures built per iteration where the second captures
// the first by name. The capture is a closure pointer, so the
// inner closure has a heap-typed (Closure-kind) capture slot
// that has to retain on construct and release on drop. Without
// the right cascade, every loop body leaks a chain of two
// closure cells. `leak_closure_heap_capture.il` covers an Array
// capture; this fixture adds the Closure-as-capture case.
//

use test

let baseline_b = test.liveAllocBytes()
let baseline_s = test.liveStringCount()

let i: i64 = 0
while i < 200 {
    let inner_factor = i
    let scale = fn(x: i64): i64 { x * inner_factor }
    let compose = fn(x: i64): i64 { scale(x) + 1 }
    let _ = compose(3)
    i = i + 1
}

let after_b = test.liveAllocBytes()
let after_s = test.liveStringCount()
test.expectTrue(after_b - baseline_b < 65536)
test.expectTrue(after_s - baseline_s < 50)
