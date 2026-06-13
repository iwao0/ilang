// Calling a generic fn whose only `T`-bearing argument can't pin `T` —
// `f(Result.err("e"))` against `fn f<T>(r: Result<T, string>)` — left the
// call instantiated as `<Any>`: `Result.err` fixes E but not T, and there
// was no other argument to fix it. `refine_fn_call_type_args` (added for
// return-only params) solved the fn's `T` from the expected type, but the
// INLINE argument `Result.err("e")` kept its own stash at `[Any, string]`
// (it was checked while the param was still `Result<Any, string>`), so the
// argument still lowered as `Type::Any`.
//
// Fix: after solving the fn's type params from context, re-refine each
// argument against the now-concrete param type — the arg shares the fn's
// `T`. This makes the call work wherever an expected type is available: a
// `let` annotation, a fn return position, and a call argument's param type.
//
// (Still unresolved, recorded in HANDOFF: a generic call with NO expected
// type at all and no T-fixing arg, e.g. `match f(Result.err("e")) { ... }`
// as a bare match scrutinee — genuinely ambiguous, needs an annotation.)

use std.test as test

let deinits: i64[] = [0]

class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}

// T appears only in the param; the `Result.err` call can't fix it.
fn f<T>(r: Result<T, string>): Result<T, string> { r }

fn takeR(r: Result<Box, string>): i64 {
    match r {
        ok(b) { b.n }
        err(s) { 0 - 7 }
    }
}

// return position: T comes from the declared return type.
fn viaReturn(): Result<Box, string> {
    f(Result.ok(new Box(6)))
}

// Each ok-call makes a Box (passed through `f`, then freed); the err-call
// makes none. 3 Box/round (Box5 let, Box6 return, Box9 arg).
fn run(): i64 {
    // let annotation
    let x: Result<Box, string> = f(Result.ok(new Box(5)))
    let a = match x {
        ok(b) { b.n }
        err(s) { 0 - 1 }
    }
    // err arg, T solved from the let annotation (no Box)
    let y: Result<Box, string> = f(Result.err("e"))
    let c = match y {
        ok(b) { b.n }
        err(s) { 0 - 2 }
    }
    // return position
    let d = match viaReturn() {
        ok(b) { b.n }
        err(s) { 0 - 3 }
    }
    // argument position
    let e = takeR(f(Result.ok(new Box(9))))
    a + c + d + e                            // 5 - 2 + 6 + 9 = 18
}

test.expect(run(), 18)

let acc: i64[] = [0]
let bd = deinits[0]
let d0 = test.liveAllocBytes()
let i = 0
while i < 100 { acc[0] = acc[0] + run(); i = i + 1 }
test.expect(deinits[0] - bd, 300)            // 3 Box/round x 100
test.expect(test.liveAllocBytes() - d0, 0)
