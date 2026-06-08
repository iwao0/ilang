// `match` arms that pattern-bind a heap-typed enum payload must
// not leak the inner cell. Pre-fix, `EnumPayload` codegen
// auto-retained the extracted value — an orphan +1 with no
// matching release — and `lower_enum_ctor` skipped `Enum` /
// `Promise` / `Weak` payload args from its retain set, so a
// recursive `enum Tree { node: (Tree, …) }` payload also
// double-borrowed. Both bugs balanced each other on simple
// shapes (no leak, no UAF) but desynced as soon as one side
// fired alone — e.g. `let b = match c { has(inner) { inner } }`
// kept +1 forever, and removing the codegen retain crashed the
// recursive `Tree` cleanup. This fixture pins the fixed shape:
// extract is a borrow; the let-binding's own retain + cell
// cascade close the loop.
//
// mir-jit only — `liveAllocCount` isn't wired into AOT.
// aot: skip
use std.test as test

class Box {
    n: i64
    init(n: i64) { this.n = n }
}

enum Container {
    empty
    has: (Box)
}

enum Field {
    bare
    boxed: { b: Box }
}

fn round_tuple() {
    let c = Container.has(new Box(7))
    let b = match c {
        empty { new Box(0) }
        has(inner) { inner }
    }
    let _ = b
}

fn round_struct() {
    let c = Field.boxed { b: new Box(9) }
    let b = match c {
        bare { new Box(0) }
        boxed { b: inner } { inner }
    }
    let _ = b
}

fn round_optional() {
    let o: Box? = some(new Box(5))
    let b = match o {
        some(inner) { inner }
        none { new Box(0) }
    }
    let _ = b
}

round_tuple()
round_struct()
round_optional()

let baseline = test.liveAllocCount()
for _ in 0..50 {
    round_tuple()
    round_struct()
    round_optional()
}
test.expect(test.liveAllocCount() - baseline, 0)
