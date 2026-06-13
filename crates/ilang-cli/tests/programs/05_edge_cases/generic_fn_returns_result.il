// Built-in `Result<T, E>` is monomorphized per call site (it has no source
// enum decl). `monomorphize_fns`, when specializing a generic fn body,
// re-mangles each generic-enum `EnumCtor.enum_name` to its concrete form
// (`Result.ok` -> `Result<i64, string>.ok`) — but it built its set of
// "which enum names are generic" only from program-declared enums, so the
// built-in `Result` was absent. A generic fn that constructs a `Result`
// (`fn wrapOk<T>(x: T): Result<T, string> { Result.ok(x) }`) kept
// `enum_name="Result"` in the specialized body and MIR lower failed with
// "unknown enum Result". A user-declared generic enum (`Maybe<T>`) worked
// because it WAS in that set.
//
// Fix: seed `monomorphize_fns`'s generic-enum set with the `Result`
// template (the `monomorphize_enums` pass already did the same). Now a
// generic fn can build / return a `Result` whose type params are fixed by
// its arguments. Heap payloads flow through and ARC stays exact, including
// the err path that discards a heap argument.
//
// (Separate, pre-existing limitation, NOT covered here: a generic fn whose
// type param is determined ONLY by the call's return annotation — e.g.
// `let r: Result<i64,string> = wrapErr()` with `fn wrapErr<T>(): Result<T,
// string>` — still leaves T unresolved. That gap is general to generic-fn
// return-type inference, affecting `T[]` and user enums alike, not Result.)

use std.test as test

let deinits: i64[] = [0]

class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}

// T fixed by the ok arg.
fn wrapOk<T>(x: T): Result<T, string> {
    Result.ok(x)
}

// E fixed by the err arg; the i64 ok type is concrete.
fn wrapErrE<E>(e: E): Result<i64, E> {
    Result.err(e)
}

// T fixed by an arg, both arms exercised; the err arm drops `x`.
fn pick<T>(x: T, ok: bool): Result<T, string> {
    if ok {
        Result.ok(x)
    } else {
        Result.err("no")
    }
}

// Value-level checks across non-heap instantiations.
let okVal = match wrapOk(5) {
    ok(v) { v }
    err(s) { 0 - 1 }
}
test.expect(okVal, 5)
let errVal = match wrapErrE("boom") {
    ok(v) { v }
    err(s) { 0 - 2 }
}
test.expect(errVal, 0 - 2)

// Heap payload through the generic Result. 3 Box/round:
//   wrapOk(Box) ok + pick(Box,true) ok + pick(Box,false) drops its arg.
fn run(): i64 {
    let a = match wrapOk(new Box(3)) {
        ok(b) { b.n }
        err(s) { 0 - 1 }
    }
    let c = match pick(new Box(4), true) {
        ok(b) { b.n }
        err(s) { 0 - 1 }
    }
    let d = match pick(new Box(5), false) {
        ok(b) { b.n }
        err(s) { 0 - 2 }
    }
    a + c + d                                // 3 + 4 - 2 = 5
}

test.expect(run(), 5)

let acc: i64[] = [0]
let bd = deinits[0]
let d0 = test.liveAllocBytes()
let i = 0
while i < 100 { acc[0] = acc[0] + run(); i = i + 1 }
test.expect(deinits[0] - bd, 300)            // 3 Box/round x 100
test.expect(test.liveAllocBytes() - d0, 0)
