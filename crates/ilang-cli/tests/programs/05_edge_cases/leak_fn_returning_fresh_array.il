// A function constructs a fresh `i64[]` and returns it. The
// caller binds the return to `arr`, reads `arr.length`, and
// `arr` falls out of scope on the next loop iteration. The
// returned array's data buffer + 48-byte header must drop on
// each iteration. `leak_array_push_grow_loop.il` covers the
// realloc-path leak; this fixture pins down the fresh-return-
// from-fn path which goes through callee-retain and a separate
// caller-side release.
//

use test

fn make_temp(n: i64): i64[] {
    let xs: i64[] = []
    let i: i64 = 0
    while i < 5 {
        xs.push(n + i)
        i = i + 1
    }
    xs
}

let baseline = test.liveAllocBytes()

let i: i64 = 0
while i < 200 {
    let arr = make_temp(i)
    let _ = arr.length
    i = i + 1
}

let after = test.liveAllocBytes()
test.expectTrue(after - baseline < 65536)
