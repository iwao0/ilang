// A class field of type `Promise<T>` participates in ARC like any
// other heap field — assigning and reading through `this`, and
// dropping the instance cascade-releases the promise. Confirms
// the cascade table treats KIND_PROMISE field releases correctly.
//
// expect: stored:88
use std.test as test

class Box {
    pending: Promise<i64>
    init(p: Promise<i64>) { this.pending = p }
}

let b = new Box(Promise.resolve(7))
// Re-assign the field — ARC must release the old promise before
// the new pointer lands; reading through `this.pending` after
// must observe the new value.
b.pending = Promise.resolve(88)
b.pending.then(fn(n: i64) {
    console.log("stored:" + n.toString())
})

let _ = 0
