// A class field of type `Promise<T>` participates in ARC like any
// other heap field — assigning and reading through `this`, and
// dropping the instance cascade-releases the promise. Confirms
// the cascade table treats KIND_PROMISE field releases correctly.
//
// expect: stored:7
// expect: stored:88
use std.test as test

class Box {
    pending: Promise<i64>
    init(p: Promise<i64>) { this.pending = p }
    chain(label: string) {
        this.pending.then(fn(n: i64) {
            console.log(label + ":" + n.toString())
        })
    }
}

let b1 = new Box(Promise.resolve(7))
b1.chain("stored")

// Re-assign the field — the old promise should be released by
// ARC before the new pointer lands.
let b2 = new Box(Promise.resolve(0))
b2.pending = Promise.resolve(88)
b2.chain("stored")

let _ = 0
