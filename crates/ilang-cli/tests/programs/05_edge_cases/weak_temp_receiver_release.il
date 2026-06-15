// A fresh weak returned from a call and consumed by `.get()`
// (`mkWeak().get()`) must release its weak share, the same way fresh
// optional / array / string / promise receivers are released after a
// method dispatch. The weak-method dispatch was the one path missing
// that release, so once a weak binding correctly owns a +1 the
// temporary leaked one zombie box per call (invisible to deinit
// counting — the deinit runs; the box memory just never frees).
//
// expect: leakfree=true

use std.test as test

class Tag {
    v: i64
    init(x: i64) { this.v = x }
    deinit() {}
}

fn mkWeak(): Tag.weak {
    let n = new Tag(1)
    let w: Tag.weak = n
    w
}

let base = test.liveAllocBytes()
let i = 0
while i < 300 {
    let _ = mkWeak().get()
    i = i + 1
}
console.log(`leakfree=${(test.liveAllocBytes() - base) < 1024}`)
