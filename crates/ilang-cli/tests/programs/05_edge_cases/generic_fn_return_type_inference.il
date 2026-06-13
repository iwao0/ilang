// A generic fn whose type parameter appears ONLY in the return type
// (`fn makeArr<T>(): T[]`, `fn wrapErr<T>(): Result<T, string>`) can't pin
// that param from its arguments — inference left it `Any`, so the value
// type came back `any[]` / `Result<Any, string>`. A `let xs: i64[] =
// makeArr()` then failed type-checking ("expected i64[], got any[]"), and
// the enum cases reached the monomorphizer as `Type::Any`.
//
// Fix: `refine_fn_call_type_args(call, target)` unifies the fn's declared
// return type against the expected type from context — solving the
// leftover params and updating the stashed type-args — at every position
// the expected type is known: the let annotation, the fn return position,
// and a call argument's param type. Partial inference works too (one param
// fixed by an arg, another by the return).
//
// Heap T flows through ARC-exact (the now-concrete element type drives the
// empty container's element kind).

use std.test as test

let deinits: i64[] = [0]

class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}

enum Maybe<T> {
    some: (T)
    nope
}

fn makeArr<T>(): T[] { [] }
fn makeNope<T>(): Maybe<T> { Maybe.nope }
fn wrapErr<T>(): Result<T, string> { Result.err("e") }
fn pair<A, B>(a: A): (A, B)[] { [] }
fn emptyBoxes<T>(): T[] { [] }

// return position: the tail call's T comes from the declared return type.
fn getInts(): i64[] { makeArr() }

// argument position: the call arg's T comes from the param type.
fn take(xs: i64[]): i64 {
    xs.push(3)
    xs[0]
}

// let annotation, array element from the annotation.
let xs: i64[] = makeArr()
xs.push(5)
xs.push(9)
test.expect(xs[0] + xs[1], 14)

// let annotation, user enum + built-in Result via the return-only param.
let m: Maybe<i64> = makeNope()
let mv = match m {
    some(v) { v }
    nope { 0 - 1 }
}
test.expect(mv, 0 - 1)
let r: Result<i64, string> = wrapErr()
let rv = match r {
    ok(v) { v }
    err(s) { 0 - 2 }
}
test.expect(rv, 0 - 2)

// return + argument positions.
let g = getInts()
g.push(7)
test.expect(g[0], 7)
test.expect(take(makeArr()), 3)

// partial inference: A from the arg (i64), B from the annotation (string).
let ps: (i64, string)[] = pair(5)
ps.push((7, "hi"))
let (pn, pstr) = ps[0]
test.expect(pn + pstr.length, 9)

// Heap T: the empty `T[]` adopts `Box` from the annotation; the pushed
// boxes are freed when the array drops. 2 Box/round.
fn build(): i64 {
    let bs: Box[] = emptyBoxes()
    bs.push(new Box(1))
    bs.push(new Box(2))
    bs[0].n + bs[1].n                        // 1 + 2 = 3
}

test.expect(build(), 3)

let acc: i64[] = [0]
let bd = deinits[0]
let d0 = test.liveAllocBytes()
let i = 0
while i < 100 { acc[0] = acc[0] + build(); i = i + 1 }
test.expect(deinits[0] - bd, 200)            // 2 Box/round x 100
test.expect(test.liveAllocBytes() - d0, 0)
