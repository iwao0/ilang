// Generic class instantiated with a heap-typed T (`Box<i64[]>`).
// The monomorphized class's field has KIND_ARRAY, so the object
// drop path has to cascade-release the inner array. A regression
// in the field-kind threading would leak both the Box object's
// data + the inner array's data. Combined with the `i64[]` data
// buffer, that's ~24 bytes per iter without the cascade.
// `nested_generic.il` covers the functional Box<Box<...>> shape;
// this fixture is the heap-T memory-reclaim variant.
//
// jit: skip

use test

class Box<T> {
    v: T
    init(x: T) { this.v = x }
}

let baseline = test.liveAllocBytes()

let i: i64 = 0
while i < 200 {
    let b = new Box<i64[]>([i, i + 1, i + 2])
    let _ = b.v.length
    i = i + 1
}

let after = test.liveAllocBytes()
test.expectTrue(after - baseline < 65536)
