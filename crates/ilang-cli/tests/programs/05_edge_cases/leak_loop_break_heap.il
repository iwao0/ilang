// `loop { break heap_value }` — the heap value carried out by
// `break v` is the loop expression's result. The result binding
// holds a +1 from the break path; on scope exit the value drops.
// A regression that fails to release the broken-out array would
// leak ~24 bytes + element storage per loop body. Pairs with
// `01_basics/loop_break_value.il` (functional behaviour); this
// fixture pins down the memory reclaim.
//
// jit: skip

use test

let baseline = test.liveAllocBytes()

let i: i64 = 0
while i < 200 {
    let result = loop {
        if i % 2 == 0 {
            break [1, 2, 3]
        } else {
            break [10, 20, 30]
        }
    }
    let _ = result.length
    i = i + 1
}

let after = test.liveAllocBytes()
test.expectTrue(after - baseline < 65536)
